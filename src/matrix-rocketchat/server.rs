use diesel::Connection;
use diesel::migrations::setup_database;
use diesel::sqlite::SqliteConnection;
use embedded_migrations::run as run_embedded_migrations;
use iron::{Chain, Iron, Listening};
use persistent::{State, Write};
use router::Router;
use slog::Logger;

use api::MatrixApi;
use config::Config;
use db::ConnectionPool;
use db::user::{NewUser, User};
use errors::*;
use handlers::iron::{Transactions, Welcome};
use i18n::*;
use log::IronLogger;

/// The application service server
pub struct Server<'a> {
    /// Application service configuration
    config: &'a Config,
    /// Logger passed to the server
    logger: Logger,
}

impl<'a> Server<'a> {
    /// Create a new `Server` with a given configuration.
    pub fn new(config: &Config, logger: Logger) -> Server {
        Server {
            config: config,
            logger: logger,
        }
    }

    /// Runs the application service bridge.
    pub fn run(&self) -> Result<Listening> {
        self.prepare_database().chain_err(|| "Database setup failed")?;
        let connection_pool = ConnectionPool::create(&self.config.database_url);
        let connection = connection_pool.get().chain_err(|| "Getting database connection failed")?;

        let matrix_api = MatrixApi::new(self.config, self.logger.clone())?;
        self.setup_bot_user(&connection, &matrix_api)?;

        let router = self.setup_routes(matrix_api);
        let mut chain = Chain::new(router);
        chain.link_before(Write::<ConnectionPool>::one(connection_pool));
        chain.link_before(State::<IronLogger>::one::<Logger>(self.logger.clone()));

        info!(self.logger, "Starting server"; "address" => format!("{:?}", self.config.as_address));
        Iron::new(chain).http(self.config.as_address).chain_err(|| "Unable to start server")
    }

    fn setup_routes(&self, matrix_api: Box<MatrixApi>) -> Router {
        debug!(self.logger, "Setting up routes");
        let mut router = Router::new();
        router.get("/", Welcome {}, "welcome");
        router.put("/transactions/:txn_id",
                   Transactions::chain(self.config.clone(), matrix_api),
                   "transactions");

        router
    }

    fn prepare_database(&self) -> Result<()> {
        debug!(self.logger, format!("Setting up database {}", self.config.database_url));
        let connection =
            SqliteConnection::establish(&self.config.database_url).chain_err(|| "Could not establish database connection")?;
        setup_database(&connection).chain_err(|| "Could not setup database")?;
        run_embedded_migrations(&connection).chain_err(|| "Running migrations failed")
    }

    fn setup_bot_user(&self, connection: &SqliteConnection, matrix_api: &Box<MatrixApi>) -> Result<()> {
        let matrix_bot_user_id = self.config.matrix_bot_user_id().chain_err(|| "Could not setup matrix bot user")?;
        debug!(self.logger, format!("Setting up bot user {}", matrix_bot_user_id));
        match User::find_by_matrix_user_id(connection, &matrix_bot_user_id)? {
            Some(user) => {
                debug!(self.logger, format!("Bot user {} exists, skipping", user.matrix_user_id));
            }
            None => {
                debug!(self.logger,
                       format!("Bot user {} doesn't exists, starting registration", matrix_bot_user_id));

                matrix_api.register(matrix_bot_user_id.to_string())?;
                let new_user = NewUser {
                    matrix_user_id: matrix_bot_user_id.clone(),
                    language: DEFAULT_LANGUAGE,
                    is_virtual_user: false,
                };
                User::insert(connection, &new_user)?;
                info!(self.logger, format!("Bot user {} successfully registered", matrix_bot_user_id));
            }
        }
        Ok(())
    }
}
