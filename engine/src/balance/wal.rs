use std::io;
use std::path::PathBuf;

use bigdecimal::BigDecimal;
use serde_json::json;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use crate::db::rows::BalanceRecord;
use crate::types::Asset;

/// A journaled mutation: the resulting balance plus the idempotency key that
/// produced it. Carrying the key lets a crash recovery (WAL replay) reconstruct
/// the dedupe table BEFORE pending commands are reclaimed, so a command that
/// was fsynced but not yet acked can never be applied a second time.
#[derive(Debug, Clone)]
pub struct WalRecord {
    pub record: BalanceRecord,
    pub idempotency_key: Option<String>,
}

/// JSON-lines write-ahead log for balance mutations not yet applied to Postgres.
///
/// Record format (one per line):
/// ```text
/// {"user_id":"<uuid>","asset":"USD","amount":"123.45","idempotency_key":"d"}
/// ```
/// Each record stores the *resulting* balance for its key, so replay is
/// idempotent: applying records via upsert reconstructs the exact state at the
/// time of the crash, and the keys let subsequent command reclaims short-circuit.
pub struct Wal {
    path: PathBuf,
}

impl Wal {
    pub async fn open(path: &str) -> io::Result<Wal> {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await?;
        Ok(Wal {
            path: PathBuf::from(path),
        })
    }

    /// Append records to the log (buffered; durability comes from `sync`).
    pub async fn append_all(&self, records: &[WalRecord]) -> io::Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        let mut file = OpenOptions::new().append(true).open(&self.path).await?;
        let mut buf = String::new();
        for r in records {
            buf.push_str(&record_to_line(r));
            buf.push('\n');
        }
        file.write_all(buf.as_bytes()).await?;
        Ok(())
    }

    /// Force buffered WAL writes to stable storage.
    pub async fn sync(&self) -> io::Result<()> {
        let file = OpenOptions::new().append(true).open(&self.path).await?;
        file.sync_all().await
    }

    /// Read every record currently in the log.
    pub async fn read_all(&self) -> io::Result<Vec<WalRecord>> {
        let bytes = tokio::fs::read(&self.path).await?;
        let mut records = Vec::new();
        for line in bytes.split(|b| *b == b'\n') {
            if line.is_empty() {
                continue;
            }
            let s = std::str::from_utf8(line).map_err(|e| io::Error::other(e.to_string()))?;
            let record = line_to_record(s)
                .map_err(|e| io::Error::other(format!("corrupt wal line: {e}")))?;
            records.push(record);
        }
        Ok(records)
    }

    /// Clear the log (only after everything in it has been applied to Postgres).
    pub async fn truncate(&self) -> io::Result<()> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.path)
            .await?;
        file.sync_all().await?;
        Ok(())
    }
}

fn record_to_line(r: &WalRecord) -> String {
    json!({
        "user_id": r.record.user_id.to_string(),
        "asset": r.record.asset.as_str(),
        "amount": r.record.amount.to_string(),
        "idempotency_key": r.idempotency_key,
    })
    .to_string()
}

fn line_to_record(line: &str) -> Result<WalRecord, String> {
    let value: serde_json::Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
    let user_id = value
        .get("user_id")
        .and_then(|v| v.as_str())
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
        .ok_or_else(|| "missing user_id".to_string())?;
    let asset = value
        .get("asset")
        .and_then(|v| v.as_str())
        .and_then(Asset::parse)
        .ok_or_else(|| "missing asset".to_string())?;
    let amount = value
        .get("amount")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<BigDecimal>().ok())
        .ok_or_else(|| "missing amount".to_string())?;
    let idempotency_key = value
        .get("idempotency_key")
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned);
    Ok(WalRecord {
        record: BalanceRecord {
            user_id,
            asset,
            amount,
        },
        idempotency_key,
    })
}
