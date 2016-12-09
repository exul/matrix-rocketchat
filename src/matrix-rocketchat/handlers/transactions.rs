use iron::{Handler, status};
use iron::prelude::*;

use config::Config;
use middleware::AccessToken;

/// Transactions is an endpoint of the application service API which is called by the homeserver
/// to push new events.
pub struct Transactions {}

impl Transactions {
    /// Transactions endpoint with middleware
    pub fn chain(config: Config) -> Chain {
        let transactions = Transactions {};
        let mut chain = Chain::new(transactions);
        chain.link_before(AccessToken { config: config });

        chain
    }
}

impl Handler for Transactions {
    fn handle(&self, _request: &mut Request) -> IronResult<Response> {
        Ok(Response::with((status::Ok, "{}".to_string())))
    }
}
