extern crate iron;
extern crate matrix_rocketchat;
#[macro_use]
extern crate matrix_rocketchat_test;
extern crate router;
extern crate ruma_client_api;
#[macro_use]
extern crate slog;
extern crate slog_json;
extern crate slog_stream;
extern crate slog_term;
extern crate tempdir;

use std::sync::mpsc::channel;
use std::thread;

use iron::{Iron, Listening, status};
use matrix_rocketchat::Server;
use matrix_rocketchat::errors::*;
use matrix_rocketchat_test::{DEFAULT_LOGGER, IRON_THREADS, TEMP_DIR_NAME, default_matrix_api_versions, handlers};
use router::Router;
use ruma_client_api::Endpoint;
use ruma_client_api::r0::account::register::Endpoint as RegisterEndpoint;
use tempdir::TempDir;

#[test]
fn starup_fails_when_server_cannot_bind_to_address() {
    let temp_dir = TempDir::new(TEMP_DIR_NAME).unwrap();
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
                   handlers::MatrixVersion { versions: default_matrix_api_versions() },
                   "get_versions");
        router.post("*", handlers::EmptyJson {}, "default_post");
        let mut server = Iron::new(router);
        server.threads = IRON_THREADS;
        let listening = server.http(homeserver_mock_socket_addr).unwrap();
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
fn startup_fails_when_querying_the_api_version_is_not_successful_and_returs_a_matrix_error() {
    let mut router = Router::new();
    let error_responder = handlers::ErrorResponder {
        status: status::InternalServerError,
        message: "Could not server API versions".to_string(),
    };
    router.get("/_matrix/client/versions", error_responder, "get_versions");

    let server_result = start_servers(router);

    let err = server_result.unwrap_err();
    let _msg = String::new();
    assert_error_kind!(err, ErrorKind::MatrixError(ref _msg));
}

#[test]
fn startup_fails_when_querying_the_api_version_is_not_successful_and_returns_an_invalid_response() {
    let mut router = Router::new();
    router.get("/_matrix/client/versions",
               handlers::InvalidJsonResponse { status: status::InternalServerError },
               "get_versions");

    let server_result = start_servers(router);

    let err = server_result.unwrap_err();
    let _msg = String::new();
    assert_error_kind!(err, ErrorKind::InvalidJSON(ref _msg));
}

#[test]
fn startup_fails_when_the_server_can_query_the_matrix_api_version_but_gets_an_invalid_response() {
    let mut router = Router::new();
    router.get("/_matrix/client/versions",
               handlers::InvalidJsonResponse { status: status::Ok },
               "get_versions");

    let server_result = start_servers(router);

    let err = server_result.unwrap_err();
    let _msg = String::new();
    assert_error_kind!(err, ErrorKind::InvalidJSON(ref _msg));
}

#[test]
fn startup_failes_when_the_server_cannot_find_a_compatible_matrix_api_version() {
    let mut router = Router::new();
    router.get("/_matrix/client/versions",
               handlers::MatrixVersion { versions: vec!["9999"] },
               "get_versions");

    let server_result = start_servers(router);

    let err = server_result.unwrap_err();
    let _versions = String::new();
    assert_error_kind!(err, ErrorKind::UnsupportedMatrixApiVersion(ref _versions));
}

#[test]
fn startup_failes_when_the_bot_user_registration_failes() {
    let mut router = Router::new();
    router.get("/_matrix/client/versions",
               handlers::MatrixVersion { versions: default_matrix_api_versions() },
               "get_versions");
    let error_responder = handlers::ErrorResponder {
        status: status::InternalServerError,
        message: "Could not register user".to_string(),
    };
    router.post(RegisterEndpoint::router_path(), error_responder, "register");

    let server_result = start_servers(router);

    let err = server_result.unwrap_err();
    let _versions = String::new();
    assert_error_kind!(err, ErrorKind::MatrixError(ref _versions));
}

#[test]
fn startup_failes_when_the_bot_user_registration_returns_invalid_json() {
    let mut router = Router::new();
    router.get("/_matrix/client/versions",
               handlers::MatrixVersion { versions: default_matrix_api_versions() },
               "get_versions");
    router.post(RegisterEndpoint::router_path(),
                handlers::InvalidJsonResponse { status: status::InternalServerError },
                "register");

    let server_result = start_servers(router);

    let err = server_result.unwrap_err();
    let _msg = String::new();
    assert_error_kind!(err, ErrorKind::InvalidJSON(ref _msg));
}

fn start_servers(matrix_router: Router) -> Result<Listening> {
    let homeserver_mock_socket_addr = matrix_rocketchat_test::get_free_socket_addr();

    let (homeserver_mock_tx, homeserver_mock_rx) = channel::<Listening>();
    thread::spawn(move || {
        let mut server = Iron::new(matrix_router);
        server.threads = IRON_THREADS;
        let listening = server.http(homeserver_mock_socket_addr).unwrap();
        homeserver_mock_tx.send(listening).unwrap();
    });

    let (server_tx, server_rx) = channel::<Result<Listening>>();
    thread::spawn(move || {
        let temp_dir = TempDir::new(TEMP_DIR_NAME).unwrap();
        let mut config = matrix_rocketchat_test::build_test_config(&temp_dir);
        config.hs_url = format!("http://{}:{}",
                            homeserver_mock_socket_addr.ip(),
                            homeserver_mock_socket_addr.port());
        let log = DEFAULT_LOGGER.clone();
        let server_result = Server::new(&config, log).run();
        server_tx.send(server_result).unwrap();
    });

    let server_result = server_rx.recv_timeout(matrix_rocketchat_test::default_timeout() * 2).unwrap();
    let mut homeserver_mock_listen = homeserver_mock_rx.recv_timeout(matrix_rocketchat_test::default_timeout()).unwrap();
    homeserver_mock_listen.close().unwrap();
    server_result
}
