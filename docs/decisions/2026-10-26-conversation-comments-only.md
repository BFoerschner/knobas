# The conversation is the ticket's comments, 2026-10-26

Björn's ruling in spec #2163 (*Jira sync: drop the changelog, make the conversation
comments-only, complete worklogs concurrently*), recorded here by the #2164 implementer as the
ticket asks. It is his ruling, not an agent's, and it binds as such.

## The ruling

**The conversation is the ticket's comments.** The Conversation tab and `entity_thread` carry the
ticket's comments and nothing else. The *Include changes* chip goes, and with it every change
line: status moves from Jira's changelog, status moves knobas sent, and the opening and merge of
each confirm-linked pull request. *Mentions me*, *New since your last visit*, the comment order
and the job card's summary line stay as they were.

Requirements R-5.1 and R-5.2 (`mockups/ticket-first/requirements.md`) now read so. This
supersedes R-5.2's *Include changes* (on by default) and the change lines #819 and #929 built;
the §10.8 entry for #2164 records the narrowed answer.

## Why

The changelog had one reader, the status-move lines, and asking Jira Data Center's `/search` for
it is what made the sync slow and, on tickets with long histories, over the transport's 32 MiB
response cap (#2163's diagnosis: a 10-ticket page of 54 MB with the changelog, 237 KB without).
The reader does not want change lines in the conversation at all, pull-request lines included, so
the conversation drops them rather than keeping them on another read.

## Cost of reversing

A status-move or pull-request line in the conversation comes back as its own spec (#2163 names
fetching history on demand as out of scope, to be specced later): a new field on `Thread`, its
reads, the chip and the renderer, and a §10.8 entry. The Jira changelog is no longer stored
(#2165, #2166), so status moves made in Jira would need a read of their own.
