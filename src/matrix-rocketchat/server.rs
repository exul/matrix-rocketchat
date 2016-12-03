pub use config::Config;

use iron::Listening;
use iron::Iron;
use slog::Logger;
use router::Router;

use errors::*;
use handlers::Welcome;

/// The application service server
pub struct Server<'a> {
    /// Application service configuration
    config: &'a Config,
    /// Logger passed to the server
    logger: Logger,
}

impl<'a> Server<'a> {
    /// Create a new server with a given configuration.
    pub fn new(config: &Config, logger: Logger) -> Server {
        Server {
            config: config,
            logger: logger,
        }
    }

    /// Runs the application service bridge.
    pub fn run(&self) -> Result<Listening> {
        info!(self.logger, "Starting server"; "address" => format!("{:?}", self.config.as_address));
        let router = self.setup_routes();
        Iron::new(router).http(self.config.as_address).chain_err(|| "unable to start server")
    }


    fn setup_routes(&self) -> Router {
        debug!(self.logger, "Setting up routes");
        let mut router = Router::new();
        router.get("/", Welcome {});

        router
    }
}
