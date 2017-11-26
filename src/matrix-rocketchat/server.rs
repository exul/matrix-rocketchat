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
use errors::*;
use handlers::iron::{Rocketchat, RocketchatLogin, Transactions, Welcome};
use log::IronLogger;
use models::ConnectionPool;

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
    pub fn run(&self, threads: usize) -> Result<Listening> {
        self.prepare_database()?;
        let connection_pool = ConnectionPool::create(&self.config.database_url)?;

        let matrix_api = MatrixApi::new(self.config, self.logger.clone())?;
        self.setup_bot_user(matrix_api.as_ref())?;

        let router = self.setup_routes(matrix_api);
        let mut chain = Chain::new(router);
        chain.link_before(Write::<ConnectionPool>::one(connection_pool));
        chain.link_before(State::<IronLogger>::one::<Logger>(self.logger.clone()));

        info!(self.logger, "Starting server"; "address" => format!("{:?}", self.config.as_address));
        let mut server = Iron::new(chain);
        server.threads = threads;
        server.http(self.config.as_address).chain_err(|| ErrorKind::ServerStartupError).map_err(Error::from)
    }

    fn setup_routes(&self, matrix_api: Box<MatrixApi>) -> Router {
        debug!(self.logger, "Setting up routes");
        let mut router = Router::new();
        router.get("/", Welcome {}, "welcome");
        router.put("/transactions/:txn_id", Transactions::chain(self.config.clone(), matrix_api.clone()), "transactions");
        router.post("/rocketchat", Rocketchat::chain(self.config, matrix_api.clone()), "rocketchat");
        router.post(
            "/rocketchat/login",
            RocketchatLogin {
                config: self.config.clone(),
                matrix_api: matrix_api,
            },
            "rocketchat_login",
        );
        router
    }

    fn prepare_database(&self) -> Result<()> {
        debug!(self.logger, "Setting up database {}", self.config.database_url);
        let connection = SqliteConnection::establish(&self.config.database_url).chain_err(|| ErrorKind::DBConnectionError)?;
        setup_database(&connection).chain_err(|| ErrorKind::DatabaseSetupError)?;
        run_embedded_migrations(&connection).chain_err(|| ErrorKind::MigrationError).map_err(Error::from)
    }

    fn setup_bot_user(&self, matrix_api: &MatrixApi) -> Result<()> {
        let matrix_bot_user_id = self.config.matrix_bot_user_id()?;
        debug!(self.logger, "Setting up bot user {}", matrix_bot_user_id);
        if matrix_api.get_display_name(matrix_bot_user_id.clone())?.is_some() {
            debug!(self.logger, "Bot user {} exists, skipping", matrix_bot_user_id);
            return Ok(());
        }

        debug!(self.logger, "Bot user {} doesn't exists, starting registration", matrix_bot_user_id);
        matrix_api.register(self.config.sender_localpart.clone())?;
        info!(self.logger, "Bot user {} successfully registered", matrix_bot_user_id);
        Ok(())
    }
}
