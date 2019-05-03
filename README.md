![logo](https://cloud.githubusercontent.com/assets/1886214/25308549/323fd2f8-27b7-11e7-9d2c-e7e8380a686d.png)
=======

[![Build Status](https://travis-ci.org/exul/matrix-rocketchat.svg?branch=master)](https://travis-ci.org/exul/matrix-rocketchat)
[![Coverage Status](https://coveralls.io/repos/github/exul/matrix-rocketchat/badge.svg?branch=master)](https://coveralls.io/github/exul/matrix-rocketchat?branch=master)

This is an application service that bridges [Matrix](https://matrix.org) to
[Rocket.Chat](https://rocket.chat).

**Warning: This application service is still in development. Do not use it!
There will be breaking changes!**

![matrix-rocketchat](https://cloud.githubusercontent.com/assets/1886214/24167507/457d5ea2-0e77-11e7-8102-c14e4c04e4dd.png)


## Compiling from Source

To compile the application service you need Rust >= 1.34.

It's highly recommended that you use [rustup](https://www.rustup.rs).

```
git clone https://github.com/exul/matrix-rocketchat.git
cd matrix-rocketchat
cargo build --release
./target/release/matrix-rocketchat
```

## Dependencies

SQLite is used to store the data:

```
# On Ubuntu
sudo apt-get install libsqlite3-dev

# On Arch Linux
sudo pacman -S sqlite
```

If you are using the application service on Linux, you'll have to install OpenSSL:

```
# On Ubuntu
sudo apt-get install libssl-dev

# On Arch Linux
sudo pacman -S openssl
```

## HTTPS

It's strongly recommended to use HTTPS when running the service!

The HTTPS configuration can either be done as part of the application service or a reverse proxy can be used.

### Application Service

The service can be exposed via HTTPS by providing a PKCS 12 file and a password to decrypt the file.

To convert a certificate and a private key into a PKCS 12 file, the following command can be used:

```
openssl pkcs12 -export -in fullchain.pem -inkey privkey.pem -out cert.p12
```

The command will prompt for a password.

Configuration parameters:

```
as_address: "0.0.0.0:8822"
as_url: "https://matrix-rocketchat.example.org:8822"
use_https: true
pkcs12_path: "/pass/to/cert.p12
pkcs12_password: "p12password"
```

### Reverse Proxy

The application service can be run behind a reverse proxy and let the reverse proxy handle the HTTPS.

In this case, it's important to bind the application service only to localhost!

NGINX example config:

```
http {
  ssl_certificate       /etc/letsencrypt/live/example.org/fullchain.pem;
  ssl_certificate_key   /etc/letsencrypt/live/example.org/privkey.pem;
  ssl_protocols         TLSv1.2 TLSv1.1;
  ssl_ciphers           EECDH+AESGCM:EDH+AESGCM:AES256+EECDH:AES256+EDH;

  server {
    server_name  matrix-rocketchat.example.org;
    listen       443 ssl;
    location / {
      proxy_pass          http://localhost:8822/;
      proxy_set_header    Host            $host;
      proxy_set_header    X-Real-IP       $remote_addr;
      proxy_set_header    X-Forwarded-for $remote_addr;
      port_in_redirect    off;
    }
  }
}
```

Configuration parameters:

```
as_address: "127.0.0.1:8822"
as_url: "https://matrix-rocketchat.example.org"
use_https: false
```

## Acknowledgement

I learned a lot by reading the code of the following projects:
* [Ruma](https://github.com/ruma/ruma) (a Matrix Server mainly written by
  [Jimmy Cuadra](https://github.com/jimmycuadra))
* [Gitter Bridge](https://github.com/remram44/matrix-appservice-gitter-twisted)
  (mainly written by [Remi Rampin](https://github.com/remram44))

From the first one I learned a lot about [Rust](https://www.rust-lang.org) and
[Iron](https://github.com/iron/iron). The second one helped me to understand
how a Matrix bridge works.

## Logo

Special thanks to [Steffi](http://schriftundsatz.ch) who created the logo for this project.

## License

Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
  be dual licensed as above, without any additional terms or conditions.
