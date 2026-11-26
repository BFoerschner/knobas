# knobas link handoff (Firefox)

A Firefox extension that opens links to your knobas sources in knobas
(ADR-0061, #2534). Click a Jira ticket in chat and knobas shows the ticket.
The tab Firefox opened for it closes, and Firefox shows no `knobas://` prompt.
The extension talks to knobas through the Native Messaging host that knobas
installs. Everything it knows comes from knobas.

## What it does

- **Rules.** It asks knobas (`get_rules`) which sources have *Open matching
  links in knobas* on. It asks at startup, when a Firefox window gets focus,
  and on the first navigation after its copy is 10 s old.
- **Matching.** A top-level `GET` to an `http(s)` URL is checked against those
  rules: same scheme, host and port, a path under the source's base URL, and
  the longest base path wins. A URL no rule matches loads at once, and knobas
  hears nothing about it.
- **Handoff.** For a match it sends `handoff` and lets knobas decide.
  - `opened`: Firefox drops the load. A tab opened for this link alone closes
    (it showed nothing yet). A tab that already showed a page keeps it.
  - `bypass`, `ignore`, any error, no answer within 2.5 s, or no host: Firefox
    loads the page. Each request has a port to itself, because the host
    answers a port's requests in turn and knobas acts only within 1.5 s of
    the host passing one on: a request stuck behind a slow one could open in
    knobas after Firefox had loaded the page. *Known items only* versus *Whole source* is decided by
    knobas: a page it doesn't mirror comes back `ignore`.
- **No loops.** *Open in browser* in knobas records a one-shot bypass, and
  knobas answers `bypass` for it. For two minutes after that, the same tab
  keeps that source in Firefox, so a sign-in detour through another host and
  back stays put. Server redirects of a request already let through are not
  asked about again. A link clicked on a page of the same source stays in
  Firefox.
- **Pause.** Click the toolbar button to pause. While paused it intercepts
  nothing and sends no messages. The pause survives a restart. The button's
  tooltip says why links stay in Firefox (paused, knobas not running, host
  not installed).

## Install

1. In knobas: Settings › a source › *Open matching links in knobas*. Turn it
   on, then *Install host* on the *Firefox host* row. This writes
   `dev.knobas.desktop.json` where Firefox looks for it.
2. Build the package with `just extension-xpi`. You get
   `target/extension/knobas-link-handoff-<version>.xpi`, unsigned, and the same
   bytes for the same sources.
3. Load it in Firefox, one of three ways:
   - **For a session:** `about:debugging#/runtime/this-firefox` › *Load
     Temporary Add-on…* › pick `extension/firefox/manifest.json` or the `.xpi`.
     It stays until Firefox quits.
   - **Developer Edition or Nightly, unsigned:** set
     `xpinstall.signatures.required` to `false` in `about:config`, then open
     the `.xpi` (drag it onto Firefox, or *Install Add-on From File…* in
     `about:addons`).
   - **Release Firefox:** sign it as an unlisted add-on on
     addons.mozilla.org. Upload the `.xpi` under *Submit a New Add-on* › *On
     your own*, or run `web-ext sign --channel=unlisted --source-dir
     extension/firefox --ignore-files "test/**" build.py package.json
     README.md` with your AMO API key. AMO returns a signed `.xpi` that any
     Firefox installs. A new upload needs a higher `version` in
     `manifest.json`.

The add-on ID `link-handoff@knobas.dev` is the one knobas's host manifest
allows. Change it and Firefox refuses the connection.

## Layout and tests

| Path | What |
| --- | --- |
| `manifest.json` | Manifest V2, persistent background page, `webRequestBlocking` |
| `src/router.js` | Every decision: rules cache, matching, handoff, bypass, pause |
| `src/native.js` | Native Messaging ports to the host, one request in flight on each; a port that timed out is closed |
| `src/rules.js` | The URL-to-rule match |
| `src/toolbar.js` | The button's icon, badge and tooltip for a status |
| `src/background.js` | Wiring to `browser.*`, nothing else |
| `src/protocol.js` | Host name, add-on ID, protocol version, timeouts |
| `build.py` | The deterministic `.xpi` |
| `test/` | `just extension-test`: node's test runner, no `node_modules` |

The tests run in `just check`'s script half. They fake the `browser.*` APIs
and the host, and speak the message shapes of
`crates/knobas-app/src/link_handoff/protocol.rs`. One test reads that file and
fails if the host name, add-on ID or protocol version there changes without
this extension changing too.
