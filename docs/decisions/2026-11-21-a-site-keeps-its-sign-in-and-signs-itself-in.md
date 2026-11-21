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

- **A browser-session cookie is never written to disk** by WebKit or WebView2, so it ended with knobas. On the Mac,
  after a sign-in to Gitea in a site, the site's store (`~/Library/WebKit/<app>/WebsiteDataStore/<id>/Cookies`) held
  `gitea_incredible` alone, written within a second of the sign-in; `i_like_gitea` was never there.
- **A Jira restart ends its sessions on the server too** (Björn's hypothesis, measured): after `docker restart
  knobas-jira`, `JSESSIONID` answered 401 with or without "remember me", and `seraph.rememberme.cookie` alone answered
  200. So the cookie that lasts across both restarts is the one the product sets for "remember me".
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
- **External adapters** declare the same `web_sign_in` in their descriptor's JSON (the WIT passes the descriptor as
  JSON, so no WIT change); one that declares nothing gets *Fill sign-in* alone.
- **Quitting** drops every view and waits up to 3 s for each site's browser process to end on Windows, when Chromium
  writes its cookie store.
- **Found on the way (macOS 27)**: a site's view covered its own bar. The window's content view runs under the title
  bar (900 points for an 868-point page 32 points down), and the view was placed against the content view. It is now
  placed against the page (`mac::under_title`, the content view's height less `contentLayoutRect`'s).

## Tested

Rust: the envelope (v2 reads, v3 round-trips byte for byte, a v3 only with a sign-in, Debug redacts), the descriptor's
`web_sign_in::check` and battery clause 15, `sign_in.rs`'s origin refusals and one-try rule, the controller on the
fake host (filled once per landing, refused off the own origin, in a redirect and with nothing saved, Kuma's form
looked for while it draws), and a log capture through a whole fill with no line holding the password. Mutants: fill on
any allowed origin, fill twice per landing; each fails its test. Front end: the fill script against each built-in
kind's recorded sign-in page (`crates/knobas-site/tests/sign-in-pages/`) and the tricky ones, the Settings row and
dialog, the bar's button and notes.
