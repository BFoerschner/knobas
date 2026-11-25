# The suite red on ux-polish: four causes, four fixes (2026-11-25)

The one-off commands builder, gating its branch, found failures that also failed on `ux-polish` itself: the press trip
`strip-walk-gitea` (F-556) and 13 `just test` failures in three binaries. Light CI runs neither. Each is reproduced
on `fb2b0543` and fixed at its cause on `suite-red-fix`.

## Assets: `→` onto a holder (user-facing, `ee0e53a6`)

The trip failed at all three widths: `→` on *knobas-teamcity* left focus on *teamcity* outside any column. Bisected
on WSL (`f4343b49` good, `fb2b0543` bad; the last steps by reading, once WSL went down) to `480df5f7` (2026-11-23,
in every release since ux30), which kept `asset_tree` in the answer cache. `→` moves the address before the asset is
read, so for a moment `path` is the placeholder, the top column alone. Its read used to lose to the asset's read and
never land. Kept, it answered at once: the strip drew one column, and the focus effect focused an ancestor in it and
stopped. The 2026-11-20 compare note saw exactly this when the same caching was tried then.

The columns now stand while the address names an asset whose read is out and no walk holds a path for it. An address
opened cold still reads the top column first. The cache keeps its columns, so the ux30 Windows gains stand. A person
walking Assets by keyboard met it in the release. `AssetsView.test.svelte.ts` holds `getAsset` back while the columns
answer at once: before, the strip fell to `[hel1]`; now it stands, and focus ends in the new column.

## `console_choke_point` (`39a1909f`)

J2's jq session (`5fbcf578`) started the app's own binary with a bare `Command::new`. It now goes through
`knobas_db::console::command`, with the environment the jq step's child gets: the parent's less every `KNOBAS_*`
variable (#1643). Nobody met it: the release binary is `windows_subsystem = "windows"`, which Windows never gives a
console.

## The WASM host's older components (`6a35f02f`)

Eleven tests of 1.0.0 to 1.6.0 components failed under `just test` with `CARGO_TARGET_DIR` set. `just wasm-fixture`
built into that folder and copied from `./target`, so each `_v1_x` file was whatever `./target` held. Since `4201adf1`
(2026-11-22) a test that finds the fixture missing builds the current one there, so every older fixture was the 1.7.0
component under another name (all five files 1,819,184 bytes on WSL). The recipe now builds as `build_fixture` does:
in `target/wasm-fixture`, whatever `CARGO_TARGET_DIR` says, one list and one order. The tests are unchanged.

## The narrow-index plan check (`7e0c6e19`)

`a_search_with_words_reads_through_the_narrow_tables_indexes` failed one run in six on the Mac. Nothing it reads had
changed since the ux30 gate's green run: it read the file's shared database, which holds what the tests beside it seed
and write. In the failing run the mirror held 992 rows and 606 matched, and walking it whole is the right plan for
that. Not a plan regression. The test now seeds a scratch database of its own: 14 runs of 14 green.

## What ran, and what is owed

On the Mac: `just check-front` (svelte-check 0 and 0 over 2,102 files, 7,471 tests, the build), clippy on
`knobas-app`, `knobas-search` and `knobas-wasm-host`, `cargo fmt`, and the three crates' whole suites: `knobas-app`
1,939 passed, `knobas-search` 451, `knobas-wasm-host` 118, none failed. Owed until the WSL box is back: the full
`just test`, and the press trips `--strict` with 5 workers (the Assets fix by the trip itself).
