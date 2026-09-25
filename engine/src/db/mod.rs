pub mod query;
pub mod rows;
pub mod store;

use diesel_async::AsyncPgConnection;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::pooled_connection::bb8::Pool;

/// A bb8 pool of async Postgres connections, produced by the corresponding
/// diesel-async connection manager.
pub type DbPool = Pool<AsyncPgConnection>;

pub async fn init_pool(
    database_url: &str,
) -> Result<DbPool, Box<dyn std::error::Error + Send + Sync>> {
    let manager = AsyncDieselConnectionManager::<AsyncPgConnection>::new(database_url);
    Ok(Pool::builder().build(manager).await?)
}
