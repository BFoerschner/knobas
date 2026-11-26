# The update signing keys' public halves

`primary.pub` and `spare.pub` go here: the two public keys every knobas build trusts for an update (ADR-0052,
decisions D2). Björn makes both pairs by hand (`docs/development.md`, *Update signing*); the private halves never come
near the repository. Until both files are here, `build.rs` compiles no key in, and Settings › Updates says updates
aren't set up for that build. One without the other fails the build.

The keys under `tests/fixtures/` are throwaway test keys. `the_shipped_keys_are_not_the_fixture_keys` and the release
run's key-id check stop a build that ships one of them.
