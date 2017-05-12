extern crate matrix_rocketchat;
extern crate matrix_rocketchat_test;
extern crate tempdir;

use std::fs::File;
use std::io::Write;
use std::net::ToSocketAddrs;

use matrix_rocketchat::Config;
use matrix_rocketchat_test::TEMP_DIR_NAME;
use tempdir::TempDir;

#[test]
fn read_config_from_file() {
    let config_data = r#"hs_token: "hs_token"
                        as_token: "as_token"
                        as_address: "127.0.0.1:8088"
                        as_url: "http://localhost:8088"
                        hs_url: "http://localhost:8008"
                        hs_domain: "matrix.local"
                        sender_localpart: "rocketchat"
                        database_url: "./database.sqlite3"
                        use_ssl: false"#
            .replace("  ", ""); // hacky way to remove the whitespaces before the keys
    let temp_dir = TempDir::new(TEMP_DIR_NAME).unwrap();
    let config_path = temp_dir.path().join("test.config");

    let mut config_file = File::create(&config_path).unwrap();
    config_file.write_all(config_data.as_bytes()).unwrap();
    let config = Config::read_from_file(config_path.to_str().unwrap()).unwrap();
    assert_eq!(config.hs_token, "hs_token".to_string());
    assert_eq!(config.as_token, "as_token".to_string());
    assert_eq!(config.as_address, "127.0.0.1:8088".to_socket_addrs().unwrap().next().unwrap());
    assert_eq!(config.as_url, "http://localhost:8088");
    assert_eq!(config.hs_url, "http://localhost:8008");
    assert_eq!(config.hs_domain, "matrix.local");
    assert_eq!(config.sender_localpart, "rocketchat");
    assert_eq!(config.database_url, "./database.sqlite3");
    assert_eq!(config.use_ssl, false);
}
