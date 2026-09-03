# syntax=docker/dockerfile:1
#
# Build context is the REPO ROOT (see docker-compose.yml's `build.context: ..`):
# knobas-mockd's build.rs reads testenv/specs/jira-dc-rest.wadl to generate the
# request allowlist, and knobas-source-mock include_str!s
# fixtures/tidewater/work.json. Neither is reachable from a context rooted at
# testenv/.
#
# Both stages are pinned by digest from testenv/.env (see pin-images.sh). The
# defaults below keep a bare `docker build -f testenv/mockd.Dockerfile .`
# working outside compose; compose always passes the pins.
ARG RUST_IMAGE=rust:1-slim
ARG RUNTIME_IMAGE=debian:trixie-slim

FROM ${RUST_IMAGE} AS build
WORKDIR /src
COPY . .
# rust-toolchain.toml pins the channel (1.94), so rustup fetches exactly that
# compiler here rather than using whatever the base image happens to ship --
# the container and `just check` then build with the same rustc.
#
# Only knobas-mockd and its dependency closure are built. The workspace's other
# members are not: knobas-app links webkit2gtk through tauri, which is not in
# this image and would need ~400 MB of GTK to become so.
RUN cargo build --release -p knobas-mockd --bin mockd

FROM ${RUNTIME_IMAGE}
# curl is here for the compose healthcheck and nothing else -- a healthcheck is
# the only way compose learns the difference between "the process started" and
# "the port answers", and `depends_on: condition: service_healthy` is what the
# seed waits on. ca-certificates comes with it because a curl without one is a
# trap the next person falls into; mockd itself makes no outbound calls.
RUN apt-get update \
 && apt-get install --no-install-recommends -y curl ca-certificates \
 && rm -rf /var/lib/apt/lists/*
RUN useradd -r -u 10001 mockd
COPY --from=build /src/target/release/mockd /usr/local/bin/mockd
USER mockd
# 8213 (Flowrun, M4) is reserved by interfaces §5 and deliberately not exposed:
# that API does not exist yet. 8211 is unreserved (ADR-0013, 2026-09-03: mockd
# is deprecated and gets no Confluence half).
EXPOSE 8200 8210 8212
ENTRYPOINT ["/usr/local/bin/mockd"]
