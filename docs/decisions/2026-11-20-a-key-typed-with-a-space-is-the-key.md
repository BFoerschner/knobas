# 2026-11-20: A ticket key typed with a space, or with none, is the key

**Asked (Björn, on PR #2479, through the orchestrator, ux25):** "I've noticed that typing a ticket key and id with
space instead of - doesn't match. I want to be able to type like pay 123 and it shows up pay-123..."

## Why it missed

A key is a word to PostgreSQL: `PAY-123` lexes to `pay-123`, `pay` and `123`, and the ticket's key is in its name
at weight A, so `PAY-123` finds the ticket first. `pay 123` asks for `pay` and a word that begins `123`. On a
mirror where PAY is a project, thousands of rows hold `pay`, so the statement for a word many rows hold answers
(`broad_sql`), and the ticket is one row among many, not first. `pay123`, `pay_123` and `pay.123` are one word
nobody wrote, and find nothing. The launcher's own front end listed tickets first only for text shaped like a
key (`ticketsFirst`), which `pay 123` is not. The Tickets view's word filter matched `pay 123` (both words are in
the key `pay-123`) and nothing for `pay123`.

## Decided

1. **The rule** (`knobas_search::typed_key`). A project word followed by a word of digits (`pay 123`), or one
   word that is a project and digits with no separator, `_` or `.` between them (`pay123`, `pay_123`,
   `pay.123`), reads as `PROJECT-number`. Any case. Only where the project is one the mirror's tickets are
   numbered under, so `top 10` and `java 21` stay words unless TOP or JAVA is a project. A project that begins
   another reads as itself: `payout 12` is PAYOUT-12, `pay 12` is PAY-12. Where two projects could begin one
   word (`AB` and `AB1` for `ab1123`), the longer. `pay-123` is a key already and is left as typed. The number is
   never fuzzed: `pay 132` reads `PAY-132`.
2. **The projects** are read off the ticket ids the picker already holds in memory (ADR-0045), at each reload,
   so they cost no read: 39,770 ids are one pass in the load. A searcher hands them to each query through its
   vocabulary (`Vocabulary::projects`). A searcher without a picker reads no key.
3. **An addition, never a replacement.** The launcher asks the text read as the key beside the words as typed,
   on a connection of its own, and puts the key's rows first in each kind (`group::keyed_first`), then the
   words' rows; a kind's total is the larger of the two. A word still being typed is asked as a prefix, so
   `pay 12` lists the keys `PAY-12` begins, as `PAY-12` does. Where the key's answer holds a row, no word is
   corrected (D10's rule: a correction is only for text that found nothing).
4. **The echo.** `ParsedQuery.keyed` (optional, left out when nothing was read) carries the text read as the
   key, and the launcher lists the tickets first for it as it does for a key typed with its `-`.
5. **Every box where a key is typed.** The launcher, and through the same `search` command every picker that
   asks it (*Link to*, the block and timer pickers, *Link a ticket*, the publish target, the assets view); the
   Docs search (the pages that name the key first); the Tickets view's filter and the boards' (`mini_board`,
   both the room's board and a saved view's), whose word filter admits the key's alternative beside the
   words', so `pay123` admits what `PAY-123` admits and `pay 123` what it admitted before. The `mini_board`
   command hands the board its picker (`Room::picker`, `saved_view_board_paged`'s new last argument).
6. **The help card** has a row: `pay 123`, "a ticket key with a space or none — pay 123, pay123 find PAY-123".
   It only says how to type something, so clicking it keeps the box.

## What it costs

No new read and no new statement: the key is asked with the statements the launcher already has, so the
no-worker rule and the plans the probes hold are unchanged. A query with no key read asks nothing more. A
query with one asks one more statement, beside the others, on its own pooled connection. The projects are a
sorted list of lowered keys, a few kilobytes on any mirror.

## Left

- A saved view's stored text is counted as before (`ticket_views::list` builds its searcher without the picker):
  a view whose text is `pay123` counts nothing while its board draws the key.
- The Tickets view marks the words typed in a card, read in the browser; `pay123` marks nothing in `PAY-123`.
- Until the picker's first load after a start, no key is read.

## Reversing it

`Searcher::vocabulary` not copying the picker's projects is the search as it was.
