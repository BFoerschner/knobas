---
status: accepted
---

# A source's site is a web view of its own, with no way into knobas

Extends [ADR-0040](0040-a-docs-source-projects-at-read-time-through-a-shared-reader.md), which runs a page's embedded HTML in a frame that cannot reach knobas, and keeps the rule of `app/src/lib/shell/open-external.ts` that a mirrored address is a remote system's data. Björn asked for a source's website inside knobas's window, signed in, so that what knobas cannot do is one press away without the browser. A website is other people's code and other people's text. Decided 2026-11-20 (Fable, on Björn's ask; `docs/decisions/2026-11-20-a-sources-site-opens-inside-knobas.md`): **a source's site runs in a native web view knobas makes with raw wry as a child of the main window, outside Tauri's web view manager, with a session per source, and nothing crosses between it and knobas but an address, a title and a yes or no.**

- **Not a Tauri web view.** Tauri gives every web view it builds the IPC bootstrap with the invoke key, every plugin's initialization script, the `tauri`, `ipc` and `knobas-render` schemes and the IPC handler, whatever its URL (`manager/webview.rs`, Tauri 2.11.5). knobas's commands need no permission, so the remote-origin check would be the only wall. A view made with `wry::WebViewBuilder::build_as_child` is given none of those, and the builder is never handed an IPC handler, a custom protocol or an initialization script. No capability names the view, and none could.
- **A session per source.** Cookies, storage and cache live in a WebView2 user data folder under the profile's `sites/<source id>` on Windows and in a `WKWebsiteDataStore` by an identifier made from the profile and the source id on macOS 14 and later. Never knobas's own store, never another source's. Not in the database, a backup or a share export. Deleting the source removes it.
- **knobas's credentials stay out.** The site signs in on its own page. No token, key or stored password of knobas's is sent to it. A saved web sign-in, if one is built, is a separate credential the person saves for that purpose, filled only into a top-level page on the source's own origin.
- **An allow-list of origins decides what a site shows.** Every top-level navigation is asked, redirects and new windows included: the source's own origin and the origins the person allowed for that source. Another source's address goes to that source's site. A redirect to an unknown origin while the site is opening is stopped and the person is asked, once per origin. Anything else leaves for the system browser through `openExternal`, and a scheme other than http and https goes nowhere. An origin is never allowed because a mirrored record names it. On Windows WebView2 sends a navigation's first request while it asks (measured, WebView2 157), so a refused address is never shown or run and still receives one GET with this site's own cookies for it. On macOS a refused address receives nothing.
- **Two scripts, each on demand.** knobas evaluates a script in a site only to ask the page whether it holds unsaved changes (the page's own `beforeunload` event) and, if a web sign-in is built, to fill it. Each is a named constant, run at a moment the person caused, with its answer read as untrusted text. Nothing is added at document creation and nothing stays in the page.
- **What the site is refused.** New native windows. Camera, microphone, location, notifications and programmatic clipboard reads, with no prompt. A way past a certificate error. WebView2's host channel, host objects, password saving and form autofill. Developer tools in a release build.
- **What knobas takes from the site**: the top-level address, the document's title, load and download events, and the probe's yes or no. It draws them as text.
- **Leaving is not closing.** Another knobas page hides the view. Closing, reloading and loading another address ask the page first.
- **Keys are read natively.** With the site holding the keyboard, knobas claims a short fixed list of chords before the page sees them, through WebView2's `AcceleratorKeyPressed` and an `NSEvent` local monitor. Every other key is the site's. Esc is the site's, and a second Esc within half a second goes back to knobas.
- **The site steps aside for knobas's layers.** A native view lies above the page, so the view is hidden while a dialog, a menu or the launcher is open.

## What it is defended against

| A site's page tries to | What stops it |
|---|---|
| Call a knobas command, or read knobas's bundle or its schemes | The view has no IPC handler and no custom scheme. |
| Read or ride another source's session, or knobas's own | A store per source. Requests from one site carry no other site's cookies. |
| Take the view to a page that imitates a sign-in on another host | The allow-list. An unknown host is asked about by name, or opens the browser. The bar names the source and tones an address that is not its own. |
| Open a file, an application or a link with another scheme | Only http and https leave, through `openExternal`. |
| Use the camera, the microphone, the location or the clipboard | Refused without a prompt. |
| Get past a broken certificate with one click | No such page is shown. |
| Put markup into knobas's page through its title or address | Both are drawn as text, cleaned and cut. |
| Learn his knobas token or password | Neither is ever sent to a site. |

Not defended against: a source's own page doing harm inside that source with his session there, which is true in any browser. A page on the source's own origin that imitates its sign-in form to a saved web sign-in, which is why that is opt-in. And on Windows, one request to a refused address, which a page could send by other means anyway.

## Rejected

- Tauri's unstable child web view. Its wall is one origin check, and in the spike a remote page in one read knobas's `index.html` over `tauri://`.
- A frame in knobas's page. The sources refuse to be framed (`X-Frame-Options: SAMEORIGIN` measured on Gitea, Forgejo and Uptime Kuma), and a frame shares knobas's process and store.
- A window of its own. It meets every rule above and is not inside knobas. It is the fallback: nothing here depends on where the view sits.
- One session for all sources. One sign-in under SSO, and one source's page can then send requests with another's cookies.
- Signing the site in with knobas's token. No source kind documents a way but TeamCity, and there it means a credential in a request header the site's own page can redirect.
- Following a source's redirects to any host while it opens. A redirect is the site's choice, and its pages hold other people's text.

## Consequences

- `wry` is a direct dependency of `knobas-app`, pinned to the version `tauri-runtime-wry` resolves. A Tauri upgrade moves both.
- On macOS two of wry's delegates are wrapped: the navigation delegate, because wry's hook cannot tell a frame from the page, and the UI delegate, because wry answers no JavaScript panel and grants media capture. One WebKit delegate method used there is private. `crates/knobas-site/examples/site_probe.rs` is run on both platforms when wry, Tauri or the module changes.
- On Windows each live site is a browser process tree of its own, about 310 MB of working set before its page. At most two stay alive until the person says otherwise, and a hidden site sleeps.
- A site's page never receives the chords knobas claims. ⌘K is among them.
- The frozen surfaces grow by the site commands and events (contract §10.8). No existing command changes.
- `CONTEXT.md` gains *Site*, *Site session* and *Allowed address*.

## Amended 2026-11-21 (ux29): a saved web sign-in, filled into the site's own form

Björn asked for a site to stay signed in across restarts and to sign itself in. Measured on the live products: Jira's and Confluence's `JSESSIONID`, Gitea's `i_like_gitea` and Forgejo's `session` carry no expiry, so they end with the site's browser process, and a server restart ends Jira's and Confluence's sessions on its own side too. What lasts is the product's own "remember me" cookie (`seraph.rememberme.cookie` for 14 days on Jira, `gitea_incredible` and Forgejo's `persistent` for 31), which Jira still accepted after its container restarted. So decisions §13 (b) is built, opt-in per source:

- **Kept** in the source's keychain item beside its credential: the envelope's `v` goes to 3 with an optional `web` member, written only when one is saved (an item without one stays v2, byte for byte). Re-entering the credential keeps it, deleting the source removes it. Never in the database, a log line or a backup. Settings shows the username alone.
- **Filled** only into a page whose top-level origin is exactly the source's own, checked natively at that moment against both the committed address and the view's own (`knobas_site::sign_in::may_fill`): never on an allowed address, never in a redirect. On *Fill sign-in* in the site's bar, and by itself once a page loads at a sign-in path the adapter declares (the descriptor's new optional `web_sign_in`: `/login.jsp`, `/login.action`, `/user/login`, `/login.html`), or on any page of the own origin for one that declares `any_page` (Uptime Kuma's dialog). The script ticks the form's "remember me", so the product keeps the session past its own restarts and knobas's.
- **One try.** A sign-in form still on the page after a fill means the site did not take it: the bar says so and nothing is filled by itself until a press. A one-time code is never typed, and a provider's page (an allowed origin) is the provider's.
- **Quitting** drops every view and then waits up to 3 s for each site's browser process to end on Windows, which is when Chromium writes its cookie store (it commits only every 30 s before).

**External adapters too** (Björn: "how am i supposed to open xwiki as nicely as confluence"): `knobas:adapter@1.7.0` reads the same `web_sign_in` from a component's descriptor JSON, so an external source's site is filled by itself on its declared page as a built-in's is. The component only declares where the page is; the sign-in stays in the host's keychain item and is typed by the host into its own web view, so the sandbox is unchanged. A component built against 1.0 to 1.6 has the key dropped and keeps *Fill sign-in* (1.6.0 shipped frozen with ux29). The XWiki adapter (`knobas-adapter-xwiki`) declares `/bin/login/XWiki/XWikiLogin` after its base URL and its "Remember me" (`j_rememberme`) is ticked like the others.

The exposure the original text names stands: a page on the source's own origin that imitates its form receives the password, as it would from a browser's password manager.

## Amended 2026-11-22 (ux30): a site that signs in with its source's own credential

Björn asked for Pleasant's web interface to open from the Passwords page and sign itself in. A password manager's web interface signs in with the very account its API does, so a second saved password would only be a copy. Decided by Fable while Björn was busy (`docs/decisions/2026-11-22-a-password-managers-web-interface.md`); this changes "No token, key or stored password of knobas's is sent to it" above, and the defence table's "Learn his knobas token or password" row, for such a source:

- **The source's own credential is typed into the declared sign-in page**, under the same origin check as a saved sign-in (`may_fill`), when the adapter declares `web_sign_in.uses_credential` on declared paths (never `any_page`) and the source signs in by username and password: the source's `username` setting and the keychain item's password. Nothing new is stored; the envelope stays as it was.
- **Unless it is turned off** (Settings › Site › *Turn off*, a `knobas.setting` row), or **another sign-in is saved**, which wins.
- **A password source's site is remembered as any site's is**: the fill ticks "remember me" (Pleasant 9.5.4 keeps it 4 hours).
- **A password source's site opens inside knobas by default**: *Open its pages inside knobas* reads on until it is set, so the Passwords page's *Open web interface* opens the site.
- Only a press loads the site; knobas never loads it at start or in the background.

The adapter also declares where its site shows an item (`web_links`, `knobas:adapter@1.7.0` for external adapters): templates after the typed base URL, so an address built from them can only name the source's own site.

## Amended 2026-11-22 (ux30): a password manager's site loads nothing from other websites

Pleasant 9.5.4's sign-in page frames the vendor's news with the server's version and install id in its address, and the allow-list above rules top-level navigations only, so the frame loaded. Björn: "never contact Pleasant", and yes to blocking content from other websites inside password-manager pages. Decided by Fable while Björn was busy (`docs/decisions/2026-11-22-a-password-managers-site-loads-nothing-from-other-websites.md`); this adds to "An allow-list of origins decides what a site shows" above, for a source with the Passwords role only:

- **Every load is ruled, not only the top-level navigation.** While the page on top is anything but a page on an allowed origin, frames, scripts, styles, images, fonts, fetches, XHR and WebSockets go only to the source's own origin and the origins the person allowed for it (`ws(s)` as `http(s)` on the same host and port; `data:`, `blob:` of those origins and `about:` contact nobody). A page on an allowed origin, an identity provider the person let in, loads as in a browser. Origins are compared exactly. One pure function holds the rule (`knobas_site::confine`); the top-level rule is unchanged.
- **The platform refuses, not a script.** macOS: a `WKContentRuleList` made from the same inputs, on the view before it loads anything, and the navigation delegate for frames. Windows: `WebResourceRequested` answers 403 and `FrameNavigationStarting` cancels.
- **It fails closed.** A list that does not compile, or a handler that does not register, loads nothing and says so, with *Retry*. A changed allow-list reaches a live site before the next load knobas starts.
- **Settings says so** under the site's addresses. Nothing in the bar.

- **WebRTC is off on a confined site's view on macOS** (measured: a page's STUN packet is outside any content rule list).

Measured in the Mac window against Pleasant 9.5.4 with every load aimed at a local listener on another origin: before, the vendor's news frame and every kind of load reached it; after, nothing did, and the sign-in fill and the web client worked. Not defended against, for now: WebRTC on Windows, where WebView2 has no switch for it (owed, with the rest of the Windows measurement); a service worker's fetch and CSP reports (not measured); a removed allowed origin for the milliseconds a live Mac site's new list compiles; and a script sending the top-level address elsewhere after landing, which opens the browser as line 5 says.
