mod balance;
mod config;
mod db;
mod error;
mod queue;
mod schema;
mod types;

use crate::config::AppConfig;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();

    let config = AppConfig::from_env().map_err(|e| std::io::Error::other(e.to_string()))?;

    let pool = db::init_pool(&config.database_url)
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    let engine_queue = queue::EngineQueue::connect(&config.redis_url)
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    // Recover + seed the balance worker from the WAL/Postgres before consuming.
    let (balances, dedupe) = balance::init(&pool, &config.wal_path)
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    // Intake task owns the (never-cancelled) blocking XREADGROUP and forwards
    // commands to the worker over a bounded channel (natural backpressure).
    let (tx, rx) = tokio::sync::mpsc::channel::<(String, crate::types::Command)>(1024);
    let ingest_queue = engine_queue.clone();
    tokio::spawn(async move { ingest_queue.ingest(tx).await });

    eprintln!("engine: ready on {}", config.redis_url);

    balance::run(
        rx,
        engine_queue,
        pool,
        balances,
        dedupe,
        config.wal_path.clone(),
        config.flush_interval(),
    )
    .await;

    Ok(())
}
