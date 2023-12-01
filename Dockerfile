FROM rust:1.58.1-alpine@sha256:b61698ea823c6f9bc726272d7783867d89e79ca87e9944998739ce619da7699a AS builder

RUN apk add --update alpine-sdk
COPY --chown=nobody:nobody . /src/
WORKDIR /src
USER nobody:nobody
RUN cargo build --release

FROM alpine:3.15@sha256:19b4bcc4f60e99dd5ebdca0cbce22c503bbcff197549d7e19dab4f22254dc864

COPY --from=builder /src/target/release/git-cvs-fast-import /usr/local/bin/git-cvs-fast-import
ENTRYPOINT ["/usr/local/bin/git-cvs-fast-import"]
