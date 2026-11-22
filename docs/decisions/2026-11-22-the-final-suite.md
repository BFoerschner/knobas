# 2026-11-22: The final suite before ux29 and ux30, and what it fixed

Björn: "only run the full suite after the last feature has been implemented, then keep fixing until all is fixed".
Everything in flight was implemented: ticket moving and fields (ADR-0054), app-wide themes (ADR-0055), the in-app
site's saved sign-in with WASM adapters at 1.7.0 (ADR-0050 amended), and Passwords with Pleasant (ADR-0056), merged
from `ux30-passwords` in `4c29ca7c` (migration `0102`, next after `0101`; `migrations.sha384` checked). This note is
what the suite found and how each was settled. Rulings by Fable while Björn was away are marked.

It did not end as the final gate. WSL was blocked at about 19:15 (Björn), and Björn then moved the end gate to after
ux31 ("with all the timing tests and smoothness tests"). What it ran, on which head, and what it leaves open (each on
`small-fixes`, for the end gate) are at the end.

## Found and fixed

- **A pull request drew its commit picker over the top bar** (`b7c2220d`). The ticket fields' people and labels pickers
  took `.tf-pick` for their float (`position: fixed` at 0,0), a name the commit picker already had in `code.css`. So
  every pull request drew its commit range at the window's top left, over the Inbox entry, and a press there went
  nowhere. Found by the press trips `nav-place-pr` and `nav-reopen-pr`, bisected to `720bc0ab`. The field pickers are
  `.tf-fpick`; a new `tf-css` test holds that a class one stylesheet floats has its bare rule there alone.
- **Three pages moved once drawn** (`just moves`). A ticket's card took its scrollbar's room when its fields landed
  (`7a55dcf1`: the card keeps it). A link to a file drew its tree in steps as each folder on the path was read
  (`8b2a6ca7`: the tree is drawn down to the first folder still reading). A press in the asset tree moved the address
  before the asset was read, so the columns showed the new asset while the pane still held the old (`87b4e568`: the
  press reads first and moves the address with the answer).
- **The moves table and probe were behind** (`83dffacf`, the movement run's errors). Places now start from no stored state (an
  earlier place's own theme or moved ticket was the next one's start); the ticket moves press *In progress*, as the
  menu names a status since the board's columns took sentence case; *Duplicate and edit* and *Set apart* are pressed
  once, since the press takes away what was pressed. The probe waited for the address's `hashchange` only when the
  place's prefix was new, so from one asset to another under `#/asset/` it counted the frame between the address and
  its `hashchange`, which still draws the page left; it now waits whenever the address moves.
- **The colour-blind themes failed their own mode** (`d75bfbdb`, Fable). With the mode on and *knobas Light/Dark for
  colour blindness* chosen, Settings › Appearance still warned of pairs with Unknown; the tests had left Unknown out.
  Unknown stays a state (a pill comes in five tones) and its grey stays; *Tell them apart* may move Running in
  lightness only. Both themes retuned; every two of the five states are 0.08 apart, each 4.5:1.
- **A language the machine lacks stopped knobas making its database** (`9ae84b4d`). `initdb` took `lc_monetary`,
  `lc_numeric` and `lc_time` from the environment, and an `LC_ALL` naming an ungenerated locale (a fresh WSL, a
  container) failed it. They are fixed now, as collate and ctype were.
- **Passwords' command issued SQL of its own** (`4e126465`, #1870's rule) and **its filter box missed `filterKeys`**
  (`3a18e7c5`); **the event mirror test lacked `clipboard:state`** (`bcbdb264`); **`live_pleasant.rs` was not on the
  test-layout list** (`bdcd514a`): the Passwords branch's slips, caught by the full `just test`, `check-front` and
  `check-scripts`.
- **Flaky tests, by their causes:** scratch database names over PostgreSQL's 63 bytes met (`6c951d96`); scheduler
  tests waited five seconds among the newest fifty runs (`8bb62daa`); the archive-deadline test's two passes ran on
  400 ms, and its first one could run out before the archive was asked (`d34125c1`, `b3101ff8`); the held-initdb tests made what they executed *Text file busy* under a parallel fork
  (`0bbeec99`).
- **Gates:** workspace clippy (`e3fd9263`), Windows clippy (`fc754f26`, and `just clippy-windows`), svelte-check's
  warnings (`59e646c2`, now failing the gates), two compile warnings svelte-check never reported (`68ac4f72`).
- **Test harness:** the WASM fixture builds itself when missing or stale (`4201adf1`), so a fresh clone's
  `cargo test -p knobas-app` and every `cargo mutants` copy pass their baseline.
- **Press trips:** four tables caught up with ux29 (`3e12a723`).
- **`just test` failed a binary that printed no result line** (`6487951f`): knobas-site's harness-less `mac_forget`
  printed nothing on Linux and only its own test line on the Mac; it prints libtest's `test result:` line itself now.
  check-scripts then failed its layout: a `[[test]]` with a `path` is a second binary; it is a listed file of its own
  (`15fab040`).
- **Runs too long for one turn on a machine's queue** take `--part k/m` (`moves.sh` and `long-paths.mjs` in `862a170d`,
  `trips.mjs` beside this note).

## Mutants

`cargo mutants --in-diff` over `v3.2.9-ux28..HEAD`, crate by crate (phase 1, each crate's own tests), then the
survivors again with the tests of the crates that reach them (phase 2: `knobas-http` with Pleasant's, the SDK and the
WASM fixture with the WASM host's, `mockd` with the Jira adapter's). Each survivor got a test that fails under it, or
is named equivalent below. Fable ruled (2026-11-22) that a one-line call into a native web view is killed by the real
window run that would show it broken, and that the quit-race guards are built as mutants and run through quit and
restart; and that the secret's wipe needs a test, not a reading (`tests/zeroize.rs`, a witness allocator).

**Equivalent** (each a mutant that cannot change behaviour): trait default bodies replaced by what they already
return (`Source::ticket_fields`, `field_options`, `Sink::otp_key`, the SDK's `Adapter::ticket_fields` and
`field_options`, `SiteHost::flush`); `mac::flush` and `WryHost::flush` on the Mac, which already do nothing; a
comparison of an `Instant` with itself at one boundary (`FakeHost::flush`, Pleasant's cached token); the WASM
fixture's `reach("Done")`, which no fixture ticket stands in; `supervise`'s `cfg(not(unix))` body, not compiled on
macOS or Linux and unviable where it is (`ChildStdin` has no `Default`). The OTP truncation's `|` turned `^` (its four
terms sit in separate bit ranges), and two clipboard-keeper mutants that keep each copy's number unique and the stale
timer harmless.

**Killed in the real window** (Fable's ruling): `run`, `open_main_window`, the capture window's `open_window`,
`TauriShell::sign_in` and `without_white` are one-line calls into the window or the web view that no test process has;
the real-window runs on the Mac, on Windows and on WSL's Linux build open each and would show each broken. The five
`SystemClipboard` mutants are killed by a Linux round trip under a private display, run by hand
(`docs/decisions/2026-11-22-the-system-clipboard-is-proven-by-hand.md`).

**knobas-app** ran on its lib's tests first (each mutant links its lib alone), then its survivors again with the `it`
tests that reach them: every bring-up, archive and site survivor was caught there. Those the `it` tests did not reach
got tests: the site and window commands through Tauri's IPC (`cce28955`), Passwords' commands
(`e1894830`, a sub-agent), the WASM fixture's options filter (`1e433d91`).

## Passwords against the real Pleasant

Björn asked how knobas behaves where Pleasant's own KeePass client drops the connection and asks for the password
again: at a password the account may not see. As the reader in the Pleasant VM, in the real Linux window: an entry
the sync already knows is unreadable says "You may not read this entry's password." and sends nothing; a readable one
copied right after costs one *Password Fetched* in Pleasant's log and no new *Identity Verified*. A refusal Pleasant
itself answers (a right taken away after the sync) is an answer with Pleasant's sentence, and the token stays: only a
401 drops it. The live suite holds that against the VM (`470e5d83`).

## What ran, and what is open

On WSL at `6070baf9` (the merged ux30-pleasant-site, `14810fee`, and after): `just test` 5838 passed, 0 failed, 274
ignored, with `mac_forget` flagged (fixed above); `just check-front` clean (svelte-check 0 and 0, 469 files and 6852
tests, the build); `just moves` at 1440x868 in four parts and at 2560x1440 in parts 1 and 2, nothing moved. On the Mac
after WSL was blocked: `just moves` at 2560x1440 part 3 at `c26413ab`, nothing moved; the strict press trips' first
half at `15fab040`, 186 of 186. Before the merge: the press trips 371 of 371, check-scripts and check-front.

Open, each on `small-fixes` with its reason: the moves' last 2560 part and the trips' second half; long paths; the
probes (drag at 40 and 250 cards, theme switch, Passwords' lines); check-scripts after `15fab040`; `just test` and
check-front on the head with ux30's last two pieces (`27a63607`, `9fd9a7c7`); knobas-app's last mutants (the code
added since the first shards: 19 never run, and 38 that survived its lib's tests not yet rerun against the `it` tests,
which since have tests for many of them); the Linux, Windows and live-product checks, which need WSL.
