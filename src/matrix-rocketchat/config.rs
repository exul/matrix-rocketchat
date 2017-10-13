use std::convert::TryFrom;
use std::fs::File;
use std::io::Read;
use std::net::SocketAddr;

use ruma_identifiers::UserId;
use serde_yaml;

use errors::*;

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
    /// If this flag is set to true, the bot user accepts invites from rooms on other homeservers.
    /// Which means that users from other homeservers can use this Rocket.Chat bridge.
    pub accept_remote_invites: bool,
    /// Flag to indicate if the application service should use HTTPS
    pub use_ssl: bool,
    /// Path to the SSL certificate (only needed if SSL is used)
    pub ssl_certificate_path: Option<String>,
    /// Path to the SSL key (only needed if SSL is used)
    pub ssl_key_path: Option<String>,
}

impl Config {
    /// Loads the configuration from a YAML File.
    pub fn read_from_file(path: &str) -> Result<Config> {
        let mut config_content = String::new();
        let mut config_file = File::open(path).chain_err(|| ErrorKind::ReadFileError(path.to_string()))?;
        config_file.read_to_string(&mut config_content).chain_err(|| ErrorKind::ReadConfigError)?;
        let config: Config = serde_yaml::from_str(&config_content).chain_err(|| {
            ErrorKind::InvalidYAML("Could not serialize config".to_string())
        })?;
        Ok(config)
    }

    /// Matrix id of the bot user.
    pub fn matrix_bot_user_id(&self) -> Result<UserId> {
        let user_id = format!("@{}:{}", &self.sender_localpart, &self.hs_domain);
        UserId::try_from(&user_id).chain_err(|| ErrorKind::InvalidUserId(user_id)).map_err(Error::from)
    }

    /// Check if the user ID is part of the application service namespace
    pub fn is_application_service_user(&self, matrix_user_id: &UserId) -> bool {
        let id_prefix = format!("@{}", self.sender_localpart);
        matrix_user_id.to_string().starts_with(&id_prefix)
    }
}
