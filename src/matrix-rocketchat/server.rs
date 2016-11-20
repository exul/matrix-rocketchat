pub use config::Config;

/// The application service server
pub struct Server<'a> {
    /// Application service configuration
    config: &'a Config,
}

impl<'a> Server<'a> {
    /// Create a new server with a given configuration.
    pub fn new(config: &Config) -> Server {
        Server { config: config }
    }

    /// Runs the application service bridge.
    pub fn run(&self) {
        println!("Hello world with config: {:?}", self.config);
    }
}
