# The signed dev build and the desktop witness (macOS)

This page covers three things that need a Mac with a screen: building a debug bundle signed with the self-signed `knobas-dev` identity, the desktop witness (`just desktop-witness <driver>`, a scripted run that drives that bundle through the accessibility API), and `just live-shot`, which photographs the real app while a steps file drives it. Read it when you check a desktop notification by hand, run or write a witness driver, or need a screenshot of the app against a live product. The witness and live-shot both take over the screen, and only one of them runs on a machine at a time.

Commands on this page run from the repository root unless a block says otherwise. The rest of the runbook runs in `testenv/`, this page does not. `just` recipes work from anywhere in the checkout. An agent's shell starts at the repository root, where a hook refuses `cd <dir> && ...`, so an agent uses `env -C <dir> <command>` where a block names another directory. File paths in backticks are relative to the repository root.

## Signed dev build (macOS)

Desktop notifications (#290) need a `.app` bundle with a stable signed identity, and an unsigned build asks for the keychain again on every run (roadmap gotcha 10). `tauri dev` produces neither a bundle nor a signature, so the check below runs against a signed bundle.

No Apple identity is needed. A self-signed code-signing certificate is a stable identity as far as TCC is concerned. It is not one for the keychain: with no team ID, macOS partitions a keychain item by the writing binary's cdhash, which changes with every build. A debug build therefore keeps the demo profile's credentials in a file (#2384, *Live products as sources* in [`testenv/README.md`](../README.md)).

### Create the `knobas-dev` identity

The identity on the dev Mac is `knobas-dev`, a self-signed certificate created 2026-09-03 (#273). Check that it is there:

```sh
security find-identity -v -p codesigning
```

If the list does not name `knobas-dev`, create it again. Run this in a scratch directory, because it writes four files:

```sh
cat > knobas-dev.cnf <<'CNF'
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = knobas-dev
[ext]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
subjectKeyIdentifier = hash
CNF
openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -config knobas-dev.cnf \
  -keyout knobas-dev.key -out knobas-dev.pem
openssl pkcs12 -export -inkey knobas-dev.key -in knobas-dev.pem -name knobas-dev \
  -passout pass:knobas -out knobas-dev.p12
security import knobas-dev.p12 -k ~/Library/Keychains/login.keychain-db -P knobas \
  -T /usr/bin/codesign -T /usr/bin/security
security add-trusted-cert -r trustRoot -p codeSign \
  -k ~/Library/Keychains/login.keychain-db knobas-dev.pem
```

`openssl pkcs12 -export` may need `-legacy` on an OpenSSL 3 that refuses the default cipher. An Apple Development certificate from Xcode works the same way. Export its full quoted name as the identity instead.

### Build, sign and launch the bundle

Build from the crate that owns `tauri.conf.json`, as the `dev` recipe does. `env -u RUSTUP_TOOLCHAIN` is there for the reason at the top of the justfile, and `--bundles app` skips the dmg.

```sh
export APPLE_SIGNING_IDENTITY="knobas-dev"
cd crates/knobas-app
env -u RUSTUP_TOOLCHAIN PATH="$PWD/../../app/node_modules/.bin:$PATH" \
  tauri build --debug --bundles app
```

With `env -C crates/knobas-app` in place of the `cd`, `$PWD` is still the repository root, so the `PATH` entry is `$PWD/app/node_modules/.bin`.

Back at the repository root, verify the signature and launch the bundle:

```sh
codesign -dv --verbose=2 target/debug/bundle/macos/knobas.app   # Authority=knobas-dev
codesign --verify --deep --strict target/debug/bundle/macos/knobas.app
open --stdout /tmp/knobas.out --stderr /tmp/knobas.err target/debug/bundle/macos/knobas.app
```

- `TeamIdentifier=not set` is expected for a self-signed certificate. So is the notarization warning.
- Launch the bundle, not the bare binary, and launch it from where it was built, not from a copy. A click on a desktop notification activates the bundle Launch Services has registered for `dev.knobas.desktop`. A copy launched from elsewhere keeps running while the click starts a second instance from the registered path.
- `--stdout` and `--stderr` are optional. A bundle started by `open` or Finder has no terminal, so those two files are where the `tracing` lines land.

### Check a desktop notification and its click

1. The banner (#290). In that bundle, switch one category on, take the focus off the window, and see one desktop notification in Notification Center.
2. The click (#339). Click the banner's body. There is no button. knobas comes to the front and opens the item. `addressOf` in `app/src/lib/inbox/address.ts` decides where: a credential expiry opens its source's page in Settings, and an item with nothing to open opens `#/inbox`.

stderr tells you what happened:

| Line | Level | Meaning |
| --- | --- | --- |
| `notification shown, waiting on its click`, with the `address` | `info` | macOS took the desktop notification |
| `notification clicked` | `info` | the click came back and knobas navigated |
| `notification closed without a click` | `debug` | somebody cleared it from Notification Center. Set `RUST_LOG=knobas_app=debug` to see it |
| `notification not shown` | `warn` | the send failed, see troubleshooting |

knobas sends through `notify-rust` on its `UNUserNotificationCenter` backend (`crates/knobas-app/src/notify.rs`, Björn's ruling on the #339 prototype). It waits on the handle off the main thread and emits `notification:clicked` with the item's address. `tauri-plugin-notification` stays for `is_permission_granted` and `request_permission` only, on the click that switches a category on. knobas asks macOS for nothing at send time.

### What this check does not cover

- Linux and Windows. The freedesktop `default` action and the Windows handle API are written from `notify-rust` 4.18.0's sources and nobody has clicked a banner on either. `notify.rs`'s and `app/src/lib/inbox/notify.svelte.ts`'s module headers say the same, and so does `docs/archive/contract-10.8/339-ipc-command-schema-rust-handler-barrel.md`.
- A first run on a bundle identifier macOS has never authorized. The dev Mac has had `dev.knobas.desktop` authorized since 2026-09-03 and there is no cheap way to undo that, so every run since #339 has been a later run. What a first run looked like on 2026-09-03 is the last troubleshooting row below. That observation predates the `UNUserNotificationCenter` backend.

### Troubleshooting a signed build

| Symptom | Cause | Fix |
| --- | --- | --- |
| A click on a banner starts a second knobas | The running copy is not the one Launch Services has registered for `dev.knobas.desktop`. | Launch `target/debug/bundle/macos/knobas.app` where `tauri build` wrote it. If another copy holds the registration, see [prerequisite 4](#prerequisites) of the witness. |
| No desktop notification under `tauri dev` (`just dev`, `just demo`) | Expected on macOS. The backend refuses a bare binary with *No bundle identifier found. UNUserNotificationCenter requires a valid .app bundle.* The `notify` command rejects with that sentence, the store swallows it, and the dev terminal shows `notification not shown` at `warn`. | Use the signed bundle. The category switches still work in dev. |
| No banner from the bundle, and stderr says `notification shown` | macOS holds it back: the app is not authorized to notify, or a Focus is on. | Read the unified log (below), then allow the app in System Settings. |

The unified log tells the states apart:

```sh
/usr/bin/log show --last 5m --info --predicate 'process == "usernoted"' --style compact \
  | grep -E 'knobas.desktop|askpermissions'
```

Call `/usr/bin/log` by path, because zsh has a builtin named `log` that answers "too many arguments". `--info` is needed because the lines below are info level and the default filter hides them.

- `Delivering <NotificationRecord app:"dev.knobas.desktop" ...> to [ .alert .lockScreen .notificationCenter ]` with no matching `Presenting` line: knobas posted it and macOS held it back. Not authorized.
- `Presenting <NotificationRecord app:"dev.knobas.desktop" ...> as banner`: the check passed.
- `Event was resolved: ... outcome: allowed; reason: disabled` from `donotdisturbd`: no Focus is swallowing anything. Any `outcome` other than `allowed` is a Focus, not a knobas bug.

To authorize the app: System Settings > Notifications > Application Notifications > knobas > *Allow notifications*. The app is listed there once it has posted one desktop notification. It never appears in `~/Library/Preferences/com.apple.ncprefs.plist` while it is refused, so its absence from that file does not show that the code never ran.

What was seen on 2026-09-03 (#290), on the older `NSUserNotification` send path: on the app's first contact macOS raised its own prompt (*"knobas Notifications: Notifications may include alerts, sounds, and icon badges"*), withdrew it unanswered when the app quit, and did not ask again. Until somebody allowed the app in System Settings, every desktop notification was delivered and never presented.

## The desktop witness (macOS)

Some features belong to the operating system more than to the app: open-in-editor, open-in-terminal, and the keystrokes macOS delivers (⌘K and the global capture shortcut). They have no instance to run a suite against. [ADR-0016](../../docs/adr/0016-a-spawned-command-takes-no-argument-from-the-mirror.md) makes their witness a scripted run against a real bundle, not a checklist somebody ticks. This is that harness (#500): [`testenv/desktop-witness.sh`](../desktop-witness.sh), the helper [`ax.swift`](ax.swift) and the drivers in [`drivers/`](drivers/).

Three rules bind it:

- Scope is OS-level features only (the v1.5 grilling's ruling). A rendered page is witnessed by headless Chrome against the `?fake-ipc` dev server (the deputy's ruling of 2026-09-08 on #496), not here. The harness takes the screen and runs one at a time, so page QA on it would queue everything behind it.
- `just desktop-witness` is not part of `just check` and must not become part of it. It takes the screen for minutes and needs a signing identity and a TCC grant a fresh checkout does not have. The gate carries [`just witness-unit`](#what-the-gate-runs-just-witness-unit) instead.
- One witness or live-shot runs on a machine at a time. The harness takes `$TMPDIR/knobas-desktop-witness.lock` with `mkdir` and refuses while another run holds it. Two runs would fight over one screen, one bundle identifier and one Launch Services registration.

### Prerequisites

A person meets these once. The harness checks them on every run, names each refusal by the name System Settings uses, and exits non-zero.

1. **The `knobas-dev` code-signing identity**, from [Create the `knobas-dev` identity](#create-the-knobas-dev-identity).

2. **Accessibility, granted once to the terminal that runs the witness.** System Settings > Privacy & Security > Accessibility. Add the terminal application (`com.mitchellh.ghostty` on the dev Mac), not a script. A shell started before the grant does not see it, so start a new one.

   It is Accessibility and never Automation. `osascript` against `System Events` is an Apple Event, which TCC gates under Automation with a prompt per target. Nobody is at the machine when the ticket loop runs, and TCC records an unanswered prompt as a denial (`auth_reason 9`) that sticks. Both Automation rows on the dev Mac (`com.apple.systemevents` and `dev.knobas.desktop`) have been denied that way since 2026-09-08. So a driver never calls `osascript`. `ax.swift` uses the accessibility API and `CGEvent`: one grant, no prompt at run time. The harness reads `AXIsProcessTrustedWithOptions(nil)` and `CGPreflightPostEventAccess()`, and neither prompts.

3. **An unlocked screen.** No synthetic keystroke reaches an application behind the lock screen. The window server also answers a query for an app's windows with the application element in place of the window, so a driver behind a lock reads an empty tree and blames the app. The harness reads `CGSSessionScreenIsLocked` twice, before the build and after it, because a build takes minutes and a screen can lock meanwhile.

4. **Launch Services resolves `dev.knobas.desktop` to the bundle this run builds.** The harness launches the copy Launch Services names, for the reason in the signed build section: a click on a desktop notification activates that copy, whichever it is. Which of several copies with one identifier Launch Services prefers is not a documented API. With `/Applications/knobas.app` installed, `lsregister -f` on a new debug bundle did not move the registration (2026-09-08). So the harness refuses when the answer is not the bundle it built.

   By default the harness moves nothing. ADR-0016 says the harness never moves an installed knobas aside, and that this is the runner's job. Whoever runs the witness, a merge-manager included, clears the registration first. Expect several paths: the dev Mac had ten on 2026-09-08 (`/Applications`, old worktrees, the root checkout's debug and release bundles, stale DMG mount points), and moving `/Applications/knobas.app` aside was not enough. The harness registers what it builds and never unregisters it, so every worktree that ran a witness leaves a path behind.

   By hand, unregister every other path and put back afterwards the ones still on disk. This changes only the Launch Services database. Nothing is moved or deleted.

   ```sh
   LSREG=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
   "$LSREG" -dump | sed -n 's/^ *path: *\(.*knobas\.app\) (0x[0-9a-f]*)$/\1/p' | sort -u
   # unregister every path but the bundle this worktree is about to build:
   "$LSREG" -u /Applications/knobas.app          # and each of the others
   just desktop-witness <driver>
   "$LSREG" -f /Applications/knobas.app          # and each one still on disk
   ```

   Or let the harness do it for this repo's own checkouts (#1176):

   ```sh
   KNOBAS_WITNESS_UNREGISTER_OTHERS=1 just desktop-witness <driver>
   ```

   After the build, the harness reads `lsregister -dump` and unregisters every other copy of `dev.knobas.desktop` that lives inside the root checkout or its `.worktrees/`. On exit (success, failure, Ctrl-C or SIGTERM) it registers again each one still on disk. It never touches a copy anywhere else, such as `/Applications/knobas.app`, a DMG mount or a scratch directory. The run lists such a copy as *left registered*, and the refusal still names it if it wins. `copies_to_unregister` in `testenv/desktop-witness-lib.sh` makes the selection, and `witness-unit` tests it over a fake dump.

5. **`swiftc`**, from the Command Line Tools. Each run compiles `ax.swift` into a scratch directory (about 2 s). No binary is committed. The harness also needs `codesign`, `security`, `open` and `pgrep` on `PATH`.

6. **No knobas running.** The harness counts processes named `knobas-app` and refuses if one is up, because it asserts that exactly one instance runs and it takes the screen from whatever is on it.

A driver can need more. `pull-request-open-in` needs the testenv Gitea, see [The four drivers](#the-four-drivers).

### Run a driver

```sh
just desktop-witness launcher-hotkey    # with no driver name, it lists the drivers
```

### What a run does

1. Takes the lock and compiles the accessibility helper.
2. Checks that this session can drive the desktop (`ax probe`: Accessibility, the right to post events, an unlocked screen).
3. Checks the signing identity and that no knobas is running.
4. Builds a debug bundle and signs it with `knobas-dev`, then verifies that `codesign` reports `Authority=knobas-dev`.
5. With `KNOBAS_WITNESS_UNREGISTER_OTHERS=1`, unregisters the repo's other copies.
6. Registers the bundle with Launch Services, asks what `dev.knobas.desktop` resolves to, and refuses if that is another copy.
7. Checks the desktop again, since the build took minutes.
8. Launches the registered path on the demo profile (`open -a <bundle> --args --demo`), waits for the process, then up to 180 s each for a window and for the web view to draw. The 180 s covers a first-run PostgreSQL provision.
9. Asserts that exactly one instance is running.
10. Gives the window a known size, see below.
11. Runs the driver.
12. Quits the app with ⌘Q, so the app stops its embedded PostgreSQL on the way out. If knobas does not come forward, it sends no ⌘Q, because the keystroke would quit whatever is in front. If the app is still up after 20 s, it sends SIGTERM, and SIGKILL after the guard.
13. On every exit it stops the PostgreSQL a killed app left on the demo profile (`stop_left_postgres`, #1967), registers again whatever Launch Services named before the run, and removes the scratch directory and the lock. A PostgreSQL that was already serving the demo profile before the launch is left alone.

**The window size** (#2056). A tiling window manager gives a new window whatever share of the screen is left, and at 1240 px and below Tickets hides the preview that `capture` opens its job from. So `witness_window_size` (`testenv/desktop-witness-lib.sh`) runs between the first draw and the driver, and `just live-shot` runs it before its steps file. When AeroSpace is running (its CLI answers), it puts the window in fullscreen by its window id, whatever its width, and the fullscreen ends with the window. Elsewhere it widens a window narrower than `WITNESS_MIN_WIDTH` through `ax resize`. `WITNESS_MIN_WIDTH` is 1241 points, and `witness-unit` pins it against the breakpoint in `app/src/lib/tf/css/tickets.css`. The run log prints the size the driver runs at. Below the minimum, the run names the size and the minimum and fails before the driver's first step.

### Environment variables

| Variable | Default | What it does |
| --- | --- | --- |
| `KNOBAS_WITNESS_WAIT_S` | `60` | The guard, in whole seconds, on the wait for the app's process after `open`, on `ax activate`, and on every wait a driver or live-shot makes on the app (#1961). A loaded machine draws slowly, so raise it there. An app that has exited fails a wait at once, so a large guard costs a real fault nothing. Anything but a whole number is refused with exit 2. The 180 s for the window and first draw is fixed. |
| `KNOBAS_WITNESS_UNREGISTER_OTHERS` | unset | `1` unregisters this repo's other copies of the bundle for the length of the run, see prerequisite 4. |
| `KNOBAS_WITNESS_GITEA` | `https://127.0.0.1:3443` | `pull-request-open-in` only: another https front for the testenv Gitea. It must be an https address. |

The harness passes three more to the driver: `KNOBAS_WITNESS_AX` (the compiled helper), `KNOBAS_WITNESS_PID` (the one running instance) and `KNOBAS_WITNESS_APP` (the bundle it launched).

### The four drivers

| Driver | Issue | Witnesses | Needs beyond the prerequisites | Leaves on the demo profile |
| --- | --- | --- | --- | --- |
| `launcher-hotkey` | #500 | ⌘K puts the focus in the launcher's box, Escape removes the box | nothing | nothing |
| `open-in-editor` | #501, ported by #1084 | an application the reader added runs with the checkout path as its only argument | nothing | the clones root it set, one application per run |
| `capture` | #503, ported by #1084 | the global shortcut opens the capture window over another application, and the typed text becomes a note | Finder running | notes shown, the shortcut stored, one note per run |
| `pull-request-open-in` | #2316 | a pull request's *Open in…*: worktree, *Diff tool*, *Browser*, and *Clone* on a repository | the testenv Gitea over https, `curl`, `jq`, `git` | the clones root it set, one application per run, Settings' record of a worktree whose folder the run deleted |

The demo profile is shared with the person's own `--demo` app, so what a driver leaves stays there.

**`launcher-hotkey`** first asserts that the launcher's box does not have the focus. It presses ⌘K and asserts through the accessibility tree that the focused element is an `AXComboBox` or `AXTextField` named *Search or act*, the `aria-label` in `app/src/lib/launcher/QueryBox.svelte`, and that exactly one element carries that name. It presses Escape and asserts that the box is gone from the tree while the window is still there. `Launcher.svelte` renders the overlay under `{#if open}`, so a closed launcher takes its input out of the DOM. It says nothing about what the launcher shows.

**`open-in-editor`** witnesses the spawn.

1. It writes a stub executable and a fake clone into a scratch directory. The clone is a directory whose `.git/config` names the demo repository as `origin`. No git command runs.
2. In Settings › *Checkouts and Open in* it sets the clones root to the scratch directory (*Change…*, *Save*) and adds an application whose command is `<stub> {path}` (*Edit applications…*, *Add an application…*).
3. It opens the repository `payout-service` through the launcher. It reads the screen before Return and after it, and requires the page's *Cloned at* line to name the clone only after.
4. It picks the new application from *Open in…*. The menu opens with the default focused, and three steps up from it is the last application, the one this run added.
5. It asserts that the stub ran with exactly one argument and that it is the checkout path. That is the assertion ADR-0016 asks for: what the program received is what the disk answered, and nothing the mirror holds.

It opens the repository's page and not the job's Code section, which draws the same `TfOpenIn`. Measured 2026-10-18: the demo's PAY-231 reaches its branch only through a proposed link, so the job's Code section reads *Nothing connected* until a reader accepts it.

**`capture`** witnesses the one keystroke that must arrive while knobas is not the application being typed at.

1. In Settings › *Notes and capture* it turns notes on if they are hidden, because the shortcut registers nothing while they are. It clears a stored shortcut, types `CmdOrCtrl+Alt+Shift+K` into the *Change…* dialog (#1086), waits for the row to read *Opens a small window over whatever you are doing.*, and counts the notes.
2. It opens the job PAY-231 from Tickets (filter by key, then the preview's job link), so the capture has a foreground. It goes through Tickets and not ⌘K: on 2026-10-18 the demo profile's launcher answered `PAY-231` with pages, commits and a branch, and `#PAY-231` with nothing.
3. It brings Finder to the front and asserts that Finder is frontmost.
4. It counts knobas' windows titled `Capture`, sends the combination, and counts again: none, then exactly one (#548). It asserts that the caret is in the capture window's box.
5. It types two lines, waits for *Saved as a note.*, and presses *Open in knobas*.
6. It asserts that knobas is frontmost again, that no window is titled `Capture`, and that the note's own page is on screen with its title and its *Captured from* row (#1504).
7. It opens Settings › *Notes and capture* again and asserts one note more than it counted.

**`pull-request-open-in`** runs against the testenv Gitea, because the demo's mock cannot carry it: the mock draws no source view, its repositories have no owner, and its shas are short (ruling on #2316). It needs:

- `just live-up gitea` from the same checkout, for the token `./seed --env` prints. It also starts the https front.
- The https front's root trusted once with `just live-trust-ca`. The driver reaches Gitea at `https://127.0.0.1:3443` on either target. See [The https front](../docs/live-products.md#the-https-front).

What it does:

1. It clones `payout-service` into a scratch clones root and names a stub as the clone's `diff.guitool`.
2. It adds the Gitea in Settings › *Sources* (*Add a source…*, *Test connection*, *Add source*), sets the clones root, and adds an application with a pull-request command.
3. It asks ⌘K for the pull request `SEPA retry with exponential backoff` until the new source has synced it, and opens it.
4. The worktree (#2285, [ADR-0035](../../docs/adr/0035-a-pull-request-is-opened-in-a-worktree-under-the-clones-root.md)): the application ran with the worktree as its only argument, the worktree is at the pull request's head, its own `FETCH_HEAD` records `refs/pull/<n>/head` from the https address, and the clone's branch, `HEAD` and status are unchanged.
5. *Diff tool* (#2275): the stub's right-hand folder holds every file the pull request changes.
6. *Browser* (#2274): the menu item names the pull request's page, and the default browser comes to the front.
7. *Clone* ([ADR-0023](../../docs/adr/0023-knobas-clones-a-repository-into-the-clones-root.md), #2377): on the page of `ledger-api`, which the scratch clones root lacks, *Clone* leaves a clone at the remote's head whose origin is the https address.
8. It removes the source together with what it synced. A run that fails before this step names the source to remove in Settings › *Sources*.

### Write or change a driver

A driver is an executable `drivers/<name>.sh`. It changes into `testenv/`, sources `desktop-witness-lib.sh`, and reads `KNOBAS_WITNESS_AX` and `KNOBAS_WITNESS_PID`. Copy the helpers the existing drivers share (`wait_until`, `count_of`, `exactly_one`, `press_when`, `fill`, `screen`, `reads`, `die`). The rules below are what the first runs cost. Each is in the scripts today.

#### What the accessibility tree answers

A Tauri window's `WKWebView` exposes its accessibility tree, with roles and `AXDOMIdentifier`s. Text reaches a driver in one of three ways, and a driver must ask the right question.

| Kind | Arrives as | What carries it | Read it with |
| --- | --- | --- | --- |
| A name | `AXDescription` or `AXTitle` | an `aria-label`, a button's own words, a field named by `<label for=...>` | `ax find` counts, `ax press` and `ax focus` act |
| A value | `AXValue` | a heading, a status line, a rendered paragraph, a field's contents | `ax values` prints one per line. Match whole lines with `has_reading` |
| A window | the `AXWindow`'s title | the title the window was built with | `ax windows` prints one line per window. Count with `window_count` |

- An `aria-label` arrives on both `AXDescription` and `AXTitle`. Which one WebKit uses is not a promise, so `ax find` matches either, and a driver that reads `ax focused` accepts either.
- A `<label for=...>` names its field on `AXTitle`, and the label element is not a second match. `ax find` answers 1.
- A name is matched whole. Where a label carries more words than the field's name (*Address* with its *required*), the field needs its own `aria-label`.
- A heading is a value, not a name. Waiting for one with `ax find` counts 0 with the heading on screen (#547).
- "Is that window open?" is a question about windows. The capture window carries the word `Capture` three times: the window's title (`crates/knobas-app/src/capture.rs`), the web area's description (`app/capture.html`'s `<title>`) and the text area's `aria-label` (`CaptureWindow.svelte`). All three are correct, and `ax find` answers 3 (#548). Count windows.
- A name is the rendered text. WebKit names an element by what is drawn, so a CSS `text-transform` on a label changes the name a driver must spell. An `aria-label` is not rendered text and is not transformed. Today no stylesheet under `app/src` sets `text-transform`, so every driver spells a name as the markup does. `witness-unit` pins that `app/src/lib/tf/tf.css` contains no `text-transform`. That file only lists the `@import`s, so the pin does not read the sheets under `app/src/lib/tf/css/`.
- `ax press` and `ax focus` act on exactly one match. Two matches fail like none, because the tree's order promises nothing. Settings opens on the section it last showed, and on Sources the word *Sources* is the crumb, the nav item and the heading at once.
- When several elements rightly carry a field's name, find the field by the focus, by role and name. In the shortcut dialog, the dialog, its heading and the field all answer to *Capture shortcut*, and `TfDialog` puts the caret in its first field (measured 2026-10-18).
- While an `aria-modal` dialog is up, the tree is the dialog and nothing behind it. Read a row behind a dialog after the dialog has closed (measured 2026-10-18).

#### Waits that witness something

- Wait for what you are about to act on. The harness waits for a window and a first draw, but the shell finishes booting after that, and Return on a launcher that has not answered selects nothing. `press_when` and `fill` wait for exactly one match first.
- A wait that was already true one keystroke earlier witnesses nothing. Read before the step and after it, and pass both to `waypoint_verdict`. It answers `witnessed`, `too-early`, `never-appeared` or `unreadable`. Use `reading_verdict` for a line and `count_verdict` for a count.
- An answer that could not be read is never "absent" and never zero. `ax values` prints nothing and exits 0 when it sees no window (a locked screen, a quit app). `ax windows` refuses when there is none. So `count_of`, `window_count` and `notes_total` answer `x`, and the verdicts answer `unreadable`. Otherwise an app that died mid-run would pass "the capture window has gone".
- Wait on a landing page only through `lands <landing> "$READING"...` or `lands_named <landing> "$NAME"...`, with the driver's own `readonly` constants as arguments (#1968). `app/src/App.test.svelte.ts` replays each landing in the DOM (`app/src/lib/shell/test-witness.ts` reads the call out of the driver), so an app change that moves where a step lands turns the gate red. `witness-unit` refuses a driver with no landing, a driver that calls `lands` without its own `wait_until`, and a landing with no replay.
- Every accessible name a driver acts on has a `pin_label` line in `testenv/desktop-witness-test.sh` against the file that carries it. Add one for each new name. A pin shows that the string is still in the file. It does not show which attribute the accessibility API answers with, how many elements answer, or where a step lands.
- To witness a spawn, point the command at `stub_script`'s stub, wait with `wait_for_record`, and read the answer with `sole_argument`. The stub writes one argument per line and renames the record into place, so a record is whole or absent. `sole_argument` answers nothing for no argument and for two.
- Name what a run adds after its scratch directory, as `Witness <suffix>` does, so an earlier run's entry never answers for this one. Say in the driver's header what it leaves on the demo profile.

#### Typing, keys and the front

- `ax key` and `ax type` post through the HID tap to the frontmost application, as a person's keyboard does. Bring knobas forward with `ax activate` first.
- A driver may not type a quote or a double hyphen. `ax type` posts characters the way a keyboard does, and macOS rewrites some on their way into a WebKit field: Smart Quotes turns `"` into curly quotes, and the same preference pane turns a double hyphen into a dash. A command template with a quoted path reached the database with curly quotes and knobas rightly refused to run it (#547). Put a path into a template as one unquoted word, and check it with `path_is_one_word` before typing.
- `ax type` clears the modifier flags on every event, so text typed after `ax key <code> command` arrives as text.
- `ax type` sends unicode strings, not key codes, so it does not depend on the keyboard layout. `ax key` takes a virtual key code, which is a physical position.
- macOS refuses to hand the front to an application that a background process merely activates. To put another application in front, `open -a` it and then `ax activate`. `open -a Finder` on a Finder with no window makes one, so `capture` closes that window again with `ax close-window`. To bring knobas back while another application holds the front, focus its window through AeroSpace and `open` the running bundle (`KNOBAS_WITNESS_APP`), then `ax activate`, as `to_front` in `pull-request-open-in.sh` does.
- Count processes with `pgrep -x knobas-app`, never `pgrep -f` on the bundle path. `-f` matches any process whose command line mentions the path. `knobas-app` is the cargo binary name, not the bundle's `productName`, and `witness-unit` pins it against `crates/knobas-app/Cargo.toml`.
- On failure, print what the next person needs: `ax focused`, `ax values` and `ax dump <pid> <depth>`, as every driver's `die` does.

#### The helper's subcommands

Every answer is tab-separated lines, read with `tsv_field`.

| Subcommand | Answers |
| --- | --- |
| `ax probe` | `trusted`, `post-events`, `screen-locked`, each 0 or 1 |
| `ax registered-path <bundle-id>` | the path Launch Services resolves the identifier to |
| `ax wait-window <pid> <seconds>` | the first window's title, once there is one |
| `ax wait-content <pid> <seconds>` | returns once the web view holds anything |
| `ax activate <pid>` | brings the app forward and waits for it, under `KNOBAS_WITNESS_WAIT_S` |
| `ax frontmost` | the frontmost application's `pid` and `bundle` |
| `ax default-browser` | the `bundle` that opens an https address |
| `ax focused <pid>` | the focused element: `role`, `subrole`, `description`, `title`, `placeholder`, `dom-id` |
| `ax find <pid> <label>` | how many elements carry the name |
| `ax values <pid>` | every string value on the app's windows, one per line |
| `ax windows <pid>` | one `window` line per window with its title. Refuses when there is none |
| `ax dump <pid> <depth>` | the tree, for a failure message |
| `ax focus <pid> <label>`, `ax press <pid> <label>` | act on the one element carrying the name |
| `ax type <text>` | types into whatever has the keyboard |
| `ax key <keycode> [command\|shift\|option\|control ...]` | one key to the frontmost application |
| `ax close-window <pid>` | closes the app's window when it has exactly one |
| `ax window-id <pid>` | the largest on-screen window: id, width, height in points |
| `ax resize <pid> <width> <height>` | sets the largest window's size |
| `ax press-visible`, `ax focus-visible`, `ax click-at <pid> <label> [n]` | live-shot's: the n-th visible match, see below |

### What the gate runs (`just witness-unit`)

`just witness-unit` is part of `just check`. It runs `testenv/desktop-witness-test.sh` and then `scripts/bash-guard-test.sh`. No gate runs a driver, so the test covers every part of the harness that is a decision and not a side effect:

- how the harness compares Launch Services' answer with the bundle it built, and which copies the opt-in step unregisters
- how it reads `ax probe`, and that each refusal names its permission
- the guard and its polling, and the stop of a PostgreSQL a killed app left
- the pure parts of the drivers: the stub and its record, the path that may go into a command unquoted, the `.git/config` the fake clone gets, a note's title, the notes count, whole-line readings, the waypoint and count verdicts, a pull request's worktree path and its `FETCH_HEAD` line
- every pinned name, the process name, and every driver's landings and their DOM replays
- the window-size step against a stub `ax` and a stub `aerospace`
- `key_code` and live-shot's refusals, which run dry and stop once its lock is held

The checks that compile `ax.swift` and ask the real helper skip themselves where macOS and `swiftc` are not both present.

### What is not witnessed

- How WebKit's accessibility tree reads a DOM. The replay in `App.test.svelte.ts` asserts the DOM, and only an on-screen run can say what the tree answers.
- What the launcher shows after ⌘K. That belongs to the frontend suite and the `?fake-ipc` walk (the deputy's ruling of 2026-09-08 on #496).
- A capture's `captured-in` link, and which entity the *Captured from* row names. `capture` asserts only that the row is there. The links' witness is `crates/knobas-app/tests/it/capture_ipc.rs` and `app/src/lib/capture/capture.test.ts`, which is where spec #491's stream map puts it. A driver step for `captured-in` is welcome and owed to nothing (the deputy's ruling of 2026-09-08 on #503, part 5).
- A shortcut the operating system refuses. That needs another application holding the combination, which no driver can arrange. `capture_ipc.rs` pins the refusal at the IPC seam with a stand-in registrar.
- Escape closing the capture window. `app/src/lib/capture/CaptureWindow.test.svelte.ts` covers it.
- Which copy Launch Services prefers among several. The harness asks and refuses, it does not predict.

### Troubleshooting a witness run

Each refusal starts `desktop-witness: FAILED --`. A driver's failure starts with the driver's name and is followed by the focused element, the strings on screen and the tree.

| Symptom | Cause | Fix |
| --- | --- | --- |
| `another desktop witness holds .../knobas-desktop-witness.lock` | Another witness or live-shot is running, or a killed run left the lock. | Wait. If nothing is running, `rmdir "$TMPDIR/knobas-desktop-witness.lock"`. |
| `the terminal running this witness is not trusted for Accessibility` | No grant, or the shell predates it. | Prerequisite 2, then start a new shell. |
| `this process may not post keyboard events` | Accessibility covers both, but a separate Input Monitoring entry can also refuse. | Check both panes under Privacy & Security. |
| `the screen is locked` | Prerequisite 3. | Unlock the Mac and run again. |
| `the accessibility probe printed nothing this harness understands` | The helper failed or its output was cut. | Run `ax probe` by hand. The run prints the compiled helper's path. |
| `no code-signing identity called 'knobas-dev'` | Prerequisite 1. | Create the identity. |
| `knobas is already running (pid ...)` | Prerequisite 6. | Quit it. |
| `Launch Services still resolves dev.knobas.desktop to another copy` | Prerequisite 4. The message names the winner. | Unregister the other copies by hand, or set `KNOBAS_WITNESS_UNREGISTER_OTHERS=1` when they are this repo's. |
| `nothing started: no knobas-app process appeared within N s` | A loaded machine took longer than the guard to start the binary. | Raise `KNOBAS_WITNESS_WAIT_S`. |
| `N knobas processes are running, not one` | A second copy started from a path this run did not launch. | Prerequisite 4. |
| `the window is WxH points, narrower than the 1241 the drivers need` | The window manager left the window too narrow and it could not be widened. | Give it an empty workspace or a wider screen. |
| A driver step ends `(waited 60 s, KNOBAS_WITNESS_WAIT_S)` | The screen never showed what the step waits for. The dump below the message shows what it did show. | Read the dump. On a loaded machine, raise `KNOBAS_WITNESS_WAIT_S`. |
| `KNOBAS_WITNESS_WAIT_S must be a whole number of seconds` (exit 2) | The value is not whole seconds. | Use a number such as `120`. |
| `knobas would not come forward, so no ⌘Q` | Another application kept the front. | Nothing to do: the run sends SIGTERM and stops the PostgreSQL the app left. |
| `the PostgreSQL the app left is still running on ...` or `a PostgreSQL is still starting on ...` | The run could not stop the demo profile's server within the guard. | Run the `pg_ctl` command the message prints. |
| `pull-request-open-in`: `the testenv Gitea does not answer at ... with a certificate this Mac trusts` | Gitea or the https front is down, or its root is not trusted. | `just live-up gitea`, then `just live-trust-ca` once. |
| `pull-request-open-in`: `the demo profile keeps the source witness-...` | The run failed after adding the source. | Remove that source in Settings › *Sources*. |

## Screenshots of the real app (`just live-shot`, #1483)

`just shot` photographs the frontend under `?fake-ipc`. It cannot show the app against a real product. `just live-shot` starts `just demo` with `KNOBAS_SECRET_STORE=memory`, waits for the window, and runs a steps file that drives the app and photographs the knobas window only.

```sh
just live-shot <out dir> <steps file>
```

It is not part of any gate. It is also not how to show Björn the app: that is `just live-demo` (Björn, 2026-11-02, `CLAUDE.md`).

### Before you run it

- macOS, with `swiftc`, `screencapture`, `sips` and `pgrep` on `PATH`.
- The witness's Accessibility grant and an unlocked screen ([prerequisites 2 and 3](#prerequisites)). It compiles the witness's `ax.swift` per run, as the witness does.
- No knobas running, and port 1420 free. Another `just demo` or `just dev` holds that port.
- The witness's lock free. live-shot takes `$TMPDIR/knobas-desktop-witness.lock`, so one witness or live-shot runs at a time.
- No signing identity. It runs `just demo`, not a bundle.

### Write a steps file

The steps file is bash, sourced once the window is up. [`testenv/live-shot.sh`](../live-shot.sh)'s header documents each command.

| Command | What it does |
| --- | --- |
| `shot <name>` | Writes `<out dir>/<name>.png`: the knobas window only (`screencapture -l` of its window id), scaled from Retina to the window's size in points. |
| `press <label> [n]` | Presses the visible element with that label. |
| `focus <label> [n]` | Gives it the keyboard. |
| `click-at <label> [n]` | A pointer click at its centre, for a control only a hover reveals. It also takes words on screen, such as a radio's label or a list row's name. |
| `type <text>` | Types into the focused element. |
| `key <key> [modifier...]` | One key: a letter, a digit, `return`, `escape`, `tab`, `space`, `delete`, `up`, `down`, `left`, `right`, or a key code. Modifiers are `command`, `shift`, `option`, `control`. |
| `settle [seconds]` | Waits for the app, 1 s by default. |

- A label is an accessible description, title or placeholder.
- Only visible matches count, because WebKit's tree reaches a table cell twice and keeps rows a filter hid. With several visible, `n` picks one in reading order. Without `n`, several is a refusal.
- With no match in the window, a match below the fold counts, such as a dialog field under its scroller's edge. The one picked is scrolled into view first.
- `press`, `focus` and `click-at` retry until they land, under `KNOBAS_WITNESS_WAIT_S`, and fail at once when knobas has exited.
- Every command but `settle` first checks that the desktop is still drivable and the screen unlocked. `type`, `key` and `click-at` also need knobas frontmost. They activate it once and refuse if it does not come forward, so no keystroke lands in another application.
- `type` shadows bash's builtin of that name, so a steps file cannot use the builtin.
- The demo profile is shared. A source a steps file adds stays in the profile, and its credential is gone at exit because the secret store is in memory. A steps file that adds a source removes it again.

| Variable | What it does |
| --- | --- |
| `LIVE_SHOT_PRODUCT` | `gitea`, `forgejo`, `teamcity`, `jira` or `confluence`. Brings that product up first through `live_session` and exports its seed variables to the steps file (`type "$KNOBAS_GITEA_TOKEN"`). Gitea and Forgejo take the Gitea lock. TeamCity, Jira and Confluence take the JVM lock. The variables come from `./seed --env`, or `./seed --env-forgejo` for Forgejo. Unset, it starts no product and takes no product lock. |
| `KNOBAS_WITNESS_WAIT_S` | The guard on `press`, `focus` and `click-at`, 60 s by default, whole seconds. |
| `LIVE_SHOT_DRY_RUN` | `1` stops once the lock is held. `witness-unit` uses it. |

### What live-shot does around your steps

1. With `LIVE_SHOT_PRODUCT` set, it brings the product up before it takes the screen lock. A JVM product can queue for its lock for 45 minutes, and that wait does not hold the screen.
2. It takes the lock, compiles the helper and checks the desktop.
3. It starts `just demo` and writes its output to `<out dir>/demo.log`. It waits up to 15 minutes for the knobas process, then up to 5 minutes each for a window and for the web view to draw.
4. It gives the window the witness's known size (#2056): fullscreen when AeroSpace is running, so a tiling window manager does not resize it between shots, and refused below 1241 points wide. See [What a run does](#what-a-run-does).
5. It sources the steps file.
6. At the end it quits the app with ⌘Q. If the app is still running it is killed, and the embedded PostgreSQL it left on the demo profile is stopped, as the witness does (#1967). The product is put back as it was found and its lock released.

### Troubleshooting live-shot

| Symptom | Cause | Fix |
| --- | --- | --- |
| Any failed command | The step could not land. | Read `<out dir>/failed.png` (what the window showed) and `<out dir>/failed-ax.txt` (its accessibility tree), then fix the step. |
| `N visible elements labelled '...' to press, wanted 1 (add <n> to pick one)` | Several visible matches. | Add `n`. |
| `a desktop witness or another live-shot holds ...lock` | One at a time, or a killed run left the lock. | Wait, or `rmdir` the lock if nothing is running. |
| `knobas is already running` | A shot names one window. | Quit it. |
| `port 1420 is taken` | Another `just demo` or `just dev` is up. | Stop it. |
| `'just demo' exited before knobas started` or `no knobas process within 15 minutes` | The build or the dev server failed. | Read `<out dir>/demo.log`. |
| `knobas is not frontmost and would not come forward` | Another application kept the front. | Leave the machine alone during the run. |
| `the window could not be given a size the steps run at` | The window is narrower than 1241 points and could not be widened. | Give it an empty workspace or a wider screen. |
