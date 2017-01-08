extern crate iron;
extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate router;
#[macro_use]
extern crate slog;
extern crate slog_json;
extern crate slog_stream;
extern crate slog_term;
extern crate tempdir;

use std::sync::mpsc::channel;
use std::thread;

use iron::{Iron, Listening, status};
use iron::Protocol::Http;
use matrix_rocketchat::Server;
use matrix_rocketchat::errors::*;
use matrix_rocketchat_test::{DEFAULT_LOGGER, IRON_THREADS, TEMP_DIR_NAME, handlers};
use router::Router;
use tempdir::TempDir;

#[test]
fn starup_fails_when_server_cannot_bind_to_address() {
    let temp_dir = TempDir::new(TEMP_DIR_NAME).expect("Could not create temp dir");
    let mut config = matrix_rocketchat_test::build_test_config(&temp_dir);
    let log = DEFAULT_LOGGER.clone();

    let (homeserver_mock_tx, homeserver_mock_rx) = channel::<Listening>();
    let homeserver_mock_socket_addr = matrix_rocketchat_test::get_free_socket_addr();
    config.hs_url = format!("http://{}:{}",
                            homeserver_mock_socket_addr.ip(),
                            homeserver_mock_socket_addr.port());

    thread::spawn(move || {
        let mut router = Router::new();
        router.get("/_matrix/client/versions", handlers::MatrixVersion {});
        router.post("*", handlers::EmptyJson {});
        let listening = Iron::new(router).listen_with(homeserver_mock_socket_addr, IRON_THREADS, Http, None).unwrap();
        homeserver_mock_tx.send(listening).unwrap();
    });
    let mut homeserver_mock_listen = homeserver_mock_rx.recv_timeout(matrix_rocketchat_test::default_timeout()).unwrap();

    let running_server_config = config.clone();
    let running_server_log = log.clone();
    let (running_server_tx, running_server_rx) = channel::<Result<Listening>>();
    thread::spawn(move || {
        let running_server_result = Server::new(&running_server_config, running_server_log).run();
        homeserver_mock_listen.close().unwrap();
        running_server_tx.send(running_server_result).unwrap();
    });
    let running_server_result = running_server_rx.recv_timeout(matrix_rocketchat_test::default_timeout()).unwrap();
    assert!(running_server_result.is_ok());

    let (failed_server_tx, failed_server_rx) = channel::<Result<Listening>>();
    thread::spawn(move || {
        let failed_server_result = Server::new(&config, log).run();
        failed_server_tx.send(failed_server_result).unwrap();
    });
    let failed_server_result = failed_server_rx.recv_timeout(matrix_rocketchat_test::default_timeout()).unwrap();
    assert!(failed_server_result.is_err());
    running_server_result.unwrap().close().unwrap();
}

#[test]
fn startup_fails_when_the_server_cannot_query_the_matrix_api_version() {
    let temp_dir = TempDir::new(TEMP_DIR_NAME).expect("Could not create temp dir");
    let mut config = matrix_rocketchat_test::build_test_config(&temp_dir);
    let log = DEFAULT_LOGGER.clone();

    let (homeserver_mock_tx, homeserver_mock_rx) = channel::<Listening>();
    let homeserver_mock_socket_addr = matrix_rocketchat_test::get_free_socket_addr();
    config.hs_url = format!("http://{}:{}",
                            homeserver_mock_socket_addr.ip(),
                            homeserver_mock_socket_addr.port());

    thread::spawn(move || {
        let mut router = Router::new();
        router.get("/_matrix/client/versions",
                   handlers::ErrorResponse { status: status::InternalServerError });
        let listening = Iron::new(router).listen_with(homeserver_mock_socket_addr, IRON_THREADS, Http, None).unwrap();
        homeserver_mock_tx.send(listening).unwrap();
    });
    let mut homeserver_mock_listen = homeserver_mock_rx.recv_timeout(matrix_rocketchat_test::default_timeout()).unwrap();

    let (failed_server_tx, failed_server_rx) = channel::<Result<Listening>>();
    thread::spawn(move || {
        let failed_server_result = Server::new(&config, log).run();
        failed_server_tx.send(failed_server_result).unwrap();
    });
    let failed_server_result = failed_server_rx.recv_timeout(matrix_rocketchat_test::default_timeout() * 2).unwrap();
    homeserver_mock_listen.close().unwrap();
    assert!(failed_server_result.is_err());
}
