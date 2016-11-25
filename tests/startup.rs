extern crate iron;
extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
#[macro_use]
extern crate slog;
extern crate slog_json;
extern crate slog_stream;
extern crate slog_term;
extern crate tempdir;

use std::sync::mpsc::channel;
use std::thread;

use iron::Listening;
use matrix_rocketchat::{ASError, Server};
use matrix_rocketchat_test::DEFAULT_LOGGER;
use tempdir::TempDir;

#[test]
fn starup_fails_when_server_cannot_bind_to_address() {
    let temp_dir = TempDir::new(matrix_rocketchat_test::TEMP_DIR_NAME).expect("Could not create temp dir");
    let config = matrix_rocketchat_test::build_test_config(&temp_dir);
    let log = DEFAULT_LOGGER.clone();

    let running_server_config = config.clone();
    let running_server_log = log.clone();
    let (running_server_tx, running_server_rx) = channel::<Result<Listening, ASError>>();
    thread::spawn(move || {
        let running_server_result = Server::new(&running_server_config, running_server_log).run();
        running_server_tx.send(running_server_result).unwrap();

    });
    let running_server_result = running_server_rx.recv_timeout(matrix_rocketchat_test::default_timeout()).unwrap();
    assert!(running_server_result.is_ok());

    let (failed_server_tx, failed_server_rx) = channel::<Result<Listening, ASError>>();
    thread::spawn(move || {
        let failed_server_result = Server::new(&config, log).run();
        failed_server_tx.send(failed_server_result).unwrap();
    });
    let failed_server_result = failed_server_rx.recv_timeout(matrix_rocketchat_test::default_timeout()).unwrap();
    assert!(failed_server_result.is_err());
    running_server_result.unwrap().close().unwrap();
}
