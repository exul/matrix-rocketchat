use iron::{Plugin, Request};
use iron::typemap::Key;
use persistent::State;
use slog::Logger;

use errors::*;

/// Struct to attach a logger to an iron request.
pub struct IronLogger;

impl IronLogger {
    /// Gets the logger from the request.
    pub fn from_request(request: &mut Request) -> Result<Logger> {
        let lock = request.get::<State<IronLogger>>().chain_err(|| ErrorKind::LoggerExtractionError)?;
        let logger = match lock.read() {
            Ok(logger) => logger,
            // we can recover from a poisoned lock, because the thread that panicked will not be
            // able to do anything with the logger and we will not have any concurrency issues.
            Err(poisoned_lock) => poisoned_lock.into_inner(),
        };

        Ok(logger.clone())
    }
}

impl Key for IronLogger {
    type Value = Logger;
}

/// Log with level error including all the chained errors
pub fn log_error(logger: &Logger, err: &Error) {
    let msg = build_message(err);
    error!(logger, msg);
}

/// Log with level info including all the chained errors
pub fn log_info(logger: &Logger, err: &Error) {
    let msg = build_message(err);
    info!(logger, msg);
}

fn build_message(err: &Error) -> String {
    let mut msg = format!("{}", err);
    for err in err.error_chain.iter().skip(1) {
        msg = msg + " caused by: " + &format!("{}", err);
    }

    msg
}