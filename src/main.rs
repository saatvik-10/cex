mod auth;
mod balance;
mod config;
mod db;
mod engine;
mod error;
mod handlers;
mod routes;
mod schema;
mod types;

use std::time::Duration;

use dashmap::DashMap;
use uuid::Uuid;

use crate::{
    config::AppConfig,
    db::{DbPool, query},
    error::AppError,
};
use actix_web::{App, HttpServer, web};

/// Shared application state. Everything is `Arc`-backed via `web::Data`.
///
/// - `pool`    : thread-safe DB connection pool (durable checkpoint sink).
/// - `users`   : DashMap cache of `username -> id` for fast lookups.
/// - `balance` : handle to the single balance worker (mpsc + oneshot replies).
pub struct AppState {
    pub pool: DbPool,
    pub config: AppConfig,
    pub users: DashMap<String, Uuid>,
    pub balance: balance::BalanceHandle,
}

impl AppState {
    /// Build state and pre-load the user cache from the DB.
    pub async fn create(
        pool: DbPool,
        config: AppConfig,
        balance: balance::BalanceHandle,
    ) -> Result<Self, AppError> {
        let users = DashMap::new();

        let mut conn = pool.get().await?;

        for u in query::load_all_users(&mut conn).await? {
            users.insert(u.username, u.id);
        }

        drop(conn);

        Ok(Self {
            pool,
            config,
            users,
            balance,
        })
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();

    let config = AppConfig::from_env().map_err(|e| std::io::Error::other(e.to_string()))?;

    let pool = db::init_pool(&config.database_url)
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    db::run_migrations(&pool)
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    // Recover + seed the balance worker from the WAL/Postgres before serving.
    let balances = balance::init(&pool, &config.wal_path)
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    let (tx, rx) = tokio::sync::mpsc::channel::<balance::BalanceCmd>(1024);
    let handle = balance::BalanceHandle::new(tx);

    let worker_pool = pool.clone();
    let wal_path = config.wal_path.clone();
    let flush_interval = Duration::from_millis(config.balance_flush_interval_ms);
    tokio::spawn(balance::run(rx, worker_pool, balances, wal_path, flush_interval));

    let state = AppState::create(pool, config, handle)
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    let port = state.config.port;
    let app_data = web::Data::new(state);

    HttpServer::new(move || {
        App::new()
            .app_data(app_data.clone())
            .configure(routes::config)
    })
    .bind(("127.0.0.1", port))?
    .run()
    .await
}
