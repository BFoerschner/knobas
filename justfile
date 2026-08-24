check: fmt clippy test front

fmt:
    cargo fmt --all --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace

front:
    if [ -d app ]; then cd app && npm run check && npm run build; else echo "no app/ yet"; fi
