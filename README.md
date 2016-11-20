# Matrix <-> Rocket.Chat bridge

This is an application service that bridges [Matrix](https://matrix.org) to
[Rocket.Chat](https://rocket.chat).

**Warning: This application service is still in development. Do not use it!
There will be breaking changs!**

## Compiling from Source

To compile the application service you need Rust >= 1.13.

```
git clone https://github.com/exul/matrix-rocketchat.git
cd matrix-rocketchat
cargo build --release
./target/release/matrix-rocketchat
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
