# The cargo recipes drop RUSTUP_TOOLCHAIN so rust-toolchain.toml is what
# decides the toolchain. An inherited RUSTUP_TOOLCHAIN would silently
# override the pin and let the gate pass on an unpinned compiler.

# `front` runs before the cargo recipes, not after: `tauri::generate_context!`
# embeds the built frontend at compile time, so `app/dist` has to exist before
# anything compiles knobas-app. On a fresh clone the reverse order fails to
# build rather than failing a test.
check: fmt front clippy test

fmt:
    env -u RUSTUP_TOOLCHAIN cargo fmt --all --check

clippy:
    env -u RUSTUP_TOOLCHAIN cargo clippy --workspace --all-targets -- -D warnings

test:
    env -u RUSTUP_TOOLCHAIN cargo test --workspace

front:
    #!/usr/bin/env bash
    set -euo pipefail
    cd app
    # `npm ci` only when the tree is absent: it deletes and reinstalls
    # node_modules wholesale, which is right for a fresh clone and pure waste
    # on every subsequent gate run.
    [ -d node_modules ] || npm ci
    npm run check
    npm run build

# The desktop app against the embedded database. Set KNOBAS_DB_URL to point it
# at a server you manage instead.
#
# The Tauri CLI is a devDependency of `app/` (there is no `cargo tauri-cli` in
# this repo), and it has to run from the crate that owns `tauri.conf.json`, so
# the binary is named by path rather than found on PATH.
dev:
    cd crates/knobas-app && ../../app/node_modules/.bin/tauri dev
