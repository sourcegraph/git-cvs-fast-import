FROM rust:1.56.1-alpine@sha256:8dc667e247d933bb933b45eeaf0e3e97982b77ed91823d74befc0f86f21ccc8c AS builder

RUN apk add --update alpine-sdk
COPY --chown=nobody:nobody . /src/
WORKDIR /src
USER nobody:nobody
RUN cargo build --release

FROM alpine:3.14@sha256:635f0aa53d99017b38d1a0aa5b2082f7812b03e3cdb299103fe77b5c8a07f1d2

COPY --from=builder /src/target/release/git-cvs-fast-import /usr/local/bin/git-cvs-fast-import
ENTRYPOINT ["/usr/local/bin/git-cvs-fast-import"]
