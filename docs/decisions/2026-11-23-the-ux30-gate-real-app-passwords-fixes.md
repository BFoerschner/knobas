# The ux30 end gate's real-app Passwords run: three fixes (2026-11-23)

The ux30 end gate ran the Passwords main path in the Linux build under Xvfb on the WSL box (scratch HOME, real XTest
input) against the real Pleasant 9.5.4 server in its VM, as the writer `tomasz.wojcik` and the reader
`mara.lindqvist`. Three things were wrong; each is fixed with a test that failed before.

## A blocked folder's entries asked for a usage comment

Pleasant's `Tidewater/Payments/Prod` asks for a comment on every password read, and `Payments/Prod/PSP` blocks access
inheritance, which blocks that rule too. The server says so on every entry: in the whole-tree `GET folders` the PSP
entries carry `CommentPrompts.AskForCommentOnViewPassword: false`, and `IsCommentRequired` with
`ViewEntryPassword` answers `false` for them (measured as the writer, 19:18). knobas laid the folder above over the
entry's own prompts (`rules(inherited, own)`, an OR), so *PSP dashboard* and *PSP live secret key* said "Asks for a usage
comment first: Payments / Prod / PSP requires one" and asked.

**Now:** an entry's own `CommentPrompts` are its rule, since the server sends each entry the rule in force for it;
the folders' rule is used only for an entry that carries none. Folders keep the inherited OR: a folder read alone
(`GET folders/{id}`) says false for every rule (recorded), so the rule above still counts there.
`map.rs`: `an_entrys_own_comment_rule_wins_over_the_folder_above_a_blocked_one`.

## Edit could not clear a field

Emptying *Notes* (or *Username*, *Address*, *Expires*, or moving an entry to the source's top) and pressing *Save*
left the form open with "protocol: the edit changes nothing, so nothing was sent". The page sends a cleared field as
`null`; `Write::EditEntry`'s fields are `Option<Option<_>>` with `#[serde(default)]`, and serde reads `null` there as
"left out". So no cleared field ever reached the source.

**Now:** those five fields read `null` as "clear it" (`null_is_a_value`) and a field left out as unchanged. The wire
format and the generated TypeScript are unchanged. `passwords.rs`:
`an_edits_null_clears_the_field_and_a_missing_one_leaves_it`. In the real run the entry was put back through the API
(only its `Modified` time differs from the seed).

## Linux said "macOS" in a source's Site section

On Linux, where no site is offered, Settings › Sources › ⟨source⟩ › Site › *Sign-in* read "Kept until knobas quits
on this macOS" (the hint for a macOS before 14). It now reads "Nothing kept: no site here" wherever the site is not
offered. `TfSiteSignIn.test.svelte.ts`.

## After the fixes, in the Mac window

A debug build with the three fixes, a scratch HOME, `testenv/pleasant/standin.mjs` (20 entries, the same comment
rule and PSP block) as the writer, presses through the dev server's probe (the WSL box was Björn's again, so not the
VM): *PSP dashboard* says "The password manager records each read" and *Show* read it with no comment asked;
*ledger-api database user* still asks ("Payments / Prod / Databases requires one"); *Guest Wi-Fi*'s Notes set and then
emptied, each *Save* a `PATCH` answered 204, the form closed and the Notes row gone.

## Seen and left as designed

- A restart signs in to Pleasant again: one "Identity Verified" per start. The bearer token is kept per process only
  (`token_key` hashes the password with the process's own seed and is never stored), so this is by design.
- `SIGTERM` to the app left its embedded PostgreSQL running, on Linux and on the Mac alike; the harnesses stopped it
  by PID. A person quits through the window or ⌘Q, which these runs did not use.
