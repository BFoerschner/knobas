# 2026-11-21: knobas updates itself, from its GitHub releases, with a channel for pre-release builds

**Asked (Björn):** "I want you to design a built in way to run updates. So every time a full release happens on GitHub,
then I want you to be able to do one notify in the settings page and to be able to download and restart it [...] I also
want you to be able to enable [...] beta builds [...] and be able to, like, install the most recent public pre-release.
Yeah, let's call them pre-release builds."

Designed by Fable, built by Opus. The architecture is ADR-0052. This note is the forks and why each went as it did.

## Decided

1. **A client of knobas's own, not the official plugin.** `tauri-plugin-updater` 2.13 and later need Tauri 2.12, which
   moves wry from 0.55 to 0.57 under the site views (ADR-0050). The newest that fits Tauri 2.11 (2.12.0) can delete the
   installed Mac app on a failed install. Every version holds the download in memory and leaves a Windows app through
   `std::process::exit`, past the handler that stops PostgreSQL. The new crate `knobas-update` is about 800 lines.
2. **The releases stay in the official updater's format.** The Tauri CLI's minisign signatures, one `.sig` an artifact,
   and a static `latest.json` in each release. A later move to the plugin changes the client only.
3. **Two keys.** A primary in the repository's secrets and a spare Björn keeps. Both public halves are compiled in. A
   lost or leaked key is then a secret swap and one release.
4. **Two channels.** *Releases* reads `releases/latest/download/latest.json`: no API, no rate limit, never a draft or a
   pre-release. *Pre-release builds* asks the API for the ten newest published releases and takes the newest that
   carries a manifest.
5. **Nothing is written at publish time.** The manifest is uploaded into the draft, and a draft's files answer 404
   without a token. Publishing is what offers the update.
6. **Versions are ordered by knobas's own rule.** Plain semver puts `3.2.9-ux9` above `3.2.9-ux10`. A pre-release tag
   stays `vX.Y.Z-<word><number>`.
7. **A check 60 s after start and every 6 h,** off in a debug build, in the demo profile and under `KNOBAS_UPDATE=off`.
   It sends a user agent and nothing else.
8. **Nothing downloads or installs without a press.** The download is a file in the cache folder, verified as it lands
   and again as it is staged. An unverified file is never run or unpacked.
9. **The restart goes through the ordinary quit.** The sync and PostgreSQL stop, then the installer starts (Windows) or
   the app restarts into the bundle already swapped in (Mac, AppImage).
10. **A backup before each install,** an ordinary archive. A failed backup stops an update that changes the schema.
11. **No downgrade.** An older knobas refuses a newer database.
12. **The settings are a file,** so the check works when the database cannot start, and the start screen offers the
    update then.
13. **The page is Settings › Updates, the notice is a dot on the cog.** One box of two versions whose rooms never
    change; the track between them is the download's progress. No dialog opens by itself.
14. **`.deb` and `.rpm` copies get the notice and a link.** Replacing a system package needs a password knobas should
    not hold.

## Built, and where the build departs from the design

- **No keys yet.** Björn makes the two pairs by hand and hands over the public halves later. Until
  `crates/knobas-update/keys/primary.pub` and `spare.pub` are both committed, `build.rs` compiles no key in (one
  without the other fails the build), the state says `set_up: false`, the page says "Updates aren't set up for this
  build.", and no check or download runs. The design's step 0 had fixture keys standing in; the build ships none.
- **The release run signs only with the secret.** Every update step in `release.yml` and the `manifest` job run only
  when the `draft` job finds `TAURI_SIGNING_PRIVATE_KEY` set (a secret cannot stand in an `if:`, so the job reads it once
  into an output). Without it a release builds and publishes exactly as before. The design had the signing step fail
  by name; the script still does when called without the key.
- **GitHub's reset time.** `knobas-http` reads a failing answer's body and keeps its headers to itself, and it is frozen.
  So after a 403 or 429 from the API the client asks `api.github.com/rate_limit`, which is not counted against the
  limit and carries the same `x-ratelimit-remaining` and `x-ratelimit-reset` headers.
- **Two fields more on the state:** `set_up` (above) and `just_updated` (the toast's one-time trigger after the restart),
  and `Phase::Failed` carries how far a download got (*Stopped at 41 MB*). The channel, the standing, the install kind
  and *Last update* are `knobas-update`'s types, exported into the same `update.ts`.
- **A button's room** is every word it ever says stacked in one grid cell, the others hidden, so its width is the
  longest word's at whatever fonts are set; no pixel width is written down.
- **The start screen's sentence for an older build refusing a newer database** (decisions D10 point 5) is not built:
  the ux28 database-link fix owns the start screen's error text while it works. It is a mapping of
  `MigrateError::VersionMissing` in `bring_up_app` and is owed after that fix lands.

## The way back from an update

Install the old version, start it on an empty profile, restore the archive from before the update (Settings › Backup
and export; *Last update* on Settings › Updates names it), and let the sources sync again.

## Owed

On Windows: the NSIS and MSI installs on a runner (`update-e2e.yml`, not written yet: it can first run once two builds
with the updater exist), and SmartScreen on Björn's own first update. On Linux: an AppImage update on the WSL box. The
first release with keys: a `platforms=linux` run first, the cheapest leg, and the signing step's time on the Windows
runner.

## Reversing it

Take `updates` out of `SETTINGS_TREE`, the mark out of the top bar, and the `update_*` commands out of the handler
list. The releases keep their `.sig` files and `latest.json`, which harm nothing unread.
