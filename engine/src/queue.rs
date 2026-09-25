use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use redis::aio::ConnectionManager;
use redis::streams::{StreamId, StreamReadOptions, StreamReadReply};
use redis::{AsyncCommands, Value, from_redis_value};

use crate::error::AppError;
use crate::types::{Command, Outcome};

/// Stream carrying commands from the backend to the engine.
pub const COMMANDS_STREAM: &str = "stream:commands";
/// Stream carrying results from the engine back to the backend.
pub const RESULTS_STREAM: &str = "stream:results";
/// Consumer group + sole consumer. Single consumer preserves FIFO ordering.
pub const GROUP: &str = "engine";
pub const CONSUMER: &str = "w1";
/// The single field holding the JSON payload of a message.
pub const PAYLOAD: &str = "payload";

/// Redis-backed transport between the backend and the engine.
///
/// Two SEPARATE ConnectionManagers on purpose: the ingest task holds a
/// blocking `XREADGROUP` for up to 1s on its own connection, and the worker's
/// publish/ack must never queue behind that block on a shared socket. When a
/// future is cancelled (the worker's `flush`/`handle` are never cancelled, but
/// the `select!` boundaries are otherwise await-safe) or Redis restarts, the
/// manager is swapped for a fresh one, so a single dead pooled connection
/// cannot wedge the engine permanently: the first post-outage error rebuilds
/// both managers and everything with it recovers.
#[derive(Clone)]
pub struct EngineQueue {
    url: String,
    /// Used ONLY by the ingest task's blocking read.
    read_inner: Arc<Mutex<ConnectionManager>>,
    /// Used by everything else (publish, ack, reclaim, diagnostics).
    write_inner: Arc<Mutex<ConnectionManager>>,
}

impl EngineQueue {
    /// Open two Redis connections and ensure the engine consumer group exists.
    pub async fn connect(url: &str) -> Result<Self, AppError> {
        let read = Self::open_manager(url).await?;
        let write = Self::open_manager(url).await?;
        Ok(Self {
            url: url.to_string(),
            read_inner: Arc::new(Mutex::new(read)),
            write_inner: Arc::new(Mutex::new(write)),
        })
    }

    /// Fresh connection manager, with the consumer group created (BusyGroup on
    /// restart / reconnect is expected and fine).
    async fn open_manager(url: &str) -> Result<ConnectionManager, AppError> {
        let client = redis::Client::open(url)?;
        let mut conn = client.get_connection_manager().await?;

        // Start consuming from the beginning of the stream: commands published
        // while the engine was booting are not skipped. Delivery from `0` is
        // safe because replay is idempotent (resulting amounts + dedupe keys).
        let _ = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(COMMANDS_STREAM)
            .arg(GROUP)
            .arg("0")
            .arg("MKSTREAM")
            .query_async::<Value>(&mut conn)
            .await
            .inspect_err(|e| eprintln!("engine: xgroup create (expect BUSYGROUP on restart): {e}"));

        Ok(conn)
    }

    /// The ingest task's dedicated connection. Clone out without holding the
    /// lock across any `.await`.
    fn read_manager(&self) -> Result<ConnectionManager, AppError> {
        Ok(self.read_inner.lock().expect("queue mutex poisoned").clone())
    }

    /// The worker's connection for publish/ack/reclaim. Clone out without
    /// holding the lock across any `.await`.
    fn write_manager(&self) -> Result<ConnectionManager, AppError> {
        Ok(self.write_inner.lock().expect("queue mutex poisoned").clone())
    }

    /// Rebuild both connection managers after a Redis failure. Best-effort: the
    /// next operation either uses the fresh managers or fails again and retries.
    pub async fn reconnect(&self) {
        match Self::open_manager(&self.url).await {
            Ok(fresh) => {
                *self.read_inner.lock().expect("queue mutex poisoned") = fresh.clone();
                *self.write_inner.lock().expect("queue mutex poisoned") = fresh;
            }
            Err(e) => eprintln!("engine: reconnect failed: {e}"),
        }
    }

    /// Read the next batch of commands. Blocks up to 1s waiting for new
    /// entries. MUST only be called from the dedicated ingest task: a cancelled
    /// blocking read can strand delivered-but-unacked entries in the PEL.
    pub async fn read_commands(&self) -> Result<Vec<(String, Command)>, AppError> {
        let opts = StreamReadOptions::default()
            .group(GROUP, CONSUMER)
            .count(16)
            .block(1000);

        let mut conn = self.read_manager()?;
        let reply: StreamReadReply =
            match conn.xread_options(&[COMMANDS_STREAM], &[">"], &opts).await {
                Ok(reply) => reply,
                Err(e) => {
                    self.reconnect().await;
                    return Err(e.into());
                }
            };

        let mut out = Vec::new();
        for key in reply.keys {
            for id in key.ids {
                match entry_to_command(&id) {
                    Ok(cmd) => out.push((id.id, cmd)),
                    Err(error) => self.reject(&id.id, &error).await,
                }
            }
        }
        Ok(out)
    }

    /// Every command gets exactly one reply: a payload the engine cannot
    /// interpret is answered with an error result (when a reply_key can still
    /// be extracted) and acked, so the backend is never left hanging and the
    /// entry cannot loop forever as a poison pill.
    async fn reject(&self, cmd_id: &str, error: &str) {
        match self.command_payload(cmd_id).await {
            Some(raw) => {
                if let Some(reply_key) = reply_key_of(&raw) {
                    let _ = self
                        .publish(&Outcome::Error {
                            reply_key,
                            error: error.to_string(),
                        })
                        .await;
                }
            }
            None => eprintln!("engine: could not read payload for poison pill {cmd_id}"),
        }
        let _ = self.ack(cmd_id.to_string()).await;
    }

    /// Fetch a command entry's raw payload for diagnostics / reply correlation.
    async fn command_payload(&self, cmd_id: &str) -> Option<String> {
        let mut conn = self.write_manager().ok()?;
        let entries: Vec<(String, HashMap<String, Value>)> = redis::cmd("XRANGE")
            .arg(COMMANDS_STREAM)
            .arg(cmd_id)
            .arg(cmd_id)
            .query_async(&mut conn)
            .await
            .ok()?;
        entries
            .first()
            .and_then(|(_, map)| map.get(PAYLOAD))
            .and_then(|v| from_redis_value::<String>(v).ok())
    }

    /// Dedicated intake task: reclaim anything left unacked by a previous life,
    /// then loop-reading commands and forwarding them into the worker channel.
    /// The blocking read is never cancelled here, so nothing gets stranded.
    /// Backpressure: a full channel pauses consumption (entries stay in the
    /// stream, undelivered), which is exactly what we want.
    pub async fn ingest(self, tx: tokio::sync::mpsc::Sender<(String, Command)>) {
        for (cmd_id, cmd) in self.reclaim_pending().await {
            if tx.send((cmd_id, cmd)).await.is_err() {
                return;
            }
        }

        loop {
            match self.read_commands().await {
                Ok(cmds) => {
                    for (cmd_id, cmd) in cmds {
                        if tx.send((cmd_id, cmd)).await.is_err() {
                            return;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("engine: read_commands failed: {e}");
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        }
    }

    /// Acknowledge a command so it is removed from the group's pending list.
    pub async fn ack(&self, cmd_id: String) -> Result<(), AppError> {
        // After a Redis failure, rebuild the connection and retry once; the
        // worker must not die (and stranding acked results in a loop) just
        // because Redis hiccupped.
        let mut conn = self.write_manager()?;
        let result: redis::RedisResult<u32> = conn
            .xack(COMMANDS_STREAM, GROUP, std::slice::from_ref(&cmd_id))
            .await;
        match result {
            Ok(_) => Ok(()),
            Err(_) => {
                self.reconnect().await;
                let mut conn = self.write_manager()?;
                let _: u32 = conn.xack(COMMANDS_STREAM, GROUP, &[cmd_id]).await?;
                Ok(())
            }
        }
    }

    /// Publish a result for the backend. The results stream is capped so a long
    /// running exchange cannot grow it without bound (acked replies are
    /// consumed by the backend, pending-less entries are safe to trim).
    pub async fn publish(&self, outcome: &Outcome) -> Result<(), AppError> {
        let payload = serde_json::to_string(outcome)?;
        let mut conn = self.write_manager()?;
        match redis::cmd("XADD")
            .arg(RESULTS_STREAM)
            .arg("MAXLEN")
            .arg("~")
            .arg(1000)
            .arg("*")
            .arg(PAYLOAD)
            .arg(payload.as_str())
            .query_async::<String>(&mut conn)
            .await
        {
            Ok(_) => Ok(()),
            Err(_) => {
                self.reconnect().await;
                let mut conn = self.write_manager()?;
                let _: String = redis::cmd("XADD")
                    .arg(RESULTS_STREAM)
                    .arg("MAXLEN")
                    .arg("~")
                    .arg(1000)
                    .arg("*")
                    .arg(PAYLOAD)
                    .arg(payload.as_str())
                    .query_async(&mut conn)
                    .await?;
                Ok(())
            }
        }
    }

    /// Reclaim commands that were delivered to us but never acked (crash, or a
    /// wait cancelled exactly during delivery) and feed them back through the
    /// normal handle path. Best-effort: idempotency keys cover any residual gap.
    ///
    /// Single pass, ON PURPOSE: XPENDING lists entries owned by ANY consumer and
    /// XCLAIM only transfers ownership (it never removes them), so rescanning
    /// the same range would never terminate. A generous COUNT covers a full
    /// batch and more; anything still left behind is reclaimed next boot.
    pub async fn reclaim_pending(&self) -> Vec<(String, Command)> {
        let mut out = Vec::new();
        let mut conn = match self.write_manager() {
            Ok(conn) => conn,
            Err(_) => return out,
        };

        let pending: Vec<(String, String, i64, i64)> = match redis::cmd("XPENDING")
            .arg(COMMANDS_STREAM)
            .arg(GROUP)
            .arg("-")
            .arg("+")
            .arg(100_000)
            .query_async(&mut conn)
            .await
        {
            Ok(p) => p,
            Err(_) => return out,
        };
        if pending.is_empty() {
            return out;
        }

        // Claim every pending id for ourselves (MIN-IDLE-TIME 0).
        let mut claim = redis::cmd("XCLAIM");
        claim.arg(COMMANDS_STREAM).arg(GROUP).arg(CONSUMER).arg(0);
        for (id, ..) in &pending {
            claim.arg(id);
        }
        if let Err(e) = claim.query_async::<Value>(&mut conn).await {
            eprintln!("engine: xclaim failed: {e}");
            return out;
        }

        for (id, ..) in &pending {
            let entries: Vec<(String, HashMap<String, Value>)> = match redis::cmd("XRANGE")
                .arg(COMMANDS_STREAM)
                .arg(id)
                .arg(id)
                .query_async(&mut conn)
                .await
            {
                Ok(e) => e,
                Err(_) => continue,
            };
            for (sid, map) in entries {
                if let Some(payload) = map.get(PAYLOAD)
                    && let Ok(raw) = from_redis_value::<String>(payload)
                {
                    match serde_json::from_str::<Command>(&raw) {
                        Ok(cmd) => out.push((sid, cmd)),
                        Err(_) => {
                            self.reject(&sid, "unparseable reclaimed command").await;
                        }
                    }
                }
            }
        }
        out
    }
}

fn entry_to_command(id: &StreamId) -> Result<Command, String> {
    let payload = id
        .map
        .get(PAYLOAD)
        .ok_or_else(|| "missing payload field".to_string())?;
    let raw = from_redis_value::<String>(payload).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

/// Best-effort extraction of a command's reply key from its raw JSON, so a
/// malformed command can still be answered (not left hanging).
fn reply_key_of(raw: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    value
        .get("reply_key")
        .and_then(|k| k.as_str())
        .map(ToOwned::to_owned)
}
