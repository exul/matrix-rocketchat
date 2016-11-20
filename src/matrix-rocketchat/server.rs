pub use config::Config;

use slog::Logger;

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
    pub fn run(&self) {
        info!(self.logger, "Starting server");
    }
}
