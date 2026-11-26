# 2026-11-09: Pages show what knobas last read, and read the source behind it

**Asked (Björn, on PR #2479, after the forge pass):** "see if that problem also persists on other sources like docs, tickets etc." The rule he set: "I want the app to be smooth at all times", especially with a very slow source.

**Found.** Tickets, Docs, Monitors, the Inbox, the launcher, Standup, Time and the asset tree read the mirror and never wait on a source (0 to 84 ms at every speed, the source down or hung included). What waited was every page that draws a source's live read: TeamCity's run (its overview 3 s at a 1 s source, 15 s at 5 s, 45 s and an error when hung; its log, tests, artifacts, chain and *Run…*'s parameters), a ticket's attachment preview, the grants every write asks, and on the forges a pull request, its diff, a commit, a branch's standing, the branch list and a session's first *Commits*. ADR-0032 kept those reads in memory only, so every restart made each page wait again.

**Taken (ADR-0042).** A page answers from what knobas last read (memory, then `sync.held_read` from an earlier session, then for a pull request the mirror's own record), says from when, and reads the source behind it; a source that does not answer is named over what stays drawn. A finished run's reads and an attachment's bytes are fixed and held. Jira's, Confluence's and TeamCity's grants move, as Gitea's did. A commit's page, Compare and a pull request's *Files* draw what the mirror and the clone hold when the source does not answer.

**The changed rule.** ADR-0032's *never in the database or on disk* becomes *never in the mirror's tables*: a page's reads are held in `sync.held_read`, bounded, keyed by what the source was built from, and dropped with a new credential or the source.

**Reversing it** drops the table (a migration), and `entity_view` answers from the source again on every first read of a session.
