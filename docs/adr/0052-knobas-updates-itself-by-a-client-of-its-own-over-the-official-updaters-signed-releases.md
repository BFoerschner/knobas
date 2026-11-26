---
status: accepted
---

# knobas updates itself by a client of its own, over the official updater's signed releases

Björn asked for updates from inside knobas: a notice when a release lands, a download, a restart, and a channel for
pre-release builds. Decided 2026-11-21 (Fable, on Björn's ask;
`docs/decisions/2026-11-21-knobas-updates-itself.md`): **each release carries the Tauri updater's own signed artifacts
and manifest, and knobas reads, verifies and installs them with a client of its own, which leaves through the ordinary
quit.**

- **What a release carries.** One minisign signature beside every artifact, made by the Tauri CLI's signer with a key
  from the repository's secrets, and one `latest.json` in the official static format. The Mac's artifact is the bundle
  as a `tar.gz`. `release.yml` writes all of it into the draft.
- **What knobas trusts.** Two public keys compiled into it, a primary and a spare. A download is used only if one of
  them verifies its bytes, the signature's trusted comment names the file the manifest names, and that name holds the
  manifest's version. TLS and GitHub are not what is trusted.
- **Where it looks.** *Releases*: `releases/latest/download/latest.json`. *Pre-release builds*: the API's list of
  published releases, then the newest one's own manifest. A draft is visible to neither. No token is sent.
- **Which is newer.** knobas's own order: the numbers, then a release above its pre-releases, then each identifier as a
  word and a number. Never semver's text order, and never the publish time.
- **Nothing happens unasked.** A check sends a user agent. A download and an install each need a press.
- **The install leaves through the quit.** `shutdown_app` stops the sync and PostgreSQL before an installer starts. On
  the Mac and for an AppImage the new copy is exchanged with the old in one rename before the restart, so a failure
  leaves the old app whole.
- **A backup goes first,** and an update that changes the schema does not install without one.
- **No downgrade.** A database a newer build migrated is refused by an older one.
- **The settings and the state are files** beside the profile, outside the database, so the updater works when the
  database does not.

## Considered

- **`tauri-plugin-updater`.** The version without known install faults needs Tauri 2.12 and with it wry 0.57 under
  ADR-0050's views. The version that fits can delete the installed Mac app. Every version downloads into memory and
  exits a Windows app without the quit's handlers. Its format is kept so it can take over after a Tauri bump.
- **The GitHub API and the plain installer, unsigned.** Then whoever can publish a release or sit on the connection
  installs what they like.
- **A manifest per channel at a fixed address.** It must be rewritten at each publish, by a step that can be forgotten.

## Consequences

- A lost signing key ends updates for every installed copy unless the spare exists. Both are Björn's to keep.
- A release published without `latest.json` is never offered. A pre-release not meant for the channel stays a draft.
- Pre-release tags keep the form `vX.Y.Z-<word><number>`.
- The harness that starts release builds sets `KNOBAS_UPDATE=off`.
- If the repository becomes private, updates stop until copies carry a token or the releases are mirrored.
- Until both public keys are committed under `crates/knobas-update/keys/`, a build trusts no key: Settings › Updates
  says updates aren't set up, and nothing is checked or downloaded. `release.yml` signs and writes `latest.json` only
  when the repository has the `TAURI_SIGNING_PRIVATE_KEY` secret; without it a release builds and publishes as before
  and is never offered as an update.
