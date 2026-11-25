# 2026-11-25: A Docs page opens at once, and a site left with Esc twice lands where it is

**Asked (Björn, 2026-11-25):** the D1 checker saw the old Docs page stay drawn until the new one's read arrived; Björn:
"Yes", fix it. And: "also I want it to exit to the current page of the web version if I browsed inside knobas then go
back with double esc".

Ruled by Fable (xhigh) while Björn was away; built by Opus on `docs-nav-fix`.

## 1. A Docs page opened from another

**Before.** Docs is one mounted page for every `#/page/<id>`. On a new address it asked `docs_page` and kept the page
it had drawn until the answer came: the old page's crumb, title, body and comments stood under the new address, its
rail and its `Esc`. Only a first open showed a skeleton.

**Now (`TfDocs.svelte`).**

- **In the press's own task** the old page goes. The new page takes its place as the answer cache keeps it
  (`keptAnswer`, `ipc/answer-cache.ts`, asking nothing), so a page read before never shows a wait. Otherwise its
  frame takes its place.
- **The frame is the page's own markup** drawn from what the Docs tree holds: the crumb (Docs › space › parent ›
  title) and the title in their places, the actions as the page's would be, unseen and inert, holding their room, the
  meta line one line high without its rule (labels can wrap it), the outline column held empty, and bars for the body.
  The `h1` is the same node before and after the read, so focus on it stays. A page the tree does not hold yet has
  only *Docs* in its crumb, a bar one title line high, and no actions box.
- **The first open draws the same frame**, so one page has one waiting state.
- **The find bar stays over a frame**, so the title does not step up and back.
- **The page's reads go with the press** (`docs/press-reads.ts`): a primary press or `Enter` on any `#/page/` link,
  anywhere in the app, asks `docs_page` and `entity_head`, once a page per 3 s, no visit. Inside Docs a press on a page
  the tree does not hold asks the way to it too, so its crumb and title come with it.
- **A visit to a site no longer empties the answer cache.** `site_open`, `site_show`, `site_hide`, `site_do`,
  `site_answer`, `site_close` and `resolve_url` change nothing the mirror answers. Unknown to the cache, each one dropped
  every kept answer, so every page opened after a site read the backend again.
- **The skeleton bars hold still under reduced motion**, app-wide.

## 2. Esc twice from a site browsed inside knobas

**Before.** The second `Esc` (and *Back to ⟨name⟩* in the site's bar) went to the page that opened the site, wherever
the site had been browsed since.

**Now.** One rung before the opener, in `escTarget` (`escape.ts`), so the key, the native key, the bar's link, its
label and `site_show`'s `esc` flag cannot disagree:

- **Where the site stands now names an item knobas holds** (`resolve_url`, a read of the local mirror, by item and not
  by address, since a redirect moves the address of the same page), and that item is neither the one the site was
  opened at (the address `openSiteAt` pointed it at) nor the opener's own page: the second `Esc` lands on that item's
  page. That holds for whatever opened the site and for any kind with a page in knobas (a Jira issue lands on its
  ticket page).
- **Otherwise** it goes to the opener, as before, with no note: the bar's *Back to ⟨name⟩* already names where it goes.
  An address the mirror does not hold (a space's home, a search, an edit or history view, a page not synced) is
  otherwise.
- **The landed page takes the site's history entry in place and keeps its stamp** (`landFromSite`, a told kind of its
  own, `opener.ts`): history reads *opener, page*. The page's `Esc` and `⌘[` go to the opener, and `⌘]` from there
  comes back. A push would make the site the page's opener, so `Esc` would loop.
- **The bar names the landing** (*Back to Retry budget*): the Docs tree's title, else a list's row, else the row's own
  read (`entity_head`), a ticket by its key, and the kind's name (*Page*) until one is known. Its click does what the
  key does. *Open in knobas* is not drawn while it would go to the same page.
- **One `Esc` still only arms**, and starts the landing page's reads. A second `Esc` while the mirror is still asked
  about the address waits for its answer, at most 200 ms, the first cap held down.
- **Leaving refreshes the landed page first** of the five items the site stood on.
- **Unchanged:** a nav entry, the launcher, the shell's Back, `⌘[` inside the site, × and `⌘W`.

## Where the mirror cannot say

Confluence declares its page addresses (`viewpage.action?pageId=`, `/display/<space>/<title>`, the tiny link) besides
the stored `/spaces/<KEY>/pages/<id>/<title>`, so a page reached by its tree or a link lands. XWiki declares none: only
its stored `/bin/view/…/` address resolves (a fragment and a trailing slash are fine; `?viewer=comments` is not). An
address that does not resolve lands on the opener.

## Checked

- Tests failing first: the frame (title node kept, held meta, bars, inert actions), a kept answer drawn in the press's
  task with no frame, a page the tree lacks, the landing rung, the landing in the shell (history length kept, the
  opener after it), the site's landing and its refresh order, the cache's quiet site commands and `keptAnswer`.
- `moves.mjs` has `docs-page-next` (a page opened from another with its read 400 ms late); `open.mjs` has
  `docs-page-next` on the scale database.
