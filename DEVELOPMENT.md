# Development

`git-cvs-fast-import` is a fairly standard Rust binary that uses several packages within its workspace to provide functionality. We intend to publish the packages that make sense to publish to crates.io in the future.

## Package structure

### Generic packages

* `comma-v`: a parser for the RCS `,v` format used by CVS.
* `git-fast-import`: a client for the [`git fast-import`](https://git-scm.com/docs/git-fast-import) streaming format.
* `rcs-ed`: an implementation of the subset of [`ed`](https://linux.die.net/man/1/ed) commands [used by RCS](https://www.gnu.org/software/diffutils/manual/html_node/RCS.html).

### Helper packages

* `eq-macro`: a proc macro to derive `PartialEq<[u8]>`, used internally by `comma-v`.

### `git-cvs-fast-import` specific packages

* `src`: contains the source for the `git-cvs-fast-import` binary itself, which intentionally doesn't do very much and mostly delegates to other packages.
* `internal/process`: process management for `git fast-import`.
* `internal/state`: state management and persistence.

## Releasing

To create tags, use [cargo-release](https://github.com/crate-ci/cargo-release), specifically with `--skip-publish` for now until we have the generic packages open sourced and published on crates.io.

Once tagged, GitHub Actions will invoke [GoReleaser](https://github.com/goreleaser/goreleaser) to handle building packages and generating a release changelog. A mild amount of hackery is required to make it work with a Rust program, see [`.goreleaser.yml`](.goreleaser.yml) for the gory details.

For maximum compatibility, binaries are built as static Linux binaries using the `x86_64-unknown-linux-musl` Rust target. Note that this means that non-Rust dependencies can only be added if they can easily be statically linked. (In practice, this hasn't been a problem thus far.)

## Developing

[`tokio-console`](https://github.com/tokio-rs/console) is wired up in debug builds. The default address is used, so just running `tokio-console` should be sufficient to connect.
