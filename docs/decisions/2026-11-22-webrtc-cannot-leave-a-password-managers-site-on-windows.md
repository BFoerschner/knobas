# WebRTC cannot leave a password manager's site on Windows (2026-11-22)

The ux30 release blocker left by `2026-11-22-a-password-managers-site-loads-nothing-from-other-websites.md`: on Windows,
`WebResourceRequested` sees no WebRTC traffic, so a page on a password source's site could still send packets and open
connections to any host it named. Björn was away; Fable ruled (one ruling, xhigh), listed at the end. ADR-0050 is
amended.

## What is built

- **A confined site's WebView2 gets its own browser arguments** (`knobas_site::confine::webview2_args`, one pure
  function, unit-tested). Each site already has its own session folder, so its own browser process, so the arguments
  cost other sites nothing. An unconfined site's arguments are unchanged.
  - `--webrtc-ip-handling-policy=disable_non_proxied_udp`, and the older `--force-webrtc-ip-handling-policy` spelling
    beside it: no UDP at all, so no STUN, no TURN over UDP and no UDP ICE check.
  - `--proxy-server=127.0.0.1:9` (`CLOSED_PROXY`, a loopback port nothing listens on) for every scheme, with
    `--proxy-bypass-list=<-loopback>;` then each of the site's origins as `http(s)://host:port` and `ws(s)://host:port`.
    WebRTC's TCP sockets (TURN over TCP and TLS, a page-supplied ICE-TCP candidate) take the proxy settings, so they go
    nowhere. `<-loopback>` comes first so a loopback address is not bypassed implicitly.
  - `--host-resolver-rules="MAP * ~NOTFOUND, EXCLUDE <each origin's host>"`: WebRTC looks a TURN or STUN server's name
    up itself, proxy or not, and the name carries what the page chose.
- **The Windows handlers ask `may_request`**, whoever's page is on top: an identity provider's page could load nothing
  more through the proxy anyway, and this way the log names what it asked for. The Mac keeps ruling 2 (an allowed
  origin's page loads as in a browser).
- **A changed allow-list remakes a live Windows view** (`SiteHost::remakes_to_confine`, `Sites::reconfine`), from
  Settings' *Allow* and *Remove* and from *Continue here* while landing: the view is dropped, its browser process
  waited for (at most 5 s, off the main thread; WebView2 refuses other arguments on a session folder whose browser
  still runs), and the view made again where it was, or at the landing. The sign-in stays on disk. A view that cannot be
  made again loads nothing and shows `NOT_CONFINED` with *Retry*. *Retry* (Reload) on a site with no view makes it again.

## Measured

**Headless Chrome 156 on the WSL box** (a page on 127.0.0.1, every route aimed at listeners on the box's own other IP),
to choose the switches:

| Arguments | STUN, TURN-UDP, UDP ICE | TURN-TCP, TURNS, ICE-TCP | TURN name looked up |
|---|---|---|---|
| none | arrive | arrive | yes |
| `--force-webrtc-ip-handling-policy=disable_non_proxied_udp` | arrive (ignored by 156) | arrive | |
| `--webrtc-ip-handling-policy=disable_non_proxied_udp` | none | **arrive** | |
| `--disable-blink-features=RTCPeerConnection`, CSP `webrtc 'block'` | arrive (no effect) | arrive | |
| policy + `--proxy-server=127.0.0.1:9` + bypass list | none | none (error 701) | **yes**, 2 DNS transactions (net log) |
| the above + `--host-resolver-rules` | none | none | no |

**knobas's own WebView2 window on Björn's PC** (Edge WebView2 157; debug builds cross-compiled with `cargo xwin` into
ux29's unpacked MSI layout; scratch profiles; started hidden; every press a UI Automation `Invoke` by name, each after
`GetLastInputInfo` showed Björn idle 5 minutes or the input was the press's own). A Pleasant-shaped stand-in on WSL
(`testenv/pleasant/standin.mjs` for the API, a scratch web client with `/SignIn/Direct`, the landing, `/WebClient/Main`
and a WebSocket) served as a Pleasant source on `http://127.0.0.1:28298`. Every page ran a probe, also inside a
`srcdoc` frame: STUN, TURN over UDP, TCP and TLS under both `iceTransportPolicy` values, a direct UDP and ICE-TCP
check to page-supplied candidates, and a TURN server by name, aimed at raw listeners on the WSL box's IP and on
loopback ports, waiting 33 s, longer than ICE's timeouts.

- **Before** (`95c9c0ba`): every route arrived. On the other IP: 18 STUN, 72 TURN-UDP and 238 UDP ICE datagrams, 4
  TURN-TCP, 4 TURNS (TLS ClientHello) and 2 ICE-TCP connections. On loopback: 4 TURN-TCP, 4 TURNS and 2 ICE-TCP
  connections. The `srcdoc` frame's too.
- **After:** **no datagram and no connection** at either listener, from the page or its `srcdoc` frame. WebRTC
  reported error 701 for every TURN and the direct checks ended `disconnected`. Windows' DNS client cache held no
  probe name. Still working: *Open web interface* filled the sign-in form (remember me ticked) and the web client
  opened; the page's own script, image, fetch and WebSocket loaded; a folder's *Open web interface* took one 302 through
  `/SignIn/Direct` to `/WebClient/Main?itemId=<the folder>`.
- **The allow-list:** a redirect to another origin while landing asked in the bar, and that origin logged no request
  (the refused navigation sent no GET). *Continue here* remade the view in 0.46 s, and the other origin's page
  loaded. The vault page's image and fetch from that origin then loaded. *Remove* in Settings remade it in 0.53 s, and
  the next vault page sent that origin nothing (`a confined site's request to another website refused` for its image
  and XHR).
- No browser opened; knobas's window was the only thing of the run on Björn's screen.

## Not defended against, and owed

- An Evergreen WebView2 can drop or rename a switch between releases, as Chrome 156 dropped the `--force-` spelling.
  **Each Windows pass reruns this measurement** in knobas's window (a probe page on a password source, listeners on
  another host and on loopback); if anything arrives, stop the release.
- On Windows an identity provider's page on top loads only the site's own and allowed origins; one that needs a CDN
  needs that origin allowed too. The Mac still lets such a page load as in a browser.
- A connection to the site's own origin (exact host and port) is not refused: that is the server the site is.

## Decided by Fable while Björn was away

1. **Ship the browser-level block (C), widened to every scheme**, and on Windows a confined site is strict whoever's
   page is on top. Not an injected script that freezes `RTCPeerConnection` away (it races, and `tests/it/scan.rs`
   refuses an initialization script on a site view). Not a knobas proxy of its own for ux30 (every allowed byte through
   a loopback listener, and knobas's loopback has been seen blocked on a no-admin PC).
2. **Both policy spellings**, a fixed closed port with no listener, `<-loopback>` first in the bypass list, `ws(s)`
   entries beside `http(s)`.
3. **A changed allow-list remakes a live Windows view**, waiting for the browser process; a failed remake shows
   `NOT_CONFINED` with *Retry*.
4. **Measure WebRTC's own name lookups**; they left, so `--host-resolver-rules` went in.
5. **Most likely reversed by Björn:** Windows being stricter than ruling 2 (Fable expects him to make the Mac match by
   dropping `unless-top-url`, rather than ask for a proxy of knobas's own); the remake reloading the site and losing
   page state; the fixed closed port.
