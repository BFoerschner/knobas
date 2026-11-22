# 2026-11-22: The system clipboard is proven by hand, under a private display

**Asked.** The final suite's `cargo mutants --in-diff` over the Passwords code (ADR-0056) left five mutants that no
automated test can kill: the bodies of `SystemClipboard`'s three methods in
`crates/knobas-app/src/passwords/clipboard.rs` (`set_secret` as `Ok(())`; `text` as `Ok(None)`, `Ok(Some(""))` and
`Ok(Some("xyzzy"))`; `clear` as `Ok(())`). They are the thin `arboard` shim behind `ClipboardPort`; the `Keeper` that
decides what to clear is tested through a fake port. Björn's rule is that survivors get tests or are shown equivalent,
and nothing is waived; his other rule is that no test touches his clipboard. Björn was away, so Fable (`claude-fable-5-2`,
xhigh) ruled.

**Taken.** An `#[ignore]`d round trip, `the_system_clipboard_holds_reads_and_clears_a_secret` in `clipboard.rs`, Linux
only (arboard is built without default features, so X11 is its only Linux backend). It refuses to run unless
`KNOBAS_PRIVATE_DISPLAY` names the same display as `DISPLAY`, the display is not `:0` and `WAYLAND_DISPLAY` is unset:
WSLg exports `DISPLAY=:0` in every WSL shell and shares that clipboard with Windows, so "a display is set" is true by
default on the box that runs it. It also refuses a clipboard that is not empty, so it cannot overwrite something a person
copied, and it writes a per-run value, so a leftover selection cannot let the `set_secret` mutant pass. Run it on the
WSL box with:

```text
xvfb-run -n 99 env -u WAYLAND_DISPLAY KNOBAS_PRIVATE_DISPLAY=:99 \
    cargo test -p knobas-app --lib system_clipboard -- --ignored
```

**What that means for the mutants runs.** An automated `--in-diff` run does not execute ignored tests, so these five
stay *missed* in every such run, and this note is where they are accounted for. Whenever a diff touches
`SystemClipboard`, run the test again with each of the five mutants applied by hand and record what failed.

**The proof of 2026-11-22.** On the WSL box, each run under its own `xvfb-run -a` (display `:99`), each mutant applied
by hand as an early `return` of the mutant's value:

| Mutant | Result |
|---|---|
| none | passes |
| `set_secret` as `Ok(())` | fails: reads `None` back where the secret should be |
| `text` as `Ok(None)` | fails: reads `None` back where the secret should be |
| `text` as `Ok(Some(""))` | fails: the clipboard is not empty before the write |
| `text` as `Ok(Some("xyzzy"))` | fails: the clipboard is not empty before the write |
| `clear` as `Ok(())` | fails: "the secret stayed" |

With `DISPLAY=:0` named twice, the guard panics before any clipboard is opened.

**Not taken.** Declaring the five out of reach (a three-line delegation) would have been a waiver. A test on the Mac or
on Björn's desktop would touch his clipboard.
