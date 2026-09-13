mod wal;

use std::collections::HashMap;
use std::time::Duration;

use bigdecimal::BigDecimal;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::db::rows::BalanceRecord;
use crate::db::{query, store, DbPool};
use crate::engine::types::Asset;
use crate::error::AppError;

use wal::Wal;

/// Commands accepted by the balance worker.
pub enum BalanceCmd {
    Deposit {
        user_id: Uuid,
        asset: Asset,
        amount: BigDecimal,
        reply: oneshot::Sender<Result<BigDecimal, AppError>>,
    },
    GetBalances {
        user_id: Uuid,
        reply: oneshot::Sender<Result<Vec<(Asset, BigDecimal)>, AppError>>,
    },
}

/// Clonable handle appending to the single balance worker. No locks are used:
/// the worker is the only owner of the balances map.
#[derive(Clone)]
pub struct BalanceHandle {
    tx: mpsc::Sender<BalanceCmd>,
}

impl BalanceHandle {
    pub fn new(tx: mpsc::Sender<BalanceCmd>) -> Self {
        Self { tx }
    }

    pub async fn deposit(
        &self,
        user_id: Uuid,
        asset: Asset,
        amount: BigDecimal,
    ) -> Result<BigDecimal, AppError> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(BalanceCmd::Deposit {
                user_id,
                asset,
                amount,
                reply: tx,
            })
            .await
            .map_err(|_| AppError::Internal)?;
        rx.await.map_err(|_| AppError::Internal)?
    }

    pub async fn get_balances(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<(Asset, BigDecimal)>, AppError> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(BalanceCmd::GetBalances { user_id, reply: tx })
            .await
            .map_err(|_| AppError::Internal)?;
        rx.await.map_err(|_| AppError::Internal)?
    }
}

/// A mutation whose ack is deferred until the WAL has been fsynced (group commit),
/// so a confirmed deposit is guaranteed durable.
struct PendingAck {
    record: BalanceRecord,
    reply: oneshot::Sender<Result<BigDecimal, AppError>>,
}

/// Recover + seed: replay the WAL into Postgres, truncate it, and load all
/// balances into memory. Runs once before the worker accepts traffic.
pub async fn init(
    pool: &DbPool,
    wal_path: &str,
) -> Result<HashMap<(Uuid, Asset), BigDecimal>, AppError> {
    let wal = Wal::open(wal_path).await.map_err(|e| {
        eprintln!("balance: failed to open WAL: {e}");
        AppError::Internal
    })?;

    let leftover = wal.read_all().await.map_err(|e| {
        eprintln!("balance: failed to read WAL: {e}");
        AppError::Internal
    })?;
    if !leftover.is_empty() {
        let mut conn = pool.get().await?;
        store::upsert_balances(&mut conn, &leftover).await?;
        wal.truncate().await.map_err(|e| {
            eprintln!("balance: failed to truncate WAL after replay: {e}");
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

    Ok(balances)
}

/// Event loop: process balance commands sequentially and periodically fsync the
/// WAL + checkpoint it into Postgres.
pub async fn run(
    mut rx: mpsc::Receiver<BalanceCmd>,
    pool: DbPool,
    mut balances: HashMap<(Uuid, Asset), BigDecimal>,
    wal_path: String,
    flush_interval: Duration,
) {
    let wal = match Wal::open(&wal_path).await {
        Ok(w) => w,
        Err(e) => {
            eprintln!("balance: failed to open WAL: {e}");
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
                    Some(cmd) => handle(cmd, &mut balances, &mut pending),
                    None => break, // all senders dropped -> shutdown
                }
            }
            _ = ticker.tick() => {
                flush(&pool, &wal, &mut pending).await;
            }
        }
    }
}

fn handle(cmd: BalanceCmd, balances: &mut HashMap<(Uuid, Asset), BigDecimal>, pending: &mut Vec<PendingAck>) {
    match cmd {
        BalanceCmd::Deposit {
            user_id,
            asset,
            amount,
            reply,
        } => {
            let entry = balances
                .entry((user_id, asset))
                .or_insert_with(|| BigDecimal::from(0));
            *entry += amount;
            let new_amount = entry.clone();
            pending.push(PendingAck {
                record: BalanceRecord {
                    user_id,
                    asset,
                    amount: new_amount.clone(),
                },
                reply,
            });
        }
        BalanceCmd::GetBalances { user_id, reply } => {
            let rows = Asset::ALL
                .iter()
                .map(|asset| {
                    let amount = balances
                        .get(&(user_id, *asset))
                        .cloned()
                        .unwrap_or_else(|| BigDecimal::from(0));
                    (*asset, amount)
                })
                .collect();
            let _ = reply.send(Ok(rows));
        }
    }
}

async fn flush(pool: &DbPool, wal: &Wal, pending: &mut Vec<PendingAck>) {
    if pending.is_empty() {
        return;
    }

    let records: Vec<BalanceRecord> = pending.iter().map(|p| p.record.clone()).collect();

    // Durability point: only after the batch is on disk do we ack.
    if wal.append_all(&records).await.is_err() || wal.sync().await.is_err() {
        eprintln!("balance: wal fsync failed; retrying next tick");
        return;
    }

    for p in pending.drain(..) {
        let _ = p.reply.send(Ok(p.record.amount.clone()));
    }

    // Checkpoint the WAL into Postgres; on failure the log is retained and
    // retried next tick (replay is idempotent).
    let to_flush = match wal.read_all().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("balance: failed reading WAL: {e}");
            return;
        }
    };
    if to_flush.is_empty() {
        return;
    }

    let mut conn = match pool.get().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("balance: pool error on flush: {e}");
            return;
        }
    };
    match store::upsert_balances(&mut conn, &to_flush).await {
        Ok(()) => {
            if let Err(e) = wal.truncate().await {
                eprintln!("balance: failed truncating WAL: {e}");
            }
        }
        Err(e) => {
            eprintln!("balance: db checkpoint failed: {e}");
        }
    }
}