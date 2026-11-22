# 2026-11-21: A site keeps its sign-in past a restart, and can sign itself in

The ux29 pass, PR #2479. Björn: "can we save the login for confluence, jira etc. kinda in a way where i don't have to
keep loggin in when it restarted, and knobas logs in automatically? (when i launch the website inside knobas)", and
later "when jira/confluence restart they lose the cookie i think". ADR-0050 is amended; decisions §13 (b) is built.

## Why a sign-in did not last (measured)

Each product's sign-in, read from its `Set-Cookie` headers on the live testenv products, without and with its own
"remember me":

| Product | Session cookie | Expiry | With "remember me" |
|---|---|---|---|
| Gitea 1.27.3 | `i_like_gitea` | none: ends with the browser process | `gitea_incredible`, 31 days |
| Forgejo | `session` | none | `persistent`, 31 days |
| Jira Data Center 10.3.24 | `JSESSIONID` | none | `seraph.rememberme.cookie`, 14 days |
| Confluence Data Center | `JSESSIONID` | none | `seraph.confluence`, 14 days |
| TeamCity 2026.1.3 | not on disk after a sign-in | none | `RememberMe`, persistent (its box is ticked by default) |
| Uptime Kuma | not read (it signs in over a socket) | not read | signed in again after a new start |

- **A browser-session cookie is never written to disk** by WebKit or WebView2, so it ended with knobas. On the Mac,
  after a sign-in to Gitea in a site, the site's store (`~/Library/WebKit/<app>/WebsiteDataStore/<id>/Cookies`) held
  `gitea_incredible` alone, written within a second of the sign-in; `i_like_gitea` was never there.
- **A Jira or Confluence restart ends its sessions on the server too** (Björn's hypothesis, measured): after `docker
  restart knobas-jira`, `JSESSIONID` answered 401 with or without "remember me", and `seraph.rememberme.cookie` alone
  answered 200; after `docker restart knobas-confluence`, `JSESSIONID` no longer authenticated and `seraph.confluence`
  alone signed in as `knobas`. So the cookie that lasts across both restarts is the one the product sets for
  "remember me".
- **WebView2 writes its cookie store late.** On Windows the site's `Network/Cookies` file kept its creation time for
  the first half minute after a Confluence sign-in (Chromium commits every 30 s) and was written at 30 s and again at
  the quit. TeamCity's `RememberMe`, set 18 to 22 s before a quit, reached disk only at the quit. So quitting now waits
  for each site's browser process to end after its view is dropped (measured 172 to 203 ms with one site).
- **The store's identifier is stable**: the same function of the keychain service and the source id on every run; the
  store survived quit and start on the Mac.
- **The forget list** was empty in every run; nothing removed a store.

So the fix is the saved sign-in with "remember me" ticked: a fill makes the product set its lasting cookie, which then
carries the next start, and a fill signs in again when it has run out.

## What is built

- **Settings › Sources › ⟨source⟩ › Site › Saved sign-in**: *None* and *Save one…*, or *Saved for ⟨username⟩, never
  shown*, *Change…* and *Remove*. The dialog has a username and a password field; the password is never read back.
- **Kept** in the source's keychain item: envelope `v` 3 with an optional `web`, written only when one is saved, so an
  item without one keeps the v2 bytes an older build reads. Re-entering the credential keeps it; deleting the source
  removes it. Never in the database, a log line or a backup.
- **Filled** by `crates/knobas-site/src/fill-sign-in.js`, only into a page whose top-level origin is exactly the
  source's own, checked natively at that moment against both the committed address and the view's own
  (`knobas_site::sign_in::may_fill`). On *Fill sign-in* in the site's bar, and by itself once a page loads at a sign-in
  path the adapter declares in its descriptor's new optional `web_sign_in` (Jira `/login.jsp`, Confluence
  `/login.action`, Gitea and Forgejo `/user/login`, TeamCity `/login.html`), or anywhere on the own origin for one that
  declares `any_page` (Uptime Kuma, whose sign-in is a dialog). A form not drawn yet is looked for every half second
  for six (Jira 10's form is React: the first look found none, the second filled it).
- **One try**: a sign-in form still on the page after a fill means the site did not take it; the bar says
  "⟨source⟩ did not take the saved sign-in" and nothing is filled by itself until a press.
- **Not filled**: a one-time code ("⟨source⟩ asks for a code. That one is yours to type."), a change-password form, a
  form in a frame, a hidden password field, and a provider's page (the bar's chip: "not ⟨source⟩'s own address, so its
  sign-in is not filled").
- **External adapters** (Björn: "how am i supposed to open xwiki as nicely as confluence"): `knobas:adapter@1.7.0`
  reads the same `web_sign_in` from a component's descriptor JSON (the descriptor crosses the WIT as its serde JSON, so
  the key is the WIT's equivalent of a `web-sign-in-paths` field; the 1.7.0 `descriptor` doc names it). 1.7.0 and not
  1.6.0 because ux29 ships 1.6.0 and freezes it (Björn, 2026-11-22): the 1.7.0 world has 1.6.0's types, the SDK keeps
  the 1.6.0 world as `wit-1.6.0/` (feature `v1_6`), and this work is on the branch `ux30-signin`. The host reads the
  key from a 1.7.0 component, drops it from one built against 1.0 to 1.6, which keeps *Fill sign-in* alone, and
  refuses a malformed one at load. The component only says where its sign-in page is: the sign-in stays in the host's
  keychain item and is typed by the host into its own web view, so the sandbox is unchanged. The SDK re-exports
  `WebSignInPages`; the fixture declares `/login` in every build (the host test holds 1.7.0 to it and 1.6.0, 1.5.0 and
  1.3.0 to none), and an app test installs the 1.7.0 fixture and sees its declared page ask for the fill. The XWiki
  adapter (`knobas-adapter-xwiki`, branch `web-sign-in`) declares `/bin/login/XWiki/XWikiLogin` after its base URL,
  whose "Remember me" (`j_rememberme`) the fill ticks; it builds against 1.7.0, and against 1.6.0 and 1.5.0 with
  `INTERFACE=`.
- **Quitting** drops every view and waits up to 3 s for each site's browser process to end on Windows, when Chromium
  writes its cookie store.
- **Found on the way (macOS 27)**: a site's view covered its own bar. The window's content view runs under the title
  bar (900 points for an 868-point page 32 points down), and the view was placed against the content view. It is now
  placed against the page (`mac::under_title`, the content view's height less `contentLayoutRect`'s).

## In the real apps

- **Mac, the real window** (before Björn took the Mac back): Gitea filled by itself on `/user/login` and went to the
  dashboard; after ⌘Q and a new start the site opened signed in with nothing filled, twice. Jira 10 filled on its second
  look (its form is React) and signed in; the site's store held `seraph.rememberme.cookie` alone. The Jira proof after
  a Jira restart was cut off when the Mac went back to Björn; the cookie's survival of that restart is measured above
  with curl.
- **Windows, knobas's WebView2 window on his PC** (scratch profile, a debug build of this head in ux28's unpacked
  layout, CDP from a portable Node, Björn idle the whole time): Confluence sent the site to `/login.action`, knobas
  filled it on the second look and Confluence signed in; quit; Confluence's container restarted; a new start opened the
  site signed in with nothing saved to fill. TeamCity filled on the first look (its page encrypts the password itself),
  quit, new start: signed in. Uptime Kuma's dialog was found on `/dashboard` and filled; after a new start the dashboard
  showed signed in. Five runs' logs hold none of the three passwords.
- **XWiki through its external adapter, the Mac's real window** (`knobas-adapter-xwiki` built against 1.7.0, installed
  through knobas' own install, the test wiki's account, a forward of my own to its port): on a forgotten session
  XWiki sent the site to `/xwiki/bin/login/XWiki/XWikiLogin;jsessionid=…`, knobas filled it by itself and XWiki
  signed in; after ⌘Q and a new start the site opened signed in with nothing filled. Two runs' logs hold no password.
  The run found three faults, fixed before it passed:
  - the `;jsessionid` a servlet container puts in the path on a first visit hid the declared page; a path's `;`
    parameters are not part of the page now;
  - XWiki keeps a sign-in form in a panel of every signed-in page, so the check after a fill said "did not take" while
    XWiki was signed in. "Not taken" is now the sign-in page again, as §13 (b) said from the start; the form check
    decides it only for an `any_page` adapter (Kuma), and still finds a second factor;
  - *Forget…* on the Mac failed with "data store is currently opened": it ran the instant the view was dropped, so
    the session stayed. The platform now reports the removal and *Forget…* tries again for up to 4 s before listing
    it for the next start.
- Not run in a window: Gitea and Forgejo on Windows, Jira on Windows, and Forgejo, Confluence, TeamCity and Kuma on the
  Mac; XWiki on Windows.

## Tested

Rust: the envelope (v2 reads, v3 round-trips byte for byte, a v3 only with a sign-in, Debug redacts), the descriptor's
`web_sign_in::check` and battery clause 16, `sign_in.rs`'s origin refusals and one-try rule, the controller on the
fake host (filled once per landing, refused off the own origin, in a redirect and with nothing saved, Kuma's form
looked for while it draws), and a log capture through a whole fill with no line holding the password. Mutants: fill on
any allowed origin, fill twice per landing; each fails its test. Front end: the fill script against each built-in
kind's recorded sign-in page (`crates/knobas-site/tests/sign-in-pages/`) and the tricky ones, the Settings row and
dialog, the bar's button and notes.

## Found in the final suite (2026-11-22)

- ***Forget…* on the Mac left the session alive** when the site had been open in the same run. The Mac's real window,
  Confluence: *Forget…* removed the store's folder, yet the next open was signed in at once, a reload too, and the
  quit wrote `Cookies.binarycookies` back, so the start after it was signed in as well. While another view lives
  (knobas's own page), WebKit's network process keeps the store's session in memory after its files are removed and
  hands it to the next view with that identifier. `mac::forget` now clears every kind of data through the store
  first (`removeDataOfTypes:modifiedSince:`, which reaches that session) and removes the store on the next try, half
  a second later, since WebKit refuses to remove a store while the clear's own handle on it is open.
  `crates/knobas-site/tests/mac_forget.rs` loads a page that sets a lasting and a session cookie into a store, beside
  a second view on the default store, forgets the store, and asks it for cookies: both were still there before, none
  after. In the window after the fix: cleared, removed 0.5 s later, and the next open went to `/login.action`.
- **A source added while knobas ran had no Site section** until the next start (the Mac's real window, XWiki added
  through *Add a source*), so its pages opened the browser. The page read the sites' settings once. It now reads them
  again when the source list gains a source it has no setting for (`siteStore.lacks`), and not on a list with nothing
  new. `TfShell.test.svelte.ts` adds a source to a running shell: no setting before, its setting after one more read.
