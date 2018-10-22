use diesel::sqlite::SqliteConnection;
use iron::typemap::Key;
use iron::{Plugin, Request};
use persistent::Write;
use r2d2::{Pool, PooledConnection};
use r2d2_diesel::ConnectionManager;

use errors::*;

/// Struct to attach a database connection pool to an iron request.
pub struct ConnectionPool;

impl ConnectionPool {
    /// Create connection pool for the sqlite database
    pub fn create(database_url: &str) -> Result<Pool<ConnectionManager<SqliteConnection>>> {
        let manager = ConnectionManager::<SqliteConnection>::new(database_url);
        Pool::new(manager).chain_err(|| ErrorKind::ConnectionPoolCreationError).map_err(Error::from)
    }

    /// Extract a database connection from the pool stored in the request.
    pub fn from_request(request: &mut Request) -> Result<PooledConnection<ConnectionManager<SqliteConnection>>> {
        let mutex = request.get::<Write<ConnectionPool>>().chain_err(|| ErrorKind::ConnectionPoolExtractionError)?;
        let pool = match mutex.lock() {
            Ok(pool) => pool,
            // we can recover from a poisoned lock, because the thread that panicked will not be
            // able to finish/persist the changes, so it will be as if they never happened and we
            // are OK.
            Err(poisoned_lock) => poisoned_lock.into_inner(),
        };
        pool.get().chain_err(|| ErrorKind::GetConnectionError).map_err(Error::from)
    }
}

impl Key for ConnectionPool {
    type Value = Pool<ConnectionManager<SqliteConnection>>;
}
