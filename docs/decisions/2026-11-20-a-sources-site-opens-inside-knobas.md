# 2026-11-20: A source's site opens inside knobas, in a web view of its own, and going back never closes it

**Asked (Björn):** "one feature that would be really cool is to open the website inside knobas itself when you want.
Example: you're browsing a doc page and you want to edit or do anything with it that you can't inside knobas. Right now
it would open your browser, you'd have to maybe even log in and all that stuff. Knobas does already have the logindata,
no? [...] maybe a feature that can be turned on/off for each source [...] It should be possible to go back to knobas
aswell, so all the navigation should stay accessible. But I also don't want it to be too easy to go back to not
accidentally lose data when i press esc once on accident (maybe ask to press esc twice, and also have dedicated x etc.)"

Designed by Fable, built by Opus. The architecture is ADR-0048. This note is the forks and why each went the way it did.

## Decided

- **A site is a native web view under knobas's top bar**, one per source, on a page at `#/site/<source id>`. Off until
  *Open its pages inside knobas* is on for the source. With it on, every address of that source opens there, from a
  header, a card, the right-click menu or a link in a comment. *Open in browser* stays one press away, and a middle
  press or a ⌘ press takes the browser.
- **knobas's token does not sign the site in.** An API token, an access token and Uptime Kuma's key open no web
  session. The site keeps its own session on the computer, apart from knobas's page and from every other source's, and
  he signs in once on the site's own page, SSO and a second factor included.
- **A site shows its source's own addresses and the ones he allowed.** A redirect to an unknown host while a site is
  opening stops and asks, once per host, which is how a sign-in at an identity provider gets in. A link to another
  host from a page opens the browser. A link to another source's address opens that source's site.
- **Going back hides the site and closes nothing.** A nav entry, the launcher, Back and Esc twice all leave the page in
  it alive. Closing is the bar's × or ⌘W. Closing, reloading and pointing a site elsewhere ask the page first, with the
  page's own `beforeunload`, and ask him only when the page objects.
- **Esc is the site's. A second Esc within 500 ms goes back.** The bar shows two key caps beside *Back to ⟨name⟩* and
  holds the first down while the second still counts.
- **While a knobas layer is open the site is hidden**, and its rectangle says what is still there. A native view lies
  above the page, and nothing knobas draws can stand over it.
- **Coming back refreshes what he stood on.** The last five mirrored items whose pages the site showed are read again
  with the one-item refresh.

## What was weighed

- **Tauri's own child web view.** It hands every web view the IPC bootstrap, every plugin's script and knobas's
  schemes. In the spike a page in one read knobas's own `index.html` over `tauri://`. A raw wry view gets none of it.
- **A window of its own.** No layering problem, and not "inside knobas". Kept as the way out.
- **A frame.** Gitea, Forgejo and Uptime Kuma answered `X-Frame-Options: SAMEORIGIN` on 2026-11-20.
- **A saved user and password filled into the form**, his own suggestion. Kept as an opt-in last step: it is a script
  carrying a password into a page knobas does not control, and it cannot pass SSO.
- **One session for all sources.** One sign-in with SSO. A page of one source could then send requests with another's
  cookies.
- **Esc once, or a prompt with seconds to answer.** Once takes Esc from every site. A long window makes any two Escs in
  a site a way out.
- **A still picture under a layer.** Too slow to take inside a key's 100 ms on a wide window.

## What it costs

- A sign-in per source, once. With SSO too.
- On Windows a browser process tree per live site: about 310 MB of working set before its page, measured on his PC
  with WebView2 157. At most two sites stay alive until he sets *Sites kept open*, and a hidden one is put to sleep.
- On Windows a refused address still receives the navigation's first request. WebView2 sends it while it asks.
  Nothing of the answer is shown or run. On the Mac nothing is sent.
- ⌘K, ⌘T, ⌘R, ⌘W, ⌘L, ⌘, and the zoom and history chords never reach a site's page. ⌘K is the one a site may want.
- On the Mac two of wry's delegates are wrapped and one WebKit call is private. A wry or WebKit change can break them,
  and the probe binary is how that is seen.
- A site that signs in through a pop-up, or with Google, does not sign in here.

## How it was built, and where it departed from the design

Built by Opus from Fable's decision list, in its build order, with
these departures, each a small call:

- **The rules and the native view are a crate of their own, `knobas-site`**, not a module of `knobas-app`. The rules,
  the controller and the fake platform are tested in seconds without the app, and the probe binary
  (`crates/knobas-site/examples/site_probe.rs`, behind the `probe` feature) is built for Windows from the Mac without
  the database and the adapters. The app's half (`crates/knobas-app/src/site.rs`, `commands/site.rs`) is the Tauri
  shell, the setting rows and the commands. The source scans of decisions §3 and §4 cover both crates.
- **The probe binary is the controller's own**, driven against a local test site in a `tao` window with a stand-in
  shell page, rather than the spike tidied: it checks the code knobas ships.
- **Two commands and one event more than §17 lists**: `site_suggest` (an *Open in ⟨source⟩* that went to the browser
  for an address the site may not show, offered in Settings), `site_quit` (*Quit anyway*), and `site:quit` (the sites
  that hold changes when knobas is asked to quit). `site_show` carries whether `Esc` has somewhere to go, which the
  native key hook needs.
- **The keyboard asked for before a page committed goes to the site when its view first shows.**
- **A held chord of knobas's stays knobas's**: its repeats never reach the page either.
- **`site:status` is sent when it changes, not coalesced to 50 ms**: a site sends a handful per load.
- **The page menu's trim** (Windows) is one rule for the main window and a site (`knobas_site::menu`), each with its
  own list. The saved web sign-in (§13 b) is not built: Björn's call, after he has used the rest.

## Measured on Björn's PC (2026-11-20, 16:45)

`crates/knobas-site/examples/site_probe.rs` on WebView2 157.0.4341.60, Ryzen 5 5600X, Björn idle by the guard, in a scratch
profile under his Temp folder, removed after. Every check passed: the page saw no `__TAURI_INTERNALS__` or `isTauri` and
fetched none of `tauri://`, `ipc://` or `knobas-render://` (nor their `http://*.localhost` forms); a second site on the same
origin started with no cookie and a second run got each site's own back; a frame on another origin loaded; a link to
another origin went to the browser and was never shown (its first request reached the server, as §6 says); a redirect to
an unknown origin while landing stopped and asked; `window.open` opened no window and a `target=_blank` link on the site's
own origin loaded in place; a download landed in the folder knobas named (WebView2 had already cleaned `../../probe.txt`
to `_.._probe.txt`); the probe answered changed for a page that objects and a close asked; a typed key reached the page,
the first Esc did and armed, the second was taken and went back, F5 was taken and never reached the page; microphone,
location, clipboard read and notifications were refused. Making a view took 231 to 245 ms on the main thread; the
stand-in shell's longest frame around it was 52 ms. A shown site's WebView2 tree held about 308 MB of working set, the
shell's 274 MB; a hidden one, at WebView2's low memory level, fell from 243 MB to 21 MB.

The probe found one fault, fixed before this note: after the bar's *Open here anyway* WebView2 asked "Leave site?" itself
and the load waited on it. A forced close, reload or new address now answers the page's own leave dialog once, within 3 s
(`SiteView::leave_freely`).

## Tested

The rules are pure functions with tests and mutants: the navigation table, the key table, Esc's timing, a site's
states, the session's folder. The platform code is checked by `crates/knobas-site/examples/site_probe.rs` on both platforms, whose log is
in the PR, and by a hand list for the keyboard. The page's budget, its press trips and `just moves` run on the dev
bridge's fake.

## Reversing it

Turn the switch off for a source and it opens the browser as before. Removing the feature removes the `site` module,
the page and `openWeb`'s first branch. The sessions are folders under the profile's `sites/` on Windows and WebKit
data stores on the Mac, removed by *Forget…* or by deleting the source.
