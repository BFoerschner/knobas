# 2026-11-13: A nav entry comes back to where you were

**Asked (Björn, on v3.2.9-ux9, Windows):** "when i select a page in docs, then go somewhere else and then return to docs
my selected page isn't shown but the starting page again."

## Why Docs forgot the page

Nothing dropped the page. Docs is one kept page for every Docs address, and it was still mounted and hidden, showing the
page. The Docs entry in the nav did not link to it: it linked to Docs' own page (`#/docs`, or the last search), by a
ruling of 2026-11-07 (*Section memory*: "a place deeper than a tab is not what an entry comes back to"). So a press on
the entry sent the kept Docs page to its home, and the page you had chosen was gone from the screen. The same rule sent
every section's entry to its own page: a ticket, a pull request, a repository, a run, a marked monitor, a link set and
a source's page in Settings were all forgotten the same way. Time was not: its day, week and standup are its own
page's tabs, which the entry already kept.

Reproduced in headless Chrome on ux9 (`1f84766e`) and on the head before this (`b44946f6`), on the fixture and on the
windowless backend over the saved scale database: a page chosen on Docs' home, Inbox, then the Docs entry by pointer or
by `Enter` lands on `#/docs`. The launcher's *Docs* row lands there too, by design (an opening of the section by
name). Back (`⌘[`) kept the page, as history does.

## Taken

- **The entry comes back to the section's place** (`TfShell.svelte`, `sectionPlaces`; CONTEXT.md *Section's place*).
  Every address a reader stands on is the place of its section, unless it is a card row's narrowing, home (`#/job`,
  which lights Tickets), a tab the page does not have, or on Builds a pipeline it does not hold. From another section
  the entry, the gear and `⌘,` link to it. The kept page is shown again as it was, scroll, folds and open sections
  included, with nothing moving; a page keep-alive dropped mounts again from the answer cache and takes its scroll back
  as any return does (`places`, `restorePlace`).
- **Pressed inside the section, the entry goes to the section's own page**, as section memory keeps it (its tab, saved
  view, pipeline or search); on that page a press moves nothing, as before (F-077). So a second press is the way to
  the start, and the start keeps its tab.
- **A double press is one press.** After a person's press brought a place back, a second press of the same entry
  within the press's hold (`PRESS_HOLD_MS`, 2 s), by pointer or by `Enter`, does nothing; otherwise a double click
  would land on the section's own page, which looks like the bug. The hold ends once the address moves by anything
  else. A script's press is no person's and goes through, as the press guard lets it.
- **Kept as it was:** the launcher's section rows open the section's own page (a row naming a place by its name is an
  opening, as the spec has it); a row in a section's own page still brings back the run, pull request or repository
  left by a press inside the section (F-371); Back steps through history; `Esc` on a place come back to goes up to its
  parent, never back to the section the entry was pressed in.

Nothing drawn changed: the entries look as they did, so no screen's pixels moved. The entries carry `data-section`,
which the speed checks press by now that an entry's link names a page of any kind.

## Caught

`TfShell.test.svelte.ts`: one test per section, each failing before this (Docs' page and space, a ticket, a pull
request's tab, a repository's tab, a run's tab, a marked monitor, a link set, a source's page; Time's row passes
before and after, its week being a tab). The press trips' `nav-place` kind (`testenv/ui-checks/trips.mjs`): each place
left for Inbox and come back to by its entry twice with a still pointer and twice by `Enter`, as the same kept page
at its scroll on one history entry, then the section's own page once the hold has let go; Docs' page out of
keep-alive; a Docs comment's toast and a monitor's *Hide* toast standing through the leave and the return. All twelve
fail on the head before this. The trips that encoded the old rule (`nav-reopen-*`, the deeper `nav-return-*`, the
gear's source page, Docs' folds) now press the entry twice where they mean the section's own page. The open budget
gained `docs-page-back`, a Docs page come back to by its entry on every round.

## Reversing it

`navHref` in `TfShell.svelte` back to `sectionTabs[section] ?? sectionHash(section)` and `sectionPlaces` removed; the
hold then never holds anything, since a press from another section only ever lands on a section's own page. The spec's
*A nav entry comes back to where you were* goes, and the three entries it amends read as before.
