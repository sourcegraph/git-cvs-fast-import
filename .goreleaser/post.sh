#!/bin/sh

# This script is used by goreleaser, and shouldn't be invoked otherwise.

set -e
set -x

# Perform a completely static build using musl to avoid glibc versioning issues
# on older distros. This requires the x86_64-unknown-linux-musl Rust target to
# be available.
RUSTFLAGS='-C link-arg=-s' cargo build --release --target x86_64-unknown-linux-musl

# Overwrite the dummy Go binary.
cp target/x86_64-unknown-linux-musl/release/git-cvs-fast-import dist/git-cvs-fast-import_linux_amd64/git-cvs-fast-import
