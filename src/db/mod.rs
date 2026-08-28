pub mod query;
pub mod rows;
pub mod store;

use diesel::deserialize::QueryableByName;
use diesel::sql_query;
use diesel::sql_types::Text;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::pooled_connection::bb8::Pool;
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl, SimpleAsyncConnection};

/// A bb8 pool of async Postgres connections, produced by the corresponding
/// diesel-async connection manager.
pub type DbPool = Pool<AsyncPgConnection>;

/// Embedded migrations. Version -> (up_sql, down_sql).
///
/// Keeping the SQL as compile-time constants avoids a hard
/// runtime dependency on the migrations directory and needs no
/// libpq-backed synchronous connection.
const MIGRATIONS: &[Migration] = &[Migration {
    version: "0001_init",
    up: include_str!("../../migrations/0001_init/up.sql"),
}];

struct Migration {
    version: &'static str,
    up: &'static str,
}

/// Row used to read the set of already-applied migrations via raw SQL.
#[derive(QueryableByName, Debug)]
struct VersionRow {
    #[diesel(sql_type = Text)]
    version: String,
}

pub async fn init_pool(
    database_url: &str,
) -> Result<DbPool, Box<dyn std::error::Error + Send + Sync>> {
    let manager = AsyncDieselConnectionManager::<AsyncPgConnection>::new(database_url);
    Ok(Pool::builder().build(manager).await?)
}

/// Apply any migrations that have not yet been recorded in `_cex_migrations`.
pub async fn run_migrations(pool: &DbPool) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut conn = pool.get().await?;

    conn.batch_execute(
        "CREATE TABLE IF NOT EXISTS _cex_migrations (
            version TEXT PRIMARY KEY,
            applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
    )
    .await?;

    let applied: Vec<String> = sql_query("SELECT version FROM _cex_migrations")
        .load::<VersionRow>(&mut conn)
        .await?
        .into_iter()
        .map(|r| r.version)
        .collect();

    for migration in MIGRATIONS {
        if applied.iter().any(|v| v == migration.version) {
            continue;
        }

        conn.transaction(|conn| {
            Box::pin(async move {
                // The migration file may contain multiple DDL statements, which
                // cannot run through a single prepared statement.
                conn.batch_execute(migration.up).await?;
                sql_query("INSERT INTO _cex_migrations (version) VALUES ($1)")
                    .bind::<Text, _>(migration.version)
                    .execute(conn)
                    .await?;
                Ok::<(), diesel::result::Error>(())
            })
        })
        .await?;
    }

    Ok(())
}
