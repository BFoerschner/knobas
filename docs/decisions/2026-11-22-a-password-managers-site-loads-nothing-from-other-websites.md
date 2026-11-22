# A password manager's site loads nothing from other websites (2026-11-22)

The ux30 pass, after `2026-11-22-a-password-managers-web-interface.md`. That run found Pleasant 9.5.4's sign-in pages
(`/SignIn/Direct`, the `/Account/SignIn` landing) framing
`https://pleasantpasswords.com/product-news?FeedID=…&Version=9.5.4.0.Unregistered,3&ref=<install id>`, so each signed-out
load of the sign-in page inside knobas sent the vendor the server's version and install id. Björn's rule is "never contact
Pleasant", and to "should knobas block content from other websites inside password-manager pages?" he answered "yes".
ADR-0050 is amended. Björn was busy, so Fable ruled (one ruling, xhigh); it is listed at the end.

## What is built

- **For the role, not for Pleasant.** A site whose source's adapter declares the Passwords role is *confined*
  (`SiteRules::confined`, set by `knobas-app`'s `settings_and_rules`). A Bitwarden or Vaultwarden adapter gets it with
  no change. No vendor host list, no off switch, and a descriptor cannot opt out. Without the adapters' descriptors
  (the sources state not up yet) a site is confined until a read that knows its role.
- **The rule** (`crates/knobas-site/src/confine.rs`, one pure function, `Confine::may_load(top, url)`):
  - while the page on top is anything but a page on an allowed origin (the source's own above all), a load may go
    only to the source's own origin and the origins the person allowed for that source: `http(s)` there, `ws(s)` on
    the same host and port, `blob:` of them, and `data:` and `about:`, which contact nobody. Everything else is
    refused, an address that does not parse included. Origins are compared exactly (scheme, host, port): another port,
    `http` on an `https` source, a subdomain and another knobas source are other origins;
  - while the page on top is on an allowed origin (an identity provider the person let in), nothing is refused: it is
    that provider's page, holds no vault data, and loads its own scripts from its CDN.
  - `decide`, the top-level rule, is unchanged.
- **macOS (WKWebView):** a `WKContentRuleList` compiled per site from the same inputs (`Confine::content_rules`): one
  rule blocks every address unless the top page is on an allowed origin (`unless-top-url`), then an
  `ignore-previous-rules` per origin's `http(s)`, `ws(s)` and `blob:` prefix (a default port written both ways), and
  for `data:` and `about:`. A unit test reads the list the way WebKit does and checks it against `may_load` over a
  table of 29 addresses under 10 top pages for 3 sites. The view is made with no address; it loads once the list is on
  its `WKUserContentController` (the gate, `confine::Gate`). The navigation delegate also cancels a frame the rule
  refuses before it starts (`mac::KnobasSiteNavigation`). The compile takes a few milliseconds (logged as `site
  confined`). WebKit reports no blocked request to the app, so the Mac logs one line when the list is installed.
  **WebRTC is off** on a confined site's view (WebKit's `_peerConnectionEnabled`, SPI, answered on this macOS; a
  warning in the log where it is missing): a content rule list sees no UDP, and the measurement below caught a page's
  `RTCPeerConnection` sending STUN to a host it named.
- **Windows (WebView2):** `WebResourceRequested` with a `*` filter for every context (workers too, through
  `AddWebResourceRequestedFilterWithRequestSourceKinds` where the runtime has it) answers 403 to a refused load, and
  `FrameNavigationStarting` cancels a refused frame. Both read the top page from `Source()` and the rule from a cell
  the controller updates, so a change applies at once. One log line per refused origin and resource kind per page,
  with scheme, host and port, never a path or query (the news address's query carries the install id). The handler is
  registered only for a confined site, so other sites pay nothing.
- **Fails closed.** A rule list that does not compile, or a filter or handler that does not register, loads nothing
  and shows the failure rectangle with `confine::NOT_CONFINED` ("knobas could not block other websites' content on this
  site, so it loaded nothing. Retry, or quit and start knobas again.") and *Retry*, which tries again. A
  `SiteFailure::Other`: the failure enum crosses the frozen IPC.
- **A changed allow-list reaches the live view** before the next load knobas starts: `Sites::set_rules` (Settings'
  *Allow* and *Remove*) and `Sites::answer` (Allow while landing, before the landing loads again) hand the view its new
  rule. On macOS the new list compiles while the loads knobas starts wait at the gate; until it lands (milliseconds) the
  page already loaded keeps the old list, so a removed origin can still load in that moment. A compiled list is never
  reused after the allow-list changed.
- **Settings › Sources › ⟨source⟩ › Site**, under *Addresses it may show*, for a password source only: "Content from
  other websites is blocked on this site. Allowed addresses still load." (`SiteSetting::confined`, an additive IPC
  field). Nothing in the site bar.

## Measured (the Mac window, Pleasant 9.5.4 in the WSL VM)

A debug build of `ux-polish` before the change (`6070baf9`) and one with it, each on a scratch `HOME` from the scale
database copy, driven through the real-window probe (DOM presses, no synthetic input; window-only captures). Two
Pleasant sources added with `knobas-app live-sources apply --demo` (the read-only account), one per run, so each run
starts signed out in a store of its own. Between knobas and the VM a front of the agent's own on
`https://127.0.0.1:24801` (a leaf from the loopback-only testenv CA) passed every request on and logged it. **Pleasant
was never contacted:** on HTML answers the front pointed `https://pleasantpasswords.com` at a sink on
`https://localhost:23472` (another origin, which logged every TCP and TLS connection and every request), and on the
sign-in pages it added a probe that tried, all at the sink: an image, a style, a frame, an `<a ping>`, a prefetch, a
preconnect, a script, `fetch`, XHR, `sendBeacon`, a WebSocket, an EventSource, a worker's fetch, two own-origin 302s to
the sink (an image and a fetch), and a WebRTC STUN packet to a UDP listener.

- **Where the news frame is:** on the signed-out landing `/Account/SignIn` (any web client address while signed out),
  not on `/SignIn/Direct`, where *Open web interface* lands. So the news frame loads when a signed-out site is sent to
  a web client address (the run opened `/WebClient/Main`), not on knobas's own sign-in path.
- **Before** (18:59 to 19:00): the landing's "Password Server News" frame showed the sink's page, and the sink logged
  `GET /vendor/product-news` plus every probe kind: image, style, both redirects, fetch, XHR, beacon, the WebSocket's
  upgrade, ping, frame, worker, EventSource, and 20 bytes of UDP. Pleasant's own CSP (`script-src 'self'
  'unsafe-inline' 'unsafe-eval'`) kept the other-origin script out in both runs; WebKit made no prefetch.
- **After, first build** (19:01): the sink logged no connection at all, the log said `site confined … ms=3.3` and
  `a confined site's frame on another origin is cancelled` twice; but **20 bytes of UDP arrived**: WebRTC's STUN.
- **After, with WebRTC off** (19:03): **the sink logged nothing**, TCP, TLS or UDP. The page's own report:
  `fetch`, `fetch-redirect` and the worker "Load failed", `RTCPeerConnection` undefined, the WebSocket and EventSource
  "error". The news frame drew empty. Then Passwords › Vault after › *Open web interface*: `/SignIn/Direct`, knobas
  filled it (`site sign-in answered mode=Fill answer=Filled`), `/SignIn/Success`, `/WebClient/Main` with "Hello, Mara
  Lindqvist", the tree and the credential list, all on the own origin; the sink still silent. The rule list compiled in
  1.8 to 3.3 ms.
- **Not run against the VM:** a folder's deep link. The sync that lists the folders hit the WSL box hanging at 19:05,
  and WSL was then blocked; the stand-in run below covers it.

## Measured again on the Mac alone (a local stand-in, 19:15 to 19:30)

WSL blocked (Björn, ~19:15), so the same front served a stand-in of Pleasant's web client on
`https://127.0.0.1:24801` (shaped like the VM's: `/SignIn/Direct` with `UserName`, `Password` and `RememberMe`, the
signed-out `/Account/SignIn` landing with a "news" frame, `/WebClient/Main?itemId=`), with the API from
`testenv/pleasant/standin.mjs` (20 entries). Every page framed and scripted from another origin and ran the probe, with
no CSP, so the other origin's script was tried too; a second listener played an origin the person may allow.

- **Before:** the news frame, the script and every probe kind reached the other origin, UDP included.
- **After:** the landing, the sign-in page, the web client and a folder's page reached it with nothing; the page's
  own image, script and fetch loaded (`ownScript`, `ownImage`, `fetch-own` true in the page's report). *Open web
  interface* on the source: knobas filled the form 71 ms after it loaded, "remember me" ticked, and the web client
  opened. A folder's *Open web interface* (after *Sync now*: 20 entries, 31 folders, 150 ms) took one 302 through
  `/SignIn/Direct` to `/WebClient/Main?itemId=<the folder>`. **The allow-list, live:** allowing
  `https://localhost:23475` while the site was open recompiled the list (2 origins, 1.6 ms); after a reload that origin
  loaded and the other still got nothing. Settings › Site drew the sentence under the addresses.
- **Found and fixed on the way:** with two password sources on one server address, *Open web interface* on the second
  opened the first's site, which typed the first source's credential into the page (`open-web.ts`: a tie between
  equal bases went by list order; it now goes to the press's own source).

Captures (knobas's window only) are in the agent's report: the VM's news frame with the sink's page before and empty
after, the VM's web client after, and the stand-in's folder page after.

## Not defended against, and owed

- **The VM, owed when WSL is free again:** a folder's deep link against the real server (done on the stand-in).
- **Windows (WebView2), owed:** the whole measurement above in knobas's WebView2 window (`S/owed-on-windows.md`),
  above all **WebRTC**: `WebResourceRequested` sees no UDP and WebView2 has no switch to turn peer connections off, so a
  page's STUN probably leaves there. Per Fable's ruling a page-chosen escape comes back before merge; the candidate is
  Chromium's `--force-webrtc-ip-handling-policy=disable_non_proxied_udp` in the confined site's browser arguments, which
  stops UDP but may leave TURN over TCP. Also whether `WebResourceRequested` fires for a redirect hop and a WebSocket,
  service and shared workers, and a refused top-level navigation's first GET.
- **macOS, not measured:** a service worker's fetch (Pleasant registers none; a dedicated worker's fetch was refused),
  CSP reports (Pleasant's policy names no report address, and only its own server can set one).
- **A removed allowed origin** may still load for the milliseconds the new list takes to compile on a live Mac site.
- **The top-level navigation gap** (Fable's 12): a script on the page can still send the top address to an unknown
  origin after landing, and knobas opens the browser there.

## Decided by Fable while Björn was busy

1. **Passwords-role sites only**, keyed on the role; every site would break Gravatar, CDN scripts, Atlassian's CDN and
   SSO pages for a rule nobody asked for.
2. **Allowed origins count, and the rule depends on whose page is on top**, as above. Top-level navigation is
   unchanged. If a platform could not tell the two page kinds apart, the build was to stop and come back. The ruling
   Björn is most likely to reverse: the strict alternative is the own origin only, which stops all contact from password
   pages even with an identity provider allowed, and costs any web client that needs a second address, with no way for
   the person to fix it.
3. **Fail closed**, with a fixed sentence and *Retry*, also when the allow-list changes on a live site (the new rules
   go in before the navigation continues).
4. **Nothing in the bar**; one sentence in Settings; log lines as above.
5. Exact origins, `ws(s)` as `http(s)`, `data:`/`blob:`/`about:` through, per-hop redirects, measure what the platform
   blocking does not see, no vendor host list, a proof that never contacts Pleasant, one pure function, no new
   `SiteFailure` variant, docs in ADR-0050, CONTEXT and this note.
6. **A gap this build leaves open, for Björn:** after landing, a script on the page can still send the top-level
   address to an unknown origin, and knobas then opens the browser there (ADR-0050's line 5). That is a navigation, not
   a load, and the browser is outside knobas.
