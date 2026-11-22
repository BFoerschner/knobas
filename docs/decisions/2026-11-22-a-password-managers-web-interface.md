# A password manager's web interface, opened from the Passwords page and signed in by itself (2026-11-22)

The ux30 pass, branch `ux30-pleasant-site`. Björn: "Pleasant has a web interface too, is there an open website button
similar to pages that automatically logs in aswell?", and then "if it's like a Bitwarden wasm plugin it should then
show the Bitwarden webpage, when that later gets implemented". ADR-0050 and ADR-0056 are amended. Björn was busy, so
Fable decided (two rulings, xhigh); they are listed at the end.

## What is built

- **For the role, not for Pleasant.** A source's descriptor may declare `web_links`: its web interface's front page
  (`home`) and, per entity kind, a template of an item's page with `{key}` (the entity's key, percent-encoded once),
  both after the typed base URL (`crates/knobas-source/src/web_links.rs`, battery clause 18). An address built from
  them can only name the source's own site. A password item still carries no `web_url`, so no generic *Open in
  ⟨source⟩* names the vendor anywhere.
- **External adapters too.** `knobas:adapter@1.7.0` reads `web_links` from a component's descriptor JSON and drops it
  from an older one, as it does `web_sign_in`. The WASM fixture declares a survey page and a front page, and the host
  test holds 1.7.0 to them and 1.6.0, 1.5.0 and 1.3.0 to none. A Bitwarden or Vaultwarden adapter declares
  `/#/vault?itemId={key}` and gets the same button. (The WASM host still refuses a component that claims the Passwords
  role, since `read_secret` has no WIT export; that is a separate decision.)
- **The button.** "Open web interface" on the open entry (last in its actions) and in the folder header (between *New
  entry* and *Sync now*); `W` acts on the open entry, else the folder; `Shift W`, a ⌘/Ctrl press or a middle press
  opens the browser. It opens the entry's page, else its folder's, else the front page (`password_web_link`, asked at
  the press). A source's top, and the page's top with one source, open the front page; with several sources the
  page's top has none. A base URL that is no web address gets none. The tooltip says what it opens, where (inside
  knobas and signed in, inside knobas, or in your browser), the account a new sign-in is recorded under where the
  source keeps an audit log, and that `Shift W` opens the browser. The label never names a source or a vendor.
- **No second password.** `web_sign_in.uses_credential` says the site signs in with the source's own username and
  password (declared paths only, never `any_page`). knobas fills that, under the same origin check as a saved
  sign-in, unless the person turned it off or saved another sign-in, which wins. Nothing new is stored: the keychain
  envelope is unchanged. Settings › Sources › ⟨source⟩ › Site › *Saved sign-in*: "Same as ⟨source⟩'s credential,
  ⟨username⟩" with *Use another…* and *Turn off*; "Off. You sign in yourself." with *Turn on* and *Use another…*;
  "Saved for ⟨username⟩, never shown" with *Change…* and *Remove* (back to the credential).
- **Inside by default.** A password source's *Open its pages inside knobas* reads on until it is set, so the button
  opens its site.

## Pleasant 9.5.4, measured on the real server (the WSL VM, 2026-11-22)

- **Sign-in:** `GET /SignIn/Direct?ReturnUrl=<path>` draws the form (`UserName`, `Password`, a `RememberMe` box),
  posting to `/Account/SignIn?ReturnUrl=<path>`. A refused password draws the form again at `/Account/SignIn` (200).
  Success: 302 to `/SignIn/Success?returnUrl=<path>`, then `<path>`. No second factor was set up on the free server;
  a code page is never filled anyway.
- **Signed out,** any web client address redirects to `/Account/SignIn?ReturnUrl=<path>`, a page with no form and only
  a *Connect* link to `/SignIn/Direct?ReturnUrl=<path>`. **Signed in,** both `/SignIn/Direct?ReturnUrl=<path>` and
  `/Account/SignIn?ReturnUrl=<path>` answer a 302 to `<path>`.
- **Deep links:** `/WebClient/Main?itemId=<GUID>` opens an entry (the tree expanded to its folder and its read-only
  dialog, behind Pleasant's own `ViewEntryPassword` comment check) or a folder (selected). **Opening an entry's
  dialog writes "Password Fetched"** under the account (measured in the window, below), so Pleasant declares no entry
  page: an entry opens its folder. It declares `/SignIn/Direct?ReturnUrl=%2FWebClient%2FMain%3FitemId%3D{key}` for
  folders and `/SignIn/Direct?ReturnUrl=%2FWebClient%2FMain` as home: a signed-out press lands on the form and fills
  by itself, a signed-in press is one 302. Sign-in paths: `/SignIn/Direct` and `/Account/SignIn` (where a refusal lands, so the
  one-try rule sees it). `ReturnUrl` counts from the server's root; an instance behind a path prefix was not measured.
- **Audit lines** (the admin's Logged Events, read before and after each step): the web sign-in writes **"Session Log
  On"** ("User <mara.lindqvist> logged on") under the account, not "Identity Verified" (the API token's line). Loading
  the web client, a folder link and a folder's listing write none. A request on a kept session, remembered or not,
  writes none.
- **"Remember me":** `.PleasantIdentity.ApplicationCookie` gets an `expires` 4 hours after the sign-in; without it the
  cookie has none and ends when knobas quits. So a restart within 4 hours opens signed in with no new line; after
  that, the next press writes one "Session Log On".

## In the real app (the Mac's window, 2026-11-22 15:47 to 15:51)

A debug build of this branch on a scratch `HOME` (the demo profile, credentials in its `dev-secrets.json`), its page
driven through the dev server's probe (`testenv/ui-checks/real-window/vite.probe.config.mjs`; presses as DOM clicks,
no synthetic input, window-only captures). Pleasant 9.5.4 in the WSL VM, reached through a TLS proxy of my own on the
WSL box with a leaf for `127.0.0.1` from the loopback-only test CA the Mac trusts (the VM's own certificate has no
subjectAltName). The source: `mara.lindqvist`, the read-only account, added with `knobas-app live-sources apply
--demo`.

1. *Sync now*: 20 entries in 31 folders; two "Identity Verified" lines (the credential test and the sync's token).
2. Passwords › Platform / CI / Service accounts › *TeamCity service account*: "Open web interface" with its `W` cap
   on the entry and without it in the folder header; tooltip "Opens this entry in the password manager's web
   interface, inside knobas and signed in. A new sign-in is recorded there under mara.lindqvist. Shift W opens it in
   your browser instead."
3. The press opened the site inside knobas. It landed on `/SignIn/Direct`, knobas filled the form by itself (`site
   sign-in answered mode=Fill answer=Filled`), and Pleasant went on to `/WebClient/Main?itemId=<entry>`, "Hello, Mara
   Lindqvist", with the entry's read-only dialog open. Audit: one **"Session Log On"**, then one **"Password
   Fetched"** for the dialog's load. So entries now open their folder (Fable's ruling 7).
4. Quit (SIGTERM, the app's own shutdown), start again, Passwords › Payments / Prod / PSP › the header's *Open web
   interface*: the site opened at the folder, signed in, with no fill asked and **no new audit line**.
5. With the change built: quit, start, *TeamCity service account* › *Open web interface* ("Opens this entry's
   folder…"): the site opened `Service accounts`, signed in, no new audit line.
6. Settings › Sources › Team passwords › Site: *Open its pages inside knobas* on (never set), *Sign-in* "Kept since
   Nov 22", *Saved sign-in* "Same as Team passwords's credential, mara.lindqvist" with *Use another…* and *Turn off*.
7. *Forget…* removed the site's store (WebKit keeps it under the real `~/Library/WebKit/knobas-app/WebsiteDataStore`,
   whatever `HOME` says); the scratch profile was deleted.

**Found: Pleasant's sign-in pages frame the vendor's news.** `/SignIn/Direct` and the `/Account/SignIn` landing hold
an `<iframe>` of `https://pleasantpasswords.com/product-news?FeedID=…&Version=9.5.4.0.Unregistered,3&ref=<install id>`.
ADR-0050's allow-list rules top-level navigations, not frames, so the site loads it as any browser does: in the run
above, each signed-out load of the sign-in page sent that one GET from the Mac to pleasantpasswords.com (Björn:
"never contact Pleasant", ruled about trial keys and sales). A remembered session skips the page. Whether a source's
site should refuse frames from other origins is open for Björn; nothing here changes it.

Not run: Windows (WebView2), owed; *Shift W* in the window, which would open the browser on Björn's Mac (the unit
tests hold it); Linux has no in-app site.

## Decided by Fable while Björn was busy

1. **A template in the descriptor, top-level `web_links`**, not a link on the item record: it can never point off the
   source's own origin, adds nothing to 2,000 rows or the mirror, keeps `web_url` None, works on rows already
   mirrored, and crosses the WIT as descriptor JSON. Not inside `Offers`, which a WASM component may not carry.
2. **The button follows *Open its pages inside knobas*, which reads on when never set for a password source.**
3. **One button, "Open web interface", key `W`, `Shift W` for the browser**, with the tooltips above; no second button.
4. **`uses_credential`, on by declaration,** with the three Settings states and the dialog title "Save another sign-in
   for ⟨source⟩".
5. **"Remember me" is ticked, as for every site** (the second ruling reversed the first, which had said never): the
   session is the site's own, it rests in the site's store outside the keychain for as long as the server allows (4
   hours on Pleasant 9.5.4), *Forget…* removes it, and it means fewer "Session Log On" lines. Only a press loads the
   site. The ruling Björn is most likely to reverse; turning it off is a `remember` false on the fill for a source
   that declares `uses_credential`.
6. **Pleasant's links go through `/SignIn/Direct`**, `web_links` is `{home, pages}`, and both sign-in paths are
   declared.
7. **The entry link stays only if opening it writes no line; otherwise entries open their folder.** It writes
   "Password Fetched", so Pleasant declares folder pages alone.
