---
status: accepted
---

# Passwords are a role, a secret never rests, and a password manager is synced by hand

Björn, on PR #2479: a Passwords page with Pleasant Password Server as its first source, built from the clickdummy
(`mockups/passwords-2/`). Designed by Fable (`passwords-decisions.md`, 35 sections), amended by Björn's answers of
2026-11-22 and by what a real Pleasant 9.5.4 answered (`testenv/pleasant/README.md`,
`docs/decisions/2026-11-22-pleasant-real-server-and-stand-in.md`). The note is
`docs/decisions/2026-11-22-passwords-are-a-role-and-a-secret-never-rests.md`. **The hard rule: knobas must not fill a
password manager's audit log with the person's name.**

## Decided

- **Passwords is a role** (ADR-0029's sense), the sixth: a source fills it by emitting two kinds, `password_entry`
  and `password_folder`, with fixed records the role defines (`knobas_source::passwords`), and by answering one live
  read, `read_secret`. Beyond that minimum a source declares, in `SourceDescriptor::passwords`, which of eight
  abilities it offers (one-time codes, expiry, usage comments, an audit log, history, attachments, access requests,
  the direct writes), and the page draws a control only where the entry's source declares it (ADR-0020). The role's
  check holds each optional read to its declaration both ways. External (WASM) adapters may not fill it yet.
- **A secret never rests.** A password, a field a source marks protected, an earlier password and an attachment's
  bytes are read live, once per press, held in the app process for at most 30 seconds (both clocks, so a Mac that
  slept does not extend it), and written to no table, file, log, cache or queue. The type that carries one,
  `SecretText`, prints `<redacted>` and zeroes its bytes on drop. Only `password_show` sends one to the window; a copy
  is made on the Rust side.
- **The clipboard holds a copy for 30 seconds** and is cleared only if it still holds what knobas put there (its
  SHA-256, kept in memory), on quit too. On macOS the copy is marked concealed, on Windows it is kept out of the
  clipboard history and the cloud clipboard, on Linux it carries KDE's password hint.
- **A code key is kept, not mirrored.** A sync hands each entry's one-time-code key to the sink beside the item
  (`Sink::otp_key`), never in its payload. Keys reach the system's password store only when the run commits, in
  chunks of at most 1,200 characters under one write id, and a run that walked every entry replaces them. Codes are
  made on the machine (RFC 6238), so they read nothing from the source, ask no usage comment and work offline.
- **A password entry's title stays in the mirror.** `KindInfo::title_stays_in_mirror` keeps it out of
  `knobas.entity`, so link lists, link sets, the timer, the standup and the launcher's generic actions never name an
  entry: passwords take part in none of them. Search reads the mirror's title, so an entry is still found by it.
- **Notes and openly sent fields are mirrored and searched** (Björn's answers 1 and 2): what the source shows openly
  may rest in the mirror; what it hides behind a reveal never does. They stay out of backups and share exports.
- **A password manager is synced by hand by default.** A source carries `sync_by_hand` (migration 0102): it is never
  due, starts no run after a write, and its sources line says *by hand*. An adapter that may write an audit line per
  sync declares `by_hand_by_default`; Settings › Sources › *Syncs* turns it on or off for any source.
- **A write to a password manager is direct.** *Add*, *Edit* and *Change password* are sent once when *Save* is
  pressed and answered before the dialog closes. They are a `passwords::Write`, which cannot become a `WriteOp` (a
  compile-fail doctest holds it), so a new password never waits in the write queue. If the source does not answer,
  nothing is kept. The entry the source answers is written into the mirror under the source's run lock
  (`refresh::write_direct`), with no run after it.

## Pleasant, as the real server answers (9.5.4)

- **The audit log**: only a sign-in ("Identity Verified") and a password read ("Password Fetched", with its usage
  comment) write a line. The whole tree, listings, an entry by id, custom fields, attachments and search write
  none. So a sync is one token request and one tree request, the token (12 hours) is kept in memory and reused, and
  a sync within its life writes nothing. The adapter never calls `GET entries/{id}` or the offline package.
- **Codes**: Pleasant's own `TOTPSecret` first, then the four KeePass plugins' fields. An account without
  `HasViewTOTPAccess` gets no key, though the listing sends one.
- **No field is marked protected**, and listings send every value. Fable ruled, for Björn while he was away, that
  every such value is mirrored and searched, and Settings says so (`Offers::marks_protected_fields`, false for
  Pleasant). Code-key fields never are.
- **The comment rule is the entry's**, which carries the inherited rule in every listing; folder objects carry it
  only in the whole-tree call. Errors are JSON in a `text/plain` body and are read by their words: a missing comment
  is `400 "Comment required."`; a read the account's level withholds is `403 "Access denied. … does not have
  permission to view the password for …"`, which the page says as the server words it.
- **Attachments** are named in listings and read live, one file at a time; history and access requests have no REST
  endpoint and are not offered.

## Amended 2026-11-22 (ux30): the password manager's own web interface

- The page opens the password manager's own web interface from a link template its adapter declares (`web_links`); a
  password item still carries no `web_url`. Pleasant declares folder pages alone: its web client's entry dialog writes
  "Password Fetched" when it opens, so an entry opens its folder.
- A password manager's web session is the site's own. knobas ticks its "remember me" as for any site, so the session
  rests in the site's store, outside the keychain, for as long as the server allows. Pleasant 9.5.4 allows 4 hours.
  *Forget…* removes it. What this ADR names as a secret still never rests.
- The web sign-in writes "Session Log On" under the account (not the API token's "Identity Verified"); a request on a
  kept session writes none. `docs/decisions/2026-11-22-a-password-managers-web-interface.md` has the measurements.

## Amended 2026-11-26 (spec #2514, #2515): the words

The page and its routes are called **Secrets** (`#/secrets`; `#/passwords` addresses redirect), and its two kinds
**Secret entry** and **Secret folder**. What this ADR calls *a secret*, the value that never rests, is a **protected
value** in `CONTEXT.md`, so the word *secret* names the surface and its mirrored entries and never the value. Only the
words changed: every rule above holds for a protected value as it did for a secret, and the role id `passwords`, the
kind ids `password_entry` and `password_folder`, and `Source::read_secret` stay as they are, so no migration follows.

## Consequences

- The knobas-source SPI gains `passwords`, `Source::read_secret`, `Source::passwords_write`, `Sink::otp_key`,
  `KindInfo::title_stays_in_mirror` and `SourceDescriptor::{passwords, by_hand_by_default}`, all additive with
  defaults. The IPC gains the Passwords commands and the `clipboard:state` event.
- Volume is measured against a stand-in (`testenv/pleasant/standin.mjs`, 2,000 entries) held to the real server by a
  contract; its timings are knobas's, not Pleasant's, and are estimates.
