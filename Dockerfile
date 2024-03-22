FROM rust:1.77.0-alpine@sha256:4a7925c3974fab3fb68b3f4d93d1a4a36cb201f9f87b01014def219b125f1960 AS builder

RUN apk add --update alpine-sdk
COPY --chown=nobody:nobody . /src/
WORKDIR /src
USER nobody:nobody
RUN cargo build --release

FROM alpine:3.15@sha256:21a3deaa0d32a8057914f36584b5288d2e5ecc984380bc0118285c70fa8c9300

COPY --from=builder /src/target/release/git-cvs-fast-import /usr/local/bin/git-cvs-fast-import
ENTRYPOINT ["/usr/local/bin/git-cvs-fast-import"]
