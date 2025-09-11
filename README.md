# riki

Simple server for almost-static websites.

## To do

### Configuration

* [ ] Add way to configure hidden files and directories.
* [ ] Add way to configure which files are rendered, e.g. if I want to render
      some `.html` files as well as `.md`.

### Custom elements

* [ ] Links to static files (or pages?) with cache busting.
* [ ] Automatically add `div`s with `style="break-inside: avoid"` to work around
      Safari’s lack of `break-before` support.

### Serving pages

* [ ] Combine page and static directories
* [ ] Cache headers for rendered pages

## Installation

```sh
cargo install riki
```

If you have [`cargo binstall`][binstall], you can use it to download and install
a binary:

```sh
cargo binstall riki
```

Finally, you can download binaries directly from the [GitHub releases
page][releases]. Just extract the archive and copy the file inside into your
`$PATH`, e.g. `/usr/local/bin`. The most common ones are:

  * Linux: [x86-64](https://github.com/danielparks/riki/releases/latest/download/riki-x86_64-unknown-linux-gnu.tar.gz),
    [ARM](https://github.com/danielparks/riki/releases/latest/download/riki-aarch64-unknown-linux-musl.tar.gz)
  * macOS: [Intel](https://github.com/danielparks/riki/releases/latest/download/riki-x86_64-apple-darwin.tar.gz),
    [Apple silicon](https://github.com/danielparks/riki/releases/latest/download/riki-aarch64-apple-darwin.tar.gz)
  * [Windows on x86-64](https://github.com/danielparks/riki/releases/latest/download/riki-x86_64-pc-windows-msvc.zip)


## Rust Crate

[![docs.rs](https://img.shields.io/docsrs/riki)][docs.rs]
[![Crates.io](https://img.shields.io/crates/v/riki)][crates.io]
![Rust version 1.85+](https://img.shields.io/badge/Rust%20version-1.85%2B-success)

## Development status

This is in active development. I am open to [suggestions][issues].

## License

This project dual-licensed under the Apache 2 and MIT licenses. You may choose
to use either.

  * [Apache License, Version 2.0](LICENSE-APACHE)
  * [MIT license](LICENSE-MIT)

### Contributions

Unless you explicitly state otherwise, any contribution you submit as defined
in the Apache 2.0 license shall be dual licensed as above, without any
additional terms or conditions.

[docs.rs]: https://docs.rs/riki/latest/riki/
[crates.io]: https://crates.io/crates/riki
[binstall]: https://github.com/cargo-bins/cargo-binstall
[releases]: https://github.com/danielparks/riki/releases
[issues]: https://github.com/danielparks/riki/issues
