use std::fs::File;
use std::io::Read;
use std::net::SocketAddr;

use serde_yaml;

use errors::ASError;

/// Configuration for the application service.
#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    /// Token the application service uses when calling the homeserver api.
    pub as_token: String,
    /// Token the homeserver uses when calling the application service.
    pub hs_token: String,
    /// The address on which the application service will run.
    pub as_address: SocketAddr,
    /// The URL under which the application service will be reachable.
    pub as_url: String,
    /// The URL under wich the homeserver is reachable.
    pub hs_url: String,
    /// Domain of the homeserver
    pub hs_domain: String,
    /// Local part of the bot name which is also the namespace of the application service
    pub sender_localpart: String,
    /// URL to connect to the database
    pub database_url: String,
    /// Flag to indicate if the application service should use HTTPs
    pub use_ssl: bool,
    /// Path to the SSL certificate (only needed if SSL is used)
    pub ssl_certificate_path: Option<String>,
    /// Path to the SSL key (only needed if SSL is used)
    pub ssl_key_path: Option<String>,
}

impl Config {
    /// Loads the configuration from a YAML File.
    pub fn read_from_file(path: &str) -> Result<Config, ASError> {
        let mut config_content = String::new();
        let mut config_file = File::open(path).map_err(ASError::from)?;
        config_file.read_to_string(&mut config_content).map_err(ASError::from)?;
        let config: Config = serde_yaml::from_str(&config_content).map_err(ASError::from)?;
        Ok(config)
    }
}
