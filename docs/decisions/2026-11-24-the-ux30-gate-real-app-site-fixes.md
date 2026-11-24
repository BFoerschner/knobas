# The ux30 end gate's real-app sites, part 2: two fixes (2026-11-24)

Part 2 of the ux30 end gate's real-app paths ran the in-app site's saved sign-in (XWiki through its 1.7.0 adapter,
Confluence Data Center) and Pleasant's web interface in knobas's WebView2 window on Björn's PC (a release build of
`ux-polish` at 69884496 in ux29's unpacked MSI layout, scratch profile, the in-memory secret store, presses as DOM
clicks over CDP after the idle check) and XWiki in the Mac window. Two things in the page's site handling were wrong;
each is fixed with tests that failed before.

## Settings said "Not signed in here yet" right after the site signed in

The native side records *Kept since* when a site's first page on its own address commits (`landed`). The page's site
store read the settings once at start and again only after a write in Settings or a new source, so in the run where a
site first signed in, Settings › Sources › ⟨source⟩ › Site › *Sign-in* still said "Not signed in here yet" and offered
no *Forget…*; it said "Kept since Nov 23" only after a restart (seen on Windows with Pleasant's site, 23:51).

**Now:** the store marks a setting kept from now when a site's status first reports a page on its own address and
none was kept, which is what the native side writes at that commit. A kept date stays as it was; after *Forget…* the
next page on the own address marks it again. `site.test.ts`.

## With two sources on one server, a page's *Open in ⟨source⟩* opened the other's site

Docs' *Open in ⟨source⟩*, *Open this part in ⟨source⟩* and links in a page, and a ticket card's *Open in ⟨source⟩*,
handed the address to the opening without the source they were pressed on. `webPlace` breaks a tie between two
sources on the same base in favour of the press's source (the Passwords fix of 2026-11-22), but only when it is told
the source, so the first source in the list won: on Windows, *Open in Confluence two* on Confluence two's page opened
Confluence's site, signed in as Confluence's account. (The ticket card's chip already named the right source while
the opening went to the other.)

**Now:** both hand the opening the page's or the ticket's own source. `TfDocs.test.svelte.ts` (*Open in* and *Open
this part in* name `source`), `TfJob.test.svelte.ts` (two Jira sources on one server).

### A removed source's site still opened

In the same run, after Confluence was removed, the same press made a site for the removed source (`site made
source="confluence"`): the store kept a removed source's setting until the next start, and an address on its server
still routed to it. The shell read the settings again when the source list gained a source; it now does when the
list gains or loses one (`SiteStore.differs`, which replaces `lacks`). `site.test.ts`.

## The rest of the run, all passing

- **Windows, Passwords against the real Pleasant 9.5.4 VM** (through a front of the agent's own on WSL's loopback,
  plain HTTP to Windows): the writer and the reader added through *Add a source* (*Connected as tomasz.wojcik ·
  Password Server 9.5.4.0 · 808 ms*); *Sync now*, 20 entries in 30 folders; the filter (`psp` 3, `ebics` 1); *Show* on
  Payments/Prod/Databases asked for a usage comment and showed the password, read under tomasz.wojcik with the
  comment; *Show* on PSP asked none (the ux35 fix); PSP's one-time code matched the server's secret; *wg-hub.conf*
  (216 B) opened in the viewer; *Edit* cleared *Notes* (PATCH 204) and put it back exactly, every field equal to the
  seed's but `Modified`; after a restart the source and its 20 entries stayed (the in-memory store loses the
  credential by design); the reader's PSP entries said "You may not read this entry's password" with *Show* and *Copy*
  disabled and nothing sent; both sources removed, "No password source yet". *Copy* was not pressed on the PC.
- **Windows, Pleasant's web interface:** *Open web interface* filled `/SignIn/Direct` by itself (`RememberMe=true`)
  and the web client opened as mara.lindqvist; the probe on every page (frame, script, style, image, fetch, XHR,
  beacon, ping, prefetch, preconnect, WebSocket, EventSource, a worker's fetch, own-origin redirects, and WebRTC:
  STUN, TURN over UDP, TCP and TLS under both policies, a TURN server by name, direct UDP and ICE-TCP checks) reached
  **nothing**: no request, connection or datagram at the listeners on loopback and on the WSL box's address; TURN
  answered 701 and the direct checks ended `disconnected`; Windows' DNS cache held no probe name. The site's browser
  process ran with the confine arguments of `confine::webview2_args` (the CDP switch merged beside them). Pleasant's
  own client tried an XHR to `downloads.pleasantsolutions.com`; it was refused and logged. A landing redirected to
  another loopback origin asked in the bar and sent it nothing; *Continue here* remade the view in 0.42 s and that
  origin's page loaded; the vault page's loads then reached that origin only; *Remove* in Settings remade it in 0.37 s
  and it got nothing again. After a restart the site opened signed in at a folder (`/WebClient/Main?itemId=…`, one 302,
  no sign-in POST).
- **Saved sign-in, XWiki** (Mac window and Windows window) **and Confluence** (Windows): saved; filled on the declared
  page by itself with remember me (`j_rememberme=true`; Confluence's JSON `rememberMe=true`); after a quit and a new
  start the page opened signed in with nothing filled; a wrong password got one try and "⟨source⟩ did not take the
  saved sign-in"; *Remove* went back to *None*. No log line holds a password.
