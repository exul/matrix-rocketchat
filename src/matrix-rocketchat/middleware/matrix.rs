use iron::{BeforeMiddleware, IronResult, Request};

use config::Config;
use errors::*;
use iron::url::Url;
use log::*;

/// Compares the supplied access token to the one that is in the config
pub struct AccessToken {
    /// Application service config
    pub config: Config,
}

impl BeforeMiddleware for AccessToken {
    fn before(&self, request: &mut Request) -> IronResult<()> {
        let url: Url = request.url.clone().into();
        let mut query_pairs = url.query_pairs();
        let logger = IronLogger::from_request(request)?;

        if let Some((_, ref token)) = query_pairs.find(|&(ref key, _)| key == "access_token") {
            if token == &self.config.hs_token {
                return Ok(());
            }

            let err = simple_error!(ErrorKind::InvalidAccessToken(token.to_string()));
            info!(logger, err);
            return Err(err.into());
        }

        let err = simple_error!(ErrorKind::MissingAccessToken);
        info!(logger, err);
        Err(err.into())
    }
}
