# The cargo recipes drop RUSTUP_TOOLCHAIN so rust-toolchain.toml is what
# decides the toolchain. An inherited RUSTUP_TOOLCHAIN would silently
# override the pin and let the gate pass on an unpinned compiler.

check: fmt clippy test front

fmt:
    env -u RUSTUP_TOOLCHAIN cargo fmt --all --check

clippy:
    env -u RUSTUP_TOOLCHAIN cargo clippy --workspace --all-targets -- -D warnings

test:
    env -u RUSTUP_TOOLCHAIN cargo test --workspace

front:
    if [ -d app ]; then cd app && npm run check && npm run build; else echo "no app/ yet"; fi
