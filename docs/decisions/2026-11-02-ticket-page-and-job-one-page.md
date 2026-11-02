# The ticket page and the job are one page, and its section rows link, 2026-11-02

Björn's ruling in spec #2396 (*One page for a ticket and the job*), settled with him on
2026-11-01 (`mockups/ticket-first/ticket-page.md`, *Settled with Björn*) and recorded here by the
#2407 implementer as the ticket asks. It is his ruling, not an agent's, and it binds as such.

## The rulings

**One page.** `#/ticket/<entity id>` draws the page for a ticket and the job alike. The card is
on a sheet while the ticket is only read and on the yellow once it is the chosen job, and that
is the only difference. `#/job/<entity id>` chooses the ticket and lands on its page, so links,
bookmarks, the timer chip and *Start work* keep working; a bare `#/job` draws the chosen job's
page, or the empty state with none chosen. The Job nav entry goes, and Tickets lights for every
ticket page. Reading still never chooses a job: only the card's foot (*Start work*, *Resume
work*, *Work on this*) or a `#/job/…` address does. This supersedes the two-page split of spec
#2227 (#2332), which drew the ticket page as a page apart from the job. Spec #2227's other rules
stand: reading never chooses, and `Esc` goes up one level (from the page to its opener, else
Tickets; at home, nowhere).

**The section rows link.** The October ruling that the ticket page's *Around this ticket* lines
are counts, not links (CONTEXT.md's **Ticket page** entry before #2407; #1030 for the Tickets
preview) is reversed for the page. Each row of the card's sections table opens that section's
view on *This job* for this ticket, and a *Sub-tasks* row opens Tickets on `under:<key>`. The
Tickets preview keeps its count lines; aligning it with the card is a later ticket.

## Why

The two pages looked almost the same and differed in the wrong things. To see which build failed
on a ticket being read, the reader pressed *Open job*, which quietly made that ticket the job.
On one page the reader sees the failing build without choosing the ticket, and the card's colour
says whether this is the ticket being worked on.

The counts-not-links ruling existed because opening a section meant opening the job, and opening
the job chose it. The section views now take a ticket scope without choosing a job
(`tf/job/scoped.svelte.ts`, #2401): the row holds its ticket for that section while the section
is on screen and forgets it on leaving. A row can link without choosing the ticket, so the
ruling has no reason left.

## Cost of reversing

Splitting the page again means a second page component, a second address in the router, the Job
nav entry and its `Esc` rung, and the shell tests that drive both moods on one page
(`shell/dev/ticket-page`, the card and reading-column suites). Making the rows counts again is
one `href` per row and the scoped-ticket module (`scoped.svelte.ts`) with its shell wiring.
