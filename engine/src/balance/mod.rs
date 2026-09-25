mod wal;

use std::collections::HashMap;
use std::time::Duration;

use bigdecimal::{BigDecimal, Zero};
use uuid::Uuid;

use crate::db::rows::{BalanceRecord, IdempotencyRecord};
use crate::db::{DbPool, query, store};
use crate::error::AppError;
use crate::queue::EngineQueue;
use crate::types::{Asset, BalanceLine, Command, Outcome, fmt_amount};

use wal::{Wal, WalRecord};

/// A mutation awaiting the next group-commit flush (WAL fsync). Its result is
/// published and its command acked only after the batch is durable.
struct PendingAck {
    record: BalanceRecord,
    reply_key: String,
    /// Redis stream entry id, acked after fsync (or immediately for dupes).
    cmd_id: String,
    idempotency_key: Option<String>,
    /// Deduplicated at the pending level (same key already queued this batch).
    duplicate: bool,
}

/// (user_id, client-key) -> already-computed resulting amount. Makes retries
/// return the stored answer instead of re-applying (no double credit). Seeded
/// from the durable `idempotency_keys` table at boot so restarts stay safe.
#[derive(Default)]
pub(crate) struct Dedupe(HashMap<(Uuid, String), BigDecimal>);

/// Recover + seed: replay the WAL into Postgres, truncate it, and load all
/// balances and idempotency answers into memory. Runs once before the worker
/// accepts traffic.
pub async fn init(
    pool: &DbPool,
    wal_path: &str,
) -> Result<(HashMap<(Uuid, Asset), BigDecimal>, Dedupe), AppError> {
    let wal = Wal::open(wal_path).await.map_err(|e| {
        eprintln!("engine: failed to open WAL: {e}");
        AppError::Internal
    })?;

    let leftover = wal.read_all().await.map_err(|e| {
        eprintln!("engine: failed to read WAL: {e}");
        AppError::Internal
    })?;
    if !leftover.is_empty() {
        // Replay journals balances AND their idempotency keys in one
        // transaction, so the reclaim path below can short-circuit redelivered
        // commands instead of applying them a second time.
        let records: Vec<BalanceRecord> = leftover.iter().map(|w| w.record.clone()).collect();
        let keys: Vec<IdempotencyRecord> = leftover
            .iter()
            .filter_map(|w| {
                w.idempotency_key.as_ref().map(|k| IdempotencyRecord {
                    user_id: w.record.user_id,
                    key: k.clone(),
                    amount: w.record.amount.clone(),
                })
            })
            .collect();
        let mut conn = pool.get().await?;
        store::checkpoint(&mut conn, &records, &keys).await?;
        wal.truncate().await.map_err(|e| {
            eprintln!("engine: failed to truncate WAL after replay: {e}");
            AppError::Internal
        })?;
    }

    let mut conn = pool.get().await?;
    let mut balances = HashMap::new();
    for row in query::load_all_balances(&mut conn).await? {
        if let Some(asset) = Asset::parse(&row.asset) {
            balances.insert((row.user_id, asset), row.amount);
        }
    }

    let mut dedupe = Dedupe::default();
    for row in query::load_idempotency_keys(&mut conn).await? {
        dedupe.0.insert((row.user_id, row.key), row.amount);
    }

    Ok((balances, dedupe))
}

/// Event loop: consume commands from the ingest task's channel, process them
/// sequentially, and periodically fsync the WAL + checkpoint it into Postgres.
/// The Redis blocking read lives in the ingest task (never cancelled), so a
/// flush tick can never strand a delivered command.
pub async fn run(
    mut rx: tokio::sync::mpsc::Receiver<(String, Command)>,
    queue: EngineQueue,
    pool: DbPool,
    mut balances: HashMap<(Uuid, Asset), BigDecimal>,
    mut dedupe: Dedupe,
    wal_path: String,
    flush_interval: Duration,
) {
    let wal = match Wal::open(&wal_path).await {
        Ok(w) => w,
        Err(e) => {
            eprintln!("engine: failed to open WAL: {e}");
            return;
        }
    };

    let mut pending: Vec<PendingAck> = Vec::new();
    let mut ticker = tokio::time::interval(flush_interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            maybe = rx.recv() => {
                match maybe {
                    Some((cmd_id, cmd)) => {
                        handle(&queue, cmd_id, cmd, &mut balances, &mut pending, &mut dedupe).await;
                    }
                    None => break, // ingest task dropped all senders -> shutdown
                }
            }
            _ = ticker.tick() => {
                flush(&pool, &wal, &queue, &mut pending, &mut dedupe).await;
            }
        }
    }
}

async fn handle(
    queue: &EngineQueue,
    cmd_id: String,
    cmd: Command,
    balances: &mut HashMap<(Uuid, Asset), BigDecimal>,
    pending: &mut Vec<PendingAck>,
    dedupe: &mut Dedupe,
) {
    match cmd {
        Command::Ping { reply_key } => {
            let _ = queue.publish(&Outcome::Pong { reply_key }).await;
            let _ = queue.ack(cmd_id).await;
        }
        Command::GetBalances { reply_key, user_id } => {
            let rows = Asset::ALL
                .iter()
                .map(|asset| {
                    let amount = balances
                        .get(&(user_id, *asset))
                        .cloned()
                        .unwrap_or_else(BigDecimal::zero);
                    BalanceLine {
                        asset: asset.as_str().to_string(),
                        amount: fmt_amount(&amount),
                    }
                })
                .collect();
            let _ = queue
                .publish(&Outcome::BalancesResult {
                    reply_key,
                    balances: rows,
                })
                .await;
            let _ = queue.ack(cmd_id).await;
        }
        Command::Deposit {
            reply_key,
            user_id,
            asset,
            amount,
            idempotency_key,
        } => {
            // Defense in depth: the engine's own contract requires positive
            // amounts, not just the backend's validation. A direct stream
            // producer must not be able to mint or burn balances.
            if amount <= BigDecimal::zero() {
                let _ = queue
                    .publish(&Outcome::Error {
                        reply_key,
                        error: "amount must be positive".to_string(),
                    })
                    .await;
                let _ = queue.ack(cmd_id).await;
                return;
            }

            if let Some(key) = &idempotency_key {
                // Seen before and already durable: return the stored answer.
                if let Some(prev) = dedupe.0.get(&(user_id, key.clone())) {
                    let _ = queue
                        .publish(&Outcome::DepositResult {
                            reply_key,
                            asset: asset.as_str().to_string(),
                            amount: fmt_amount(prev),
                        })
                        .await;
                    let _ = queue.ack(cmd_id).await;
                    return;
                }
                // Same logical deposit already queued this batch: no re-apply.
                // Keys are scoped per user, so two users may reuse one key
                // without stealing each other's credit.
                if let Some(existing) = pending.iter().find(|p| {
                    !p.duplicate
                        && p.record.user_id == user_id
                        && p.idempotency_key.as_deref() == Some(key.as_str())
                }) {
                    pending.push(PendingAck {
                        record: existing.record.clone(),
                        reply_key,
                        cmd_id,
                        idempotency_key: None,
                        duplicate: true,
                    });
                    return;
                }
            }

            let entry = balances
                .entry((user_id, asset))
                .or_insert_with(BigDecimal::zero);
            *entry += amount;
            let new_amount = entry.clone();
            pending.push(PendingAck {
                record: BalanceRecord {
                    user_id,
                    asset,
                    amount: new_amount,
                },
                reply_key,
                cmd_id,
                idempotency_key,
                duplicate: false,
            });
        }
    }
}

/// Group commit: write + fsync the WAL, then publish results and ack commands.
/// Only after the batch is on disk does the client hear success.
async fn flush(
    pool: &DbPool,
    wal: &Wal,
    queue: &EngineQueue,
    pending: &mut Vec<PendingAck>,
    dedupe: &mut Dedupe,
) {
    if pending.is_empty() {
        return;
    }

    // Journal the FULL picture (resulting balances + their idempotency keys)
    // and fsync before the client hears anything. If we crash between this
    // fsync and the ack, recovery replays these records into Postgres AND the
    // dedupe map, so the re-delivered command short-circuits instead of
    // applying twice.
    let records: Vec<WalRecord> = pending
        .iter()
        .map(|p| WalRecord {
            record: p.record.clone(),
            idempotency_key: p.idempotency_key.clone(),
        })
        .collect();

    // Durability point: only after the batch is on disk do we ack.
    if wal.append_all(&records).await.is_err() || wal.sync().await.is_err() {
        eprintln!("engine: wal fsync failed; retrying next tick");
        return;
    }

    let n = pending.len();
    for p in pending.drain(..) {
        let amount = fmt_amount(&p.record.amount);
        let _ = queue
            .publish(&Outcome::DepositResult {
                reply_key: p.reply_key,
                asset: p.record.asset.as_str().to_string(),
                amount: amount.clone(),
            })
            .await;
        let _ = queue.ack(p.cmd_id).await;
        if let (Some(key), false) = (&p.idempotency_key, p.duplicate) {
            dedupe
                .0
                .insert((p.record.user_id, key.clone()), p.record.amount.clone());
        }
    }

    // Read the whole journal back: whatever survived a previously failed
    // checkpoint plus this batch. Checkpoint it as ONE picture — the final
    // resulting balances AND the matching idempotency answers, atomically.
    // Deriving the keys from the journal (not the drained batch) means a retry
    // after a failed checkpoint never drops an older record's key. The
    // checkpoint collapses per-key duplicates, satisfying Postgres'
    // one-row-per-conflict-key rule. On failure the log is retained and
    // retried next tick (replay is idempotent), so a crash can never split
    // balances from their keys.
    let to_flush = match wal.read_all().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("engine: failed reading WAL: {e}");
            return;
        }
    };
    if to_flush.is_empty() {
        return;
    }
    let balances_to_flush: Vec<BalanceRecord> = to_flush.iter().map(|w| w.record.clone()).collect();
    let keys_to_flush: Vec<IdempotencyRecord> = to_flush
        .iter()
        .filter_map(|w| {
            w.idempotency_key.as_ref().map(|k| IdempotencyRecord {
                user_id: w.record.user_id,
                key: k.clone(),
                amount: w.record.amount.clone(),
            })
        })
        .collect();

    let mut conn = match pool.get().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("engine: pool error on flush: {e}");
            return;
        }
    };
    match store::checkpoint(&mut conn, &balances_to_flush, &keys_to_flush).await {
        Ok(()) => {
            if let Err(e) = wal.truncate().await {
                eprintln!("engine: failed truncating WAL: {e}");
            }
            eprintln!("engine: flushed {n} mutation(s) to Postgres");
        }
        Err(e) => {
            eprintln!("engine: db checkpoint failed: {e}");
        }
    }
}
