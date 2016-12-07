use diesel::sqlite::SqliteConnection;
use r2d2::{Config, Pool};
use r2d2_diesel::ConnectionManager;

/// Struct to attach a database connection pool to an iron request.
pub struct ConnectionPool;

impl ConnectionPool {
    /// Create connection pool for the sqlite database
    pub fn create(database_url: &str) -> Pool<ConnectionManager<SqliteConnection>> {
        let config = Config::default();
        let manager = ConnectionManager::<SqliteConnection>::new(database_url);
        Pool::new(config, manager).expect("Failed to create pool.")
    }
}
