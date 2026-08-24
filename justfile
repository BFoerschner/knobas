check: fmt clippy test front

fmt:
    cargo fmt --all --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace

front:
    test -d app && (cd app && npm run check && npm run build) || echo "no app/ yet"
