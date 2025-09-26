# riki

Simple server for almost-static websites.

## To do

### Configuration

* [ ] Add way to configure hidden files and directories, particularly raw page
      Markdown files and template files.
* [ ] Add way to configure charsets of static files, e.g. `.md`.
* [ ] Add way to configure which files are rendered, e.g. if I want to render
      some `.html` files as well as `.md`.

### Custom elements

* [ ] Links to static files (or pages?) with cache busting.
* [ ] Automatically add `div`s with `style="break-inside: avoid"` to work around
      Safari’s lack of `break-before` support.

### Serving pages

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

## Configuration

The configuration file format is a series of rules. The rules are processed
from start to end; the first rule that matches and has a successful action ends
the processing.

Rules start with a sequence of request path matchers separated by spaces. Each
matcher matches within the context of the previous one, so `/foo bar` would
match `/foo/**/bar`.

  * Glob
    * `foo/*.{html,htm}` — standard [glob syntax].
    * If it starts with `'/'`, then it must be direct descendent of the context.
    * [ ] TODO: case sensitivity? Default to FS behavior?
  * [ ] TODO: Literal path `"path"`.
    * If it starts with `'/'`, then it must be direct descendent of the context.
    * Backslash can be used to escape backslashes, quotes, and what else?
  * [ ] TODO: regex

Each rule ends with an action:

  * `$path` — return the path as a static file
    * [ ] TODO: real variable interpolation
    * [ ] TODO: some way to distinguish this from a path matcher? This could
      be confusing.
  * `error(code)` — return an error corresponding to `code`.
  * `render(path)` — render `path` in a template.
  * `render(markdown(path))` — convert `path` from Markdown to HTML, then render
    it in a template.
  * `root=path` — set the current filesystem directory to `path`. File system
    paths within the context for future rules will use `path` as their current
    working directory.
  * `templates=path` — set the template search path to `path`.

For easier comprehension, rules can be grouped into the same context with
braces. So, every rule within `/foo { ... }` effectively has `/foo` prepended.

  * [ ] TODO: how do we configure error pages?

### Example

    / {
        root=/srv/website
        templates=templates

        *.md error(403)
        /templates error(403)

        render(markdown("$path.md"))
        $path
        error(404)
    }

[glob syntax]: https://crates.io/crates/fast-glob#syntax

## Rust Crate

[![docs.rs](https://img.shields.io/docsrs/riki)][docs.rs]
[![Crates.io](https://img.shields.io/crates/v/riki)][crates.io]
![Rust version 1.85+](https://img.shields.io/badge/Rust%20version-1.85%2B-success)

## Development status

This is in active development. I am open to [suggestions][issues].

## License

Unless otherwise noted, this project is dual-licensed under the Apache 2 and MIT
licenses. You may choose to use either.

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
