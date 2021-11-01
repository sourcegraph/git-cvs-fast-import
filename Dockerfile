FROM rust:1.56.0-alpine@sha256:ad8bbce9644c4e928e5ca5e936832c53e9b97d51d3c255cfdf49cacd46a07cf9 AS builder

RUN apk add --update alpine-sdk
COPY --chown=nobody:nobody . /src/
WORKDIR /src
USER nobody:nobody
RUN cargo build --release

FROM alpine:3.14@sha256:e1c082e3d3c45cccac829840a25941e679c25d438cc8412c2fa221cf1a824e6a

COPY --from=builder /src/target/release/git-cvs-fast-import /usr/local/bin/git-cvs-fast-import
ENTRYPOINT ["/usr/local/bin/git-cvs-fast-import"]
