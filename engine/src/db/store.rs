use std::collections::HashMap;

use diesel::upsert::excluded;
use diesel::{ExpressionMethods, insert_into};
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
use uuid::Uuid;

use crate::error::AppError;
use crate::types::Asset;

use super::rows::{BalanceRecord, IdempotencyRecord};

/// A flush batch and a WAL replay can hold multiple records for the same key
/// (a failed checkpoint keeps the old log next to the new batch). Postgres
/// rejects a single `ON CONFLICT DO UPDATE` that names a conflict key more
/// than once, so reduce to one row per key. Records carry RESULTING amounts
/// (absolute state), so the last record per key is the current truth.
fn collapse_balances(rows: &[BalanceRecord]) -> Vec<BalanceRecord> {
    let mut out: Vec<BalanceRecord> = Vec::with_capacity(rows.len());
    let mut index: HashMap<(Uuid, Asset), usize> = HashMap::new();
    for r in rows {
        match index.get(&(r.user_id, r.asset)) {
            Some(i) => out[*i] = r.clone(),
            None => {
                index.insert((r.user_id, r.asset), out.len());
                out.push(r.clone());
            }
        }
    }
    out
}

fn collapse_keys(rows: &[IdempotencyRecord]) -> Vec<IdempotencyRecord> {
    let mut out: Vec<IdempotencyRecord> = Vec::with_capacity(rows.len());
    let mut index: HashMap<(Uuid, String), usize> = HashMap::new();
    for r in rows {
        match index.get(&(r.user_id, r.key.clone())) {
            Some(i) => out[*i] = r.clone(),
            None => {
                index.insert((r.user_id, r.key.clone()), out.len());
                out.push(r.clone());
            }
        }
    }
    out
}

/// Checkpoint a flush batch: write balance mutations and the batch's idempotency
/// answers in ONE transaction. Atomicity means a crash can never persist a
/// balance without its key (a retry would re-apply) or vice-versa. Used both by
/// the live flusher and by WAL replay at boot.
pub async fn checkpoint(
    conn: &mut AsyncPgConnection,
    balances: &[BalanceRecord],
    keys: &[IdempotencyRecord],
) -> Result<(), AppError> {
    let balances = collapse_balances(balances);
    let keys = collapse_keys(keys);

    conn.transaction(|conn| {
        Box::pin(async move {
            if !balances.is_empty() {
                use crate::schema::balances;

                insert_into(balances::table)
                    .values(
                        balances
                            .iter()
                            .map(|r| {
                                (
                                    balances::user_id.eq(r.user_id),
                                    balances::asset.eq(r.asset.as_str()),
                                    // The engine only ever mutates demo money.
                                    balances::source.eq("DEMO"),
                                    balances::amount.eq(r.amount.clone()),
                                )
                            })
                            .collect::<Vec<_>>(),
                    )
                    .on_conflict((
                        balances::user_id,
                        balances::asset,
                        balances::source,
                    ))
                    .do_update()
                    .set(balances::amount.eq(excluded(balances::amount)))
                    .execute(conn)
                    .await?;
            }

            if !keys.is_empty() {
                use crate::schema::idempotency_keys;

                insert_into(idempotency_keys::table)
                    .values(
                        keys.iter()
                            .map(|k| {
                                (
                                    idempotency_keys::user_id.eq(k.user_id),
                                    idempotency_keys::key.eq(k.key.clone()),
                                    idempotency_keys::amount.eq(k.amount.clone()),
                                )
                            })
                            .collect::<Vec<_>>(),
                    )
                    .on_conflict((idempotency_keys::user_id, idempotency_keys::key))
                    .do_nothing()
                    .execute(conn)
                    .await?;
            }

            Ok::<(), diesel::result::Error>(())
        })
    })
    .await?;

    Ok(())
}
