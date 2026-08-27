mod auth;
mod config;
mod db;
mod engine;
mod error;
mod handlers;
mod routes;
mod schema;
mod types;

use dashmap::DashMap;
use uuid::Uuid;

use crate::{
    config::AppConfig,
    db::{DbPool, query},
    engine::types::Asset,
    error::AppError,
};
use actix_web::{App, HttpServer, web};

/// Shared application state. Everything is `Arc`-backed via `web::Data`.
///
/// - `pool`     : thread-safe DB connection pool (source of truth). No Mutex.
/// - `users`    : DashMap cache of `username -> id` for fast lookups.
/// - `balances` : DashMap cache of `(user_id, asset) -> amount` for fast reads.
pub struct AppState {
    pub pool: DbPool,
    pub config: AppConfig,
    pub users: DashMap<String, Uuid>,
    pub balances: DashMap<(Uuid, Asset), bigdecimal::BigDecimal>,
}

impl AppState {
    /// Build state and pre-load the caches from the DB so hot reads do not
    /// hit the database on startup.
    pub async fn create(pool: DbPool, config: AppConfig) -> Result<Self, AppError> {
        let users = DashMap::new();
        let balances = DashMap::new();

        let mut conn = pool.get().await?;

        for u in query::load_all_users(&mut conn).await? {
            users.insert(u.username, u.id);
        }

        for b in query::load_all_balances(&mut conn).await? {
            if let Some(asset) = Asset::parse(&b.asset) {
                balances.insert((b.user_id, asset), b.amount);
            }
        }

        drop(conn);

        Ok(Self {
            pool,
            config,
            users,
            balances,
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

    let state = AppState::create(pool, config)
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
