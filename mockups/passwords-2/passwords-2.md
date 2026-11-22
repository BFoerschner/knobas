# Passwords, second pass

Clickdummy, 2026-11-22. Open [index.html](index.html) as a file. It needs no server. The Passwords page is at `#/passwords`, and Settings › Sources is behind the gear. The yellow pill at the bottom left switches scenario and theme.

Everything in it is made up. The clipboard is a line in the status bar and never the real one, a read is a timer, and a reload starts over.

## The question

What does one Passwords page look like over two password managers that behave differently, when the page may not say which is which?

A Pleasant Password Server is slow, logs every password read under your name, can require a usage comment and can ask for approval. A Bitwarden or Vaultwarden vault is quick and does none of that. The page has to stay honest about what a press costs without naming either product.

## What the design assumes

The nine decisions in the brief stand as given. These are the choices the design adds on top of them.

**The list and the entry are one centred block.** The folder tree is a rail on the left, drawn with the Docs tree's classes. Beside it sit two panels on the paper, the folder's entries and the selected entry. The pair is capped at 1,438 px and centred in the space beside the tree. Below 900 px the tree is the Docs drawer. Below 700 px the list and the entry are one pane at a time.

**Thirty ticks carry the page.** A shown password covers itself after 30 seconds, the clipboard clears after 30 seconds, and a one-time code lasts 30 seconds by its nature. All three draw the same comb of thirty ticks and lose one a second. The code is set in the job key's type, Archivo at width 62 and weight 800, because it is the one value you read off the screen and type somewhere else. Nothing else on the page is loud. Yellow stays with the timer chip.

**A folder lists everything under it.** Its own entries come first, then one panel per subfolder that holds entries. A subfolder with more than six shows five and a row that opens it. The root also lists the four entries you used last. That list is knobas' own and reads nothing from a source.

**Every secret takes one path.** Offline and an open access request stop a read before it starts. A folder that requires a usage comment asks for it in the row, pre-filled, and Enter sends it. Then the adapter reads. After a read on a source with an audit log, the row says when it happened and that the source recorded it under your name. A source without an audit log gets no such line.

**Copying a password that is already shown reads nothing.** The value is in memory, so there is no second request and no second audit line.

**History is read when you open it.** It is a fold that starts closed. Opening it is a read, and the fold says so before you press. On Pleasant it lists who read and who changed the entry. On Bitwarden it lists earlier passwords, each covered.

**Edit reads no password.** Edit changes the title, username, address, notes, expiry and folder. Changing the password is its own dialog with the generator. That dialog says that saving stores the new password and does not change it at the system it belongs to.

**A password write is sent at once.** It never waits in the write queue. If the source does not answer, nothing is saved and the dialog says so. See open question 7.

**An approval is checked by hand.** The request and its state sit on the entry. knobas asks the source only when you press *Check now*. In the mockup the pill plays the approver.

**Pleasant's adapter declares "Only when I press Sync now" as its default.** A new Pleasant source therefore reads nothing until you press *Sync now*, and it lands on its Settings page saying so. A new Bitwarden source syncs every 15 minutes like any other.

**Signing in again does not sync a by-hand source.** Today a new credential triggers a sync. For a source set to sync by hand the toast says it was not synced.

**No report reads passwords.** The page has no strength, reuse or age report. Each would read every password, and on Pleasant each read is an audit line.

**The source's Settings page says what it offers.** A new section, *Passwords*, lists the eight optional abilities with Yes or Not offered. Pleasant's page also has the table of the four TOTP plugin formats and marks the one the sync found.

Four small changes to the app's own chrome come with it. Each is marked in `pw2.css` or in the page.

- The interval row's label changes from *Every* to *Syncs*, so the new choice reads as a sentence. The options read *Every 15 min* and *Only when I press Sync now*.
- An eleventh nav item pushes Time out of view at 1,100 px. Two pixels less padding a side on each item, between 901 and 1,240 px, brings it back.
- The Settings body is centred beside its nav. The app's sits at the left, which leaves a 2,000 px window empty on one side.
- A password source's credential row says *Change…* where other sources say *Re-enter…*.

## Against the first attempt

The first attempt in `mockups/passwords/` used a tree, a list rail and a detail stage. The entry has six fields, so it sat alone in a wide column. Here the list and the entry are composed as one block, and that block is centred.

I kept its rule that a folder shows what is under it, and the clipboard line in the status bar. Both were in its notes, which I read before drawing, and both still look right.

I dropped *Needs attention* and the numbered password cells. The KeePass file adapter is out because this pass covers two managers. Its usage comment was a dialog and is now a field in the row. Its TOTP ring is now the comb.

## Checked against the vendors' documentation

I had the public pages read once. Nothing was tried against a real server.

- Pleasant's REST API v5 needs Password Server 7.9.7 or later. The token comes from a POST to `/OAuth2/Token`.
- Entries carry `CustomUserFields` and `CustomApplicationFields`. The password has its own request, `GET /api/v5/rest/entries/{id}/password`. Where a comment is required the same path takes a POST with `{"Comment": "…"}`.
- The documentation's example of reading one entry by id returns the password in the entry. Its folder listings return `"Password": null`. The adapter should therefore list through the folders and never read an entry by id.
- A usage comment required on a folder applies to its subfolders. The API's `iscommentrequired` names changing, moving and deleting entries beside viewing a password, so the entry and password dialogs ask for a comment there too.
- Pleasant calls access requests *Request and Approval*. The requester writes a comment. The approver sets when the access ends.
- Bitwarden nests folders by naming them `Parent/Child`. Its API key does not replace the master password. It keeps an entry's last five passwords. Attachments need a paid plan or an organization.
- KeePass's built-in fields are `TimeOtp-Secret-Base32` with optional `TimeOtp-Length`, `TimeOtp-Period` and `TimeOtp-Algorithm`.

Three things the pages did not settle. Which API calls Pleasant writes to its audit log. Whether nested Bitwarden collections use the same `/` naming. Which KeePass version introduced the built-in fields.

## Open questions

Carried from the brief, not decided here.

1. Does Pleasant log reading an entry's custom fields as a view? With sync by hand it matters little, but check it once against a real server. The documentation does not say.
2. Entry titles in `knobas.entity.title` end up in backups and share exports. Keep password entries out of the entity table, or out of share exports?
3. Moving entries between sources would be a copy and a delete across two adapters. Allow it, or keep moves within one source? The mockup's Edit offers only the entry's own source.
4. Should knobas ever show a password, or only copy it? The mockup does both.
5. Access-request approvals in knobas' Inbox, or left with the password manager? The mockup leaves them there and checks only when asked. An Inbox item would need knobas to ask the source in the background, which decision 4 rules out for Pleasant.

Found while drawing it.

6. Do notes and custom fields sync into the mirror? The role's minimum names five fields and neither is among them. People keep recovery codes and PINs in both. The mockup draws notes from the mirror and covers a custom field the source marks as protected, reading it live like a password.
7. Can a password write go through the write queue at all? The queue is a Postgres table and is in the backup, so a queued password would be a password at rest. The mockup sends every password write at once and refuses it while the source is down. `CONTEXT.md` says a change goes through the write queue and a view never writes, so this needs a ruling.
8. A Bitwarden entry can sit in several collections. The role carries one folder path per entry. List the entry under each collection, or under the first?
9. Which reads does a usage comment cover? The mockup asks for an attachment as it does for a password, wherever the server says a comment is required.
10. Entries need a kind. `CONTEXT.md` lists none for a password entry, and the launcher will find entries once they are in the mirror. The mockup draws only the page's own filter.

## What was checked

Headless Chrome, as a `file://` page and once over `python3 -m http.server`.

- Every flow in the brief runs with no console errors or exceptions.
- At 2,400 px the bench has 341 px of paper on each side, the Settings body 651 and 659, and the empty state sits in the middle of the page both ways.
- At 1,440 and 1,100 px all three columns fit and Time stays in the nav.
- At 390 px no page scrolls sideways.
- Light and dark at all four widths.
