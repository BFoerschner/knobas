- **`crates/knobas-source/src/**` — issue #1645 (2026-10-23): a hostile quoted path in a diff
  reads as its characters instead of panicking.** Parent spec #1620 (finding SPI-6). Behaviour
  only, no shape change: no new or changed public item, field, variant, command, event,
  `WriteOp`, migration or module. Recorded under Björn's 2026-10-22 waiver (an Opus implementer
  rules the reserved step).

  **`crates/knobas-source/src/diff.rs`:**
  - `take_quoted` (private, behind `diff::parse` and `diff::parse_view`, so every source's
    `Source::view` diff and the checkout's uncommitted diff) read an octal escape by slicing the
    `&str` at byte offsets `i + 1..i + 4` after a backslash and a digit. When those bytes ended
    inside a multibyte character (`"\1aé.txt"`), the slice panicked. It now takes the three
    bytes after the backslash with `<[u8]>::get` and reads them through the new private
    `fn octal(&[u8]) -> Option<u8>`. The bytes must all be ASCII digits, then go through
    `std::str::from_utf8` and `u8::from_str_radix(_, 8)`. Anything else is not an escape: the
    backslash is dropped and the next character kept, as for any other unknown escape, so
    `"\1aé.txt"` is `1aé.txt`, `"\12x"` is `12x` and `"\+12"` is `+12` (without the digit
    check, `from_str_radix` would read `+12` as byte 10). Only inputs that panicked read
    differently: a well-formed escape (`\303\251` → `é`), a short one before ASCII, and `\n`,
    `\t`, `\r` are read as before.
  - Tests: `a_short_escape_before_a_multibyte_character_is_the_character` (red before: the
    panic) and `an_escape_short_of_three_digits_is_its_characters`.
