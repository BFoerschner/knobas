# 2026-11-16: Two and three letters are answered by names, and a key waits for its answer

**Asked (Björn, on PR #2479, through the orchestrator):** the two things the one-letter pass left for a ruling
(`2026-11-15-one-letter-is-answered-by-names.md`, *Left*). Two letters were as slow as one had been, `pa` about 100 ms
of backend on the Mac, and the key targets need two and three letters too: 100, 100 and 200 ms from a key to its rows
at CPU 1x, 4x and 6x on his PC, whose PostgreSQL runs 1.5 to 2 times the Mac's. And the box said *Searching…* for a
frame or two before the first answer, with no ruling on what Enter does meanwhile. His standing rule: directness,
performance and responsiveness over data scarcity. Decided by Fable under his delegation, and carried out in the same
pass.

Two decisions, in two parts.

# Part one: two and three letters

## What two or three typed letters are for

They are still the start of a name. Somebody typing `pa` is on the way to `payout-service` or `PAY-231`, and so is
somebody at `pay`.

The ranking of words said as little about them as it said about one letter. `pa` matches every row holding a word
that begins with those letters, 25,886 of 39,770 on his scale, and `re` 28,778, and ranks each by how many such words
it holds. The first rows for `pa` were the tickets and pages that say *page*, *part*, *path* and *payout* most often.
That count is the whole cost: about 3 µs a match on one process, a third to fetch the row, a third to rank it, and it
depends on what was typed, so nobody could store it.

Three letters cost nearly the same (`con` 25,498 rows, 71 ms). At four a word is usually told from its neighbours, and
the count starts to say something about a row. So the line is drawn after the third letter.

## Decided

1. **Two and three typed letters or digits are answered as one is: by names.** A row answers when a word of its title
   or key begins with them, under *Titles* and under *Everything* alike. Those whose name begins with them come first
   (a title past its stop words, the text after the title's last `/`, a ticket's key, a pull request's number, its key
   cell), then the newest, the id last. Ten rows a kind, the page's forty dealt a row a kind at a time, and no match
   counted: a kind's `total` is the rows the answer holds of it. All of this is the one-letter answer, a letter longer.
2. **They are read as the ranking of words reads them.** The typed characters are lexed under `english`, and that
   lexeme is what the names are asked for. `ops` asks for `op`, as it always did, since `english` stems it. So the
   names two or three letters match are exactly the names *Titles* matched them by, and what changes is their order.
3. **A stop word answers as the start of a name.** `in`, `the`, `and`, `or`, `it`, `to` matched nothing, because
   `english` drops them. They are kept now, lower-cased, and name rows: `in` finds *Invoice renderer* and *Investigate
   the config loader*. This is the one-letter ruling for `s` and `a`, at two and three letters.
4. **Under *Everything*, few names are followed by the text.** Where the names fill no kind's ten rows and not the
   page, the text is asked as well, by the ranking of words as before, and its rows follow the names in the room they
   leave. No name gives way. A row both found is the name. A kind still holds ten, and the room is dealt a row a kind
   at a time, so a kind with no row yet takes its first before another takes its fourth. `tls` naming six tickets and
   a page is followed by the tickets and pages whose bodies say it. This keeps the one-letter ruling's word, that
   bodies join at the second character, wherever the names leave them room.
5. **Names that fill a kind are the answer.** Ten tickets named by the letters say the letters are common, and the
   text is not asked: it would rank every match to add rows below ten tickets nobody has read yet. A fourth letter or
   a space asks it.
6. **Characters no name holds are asked of the text under *Everything*, as before.** Its answer whole, with its count
   of every match, so *nothing matches* is never said of letters a body holds. Under *Titles* the names were the
   question and none is the answer.
7. **Everything else is untouched.** One character. Four characters and more. A finished word (`pa` and a space). Two
   or three characters that are not all letters and digits (`c#`, `a-b`, `-pa`), which the parser reads as more than
   one word. The same statement, the same answer, byte for byte.

## What was weighed

- **Names alone, the text only from the fourth letter.** The cheapest answer, and the brief's first suggestion. It
  hides the pages that say `tls` behind six tickets named by it, under a chip that says *Everything*. Ruling 4 costs
  nothing where names are many and one more read where they are few.
- **The text in whatever room the page has, not only where names are few.** More rows, and a trap on the commonest
  thing a person types, a project key. `scl` names ten tickets and six rows of one other kind, and its text matches
  21,880 rows: ranking them took 55 ms to add ten rows of a third kind below. Ruling 5 leaves those ten out until the
  fourth letter.
- **One row a word, read by range** (`word >= 'pa' and word < 'pb'`). 323,150 rows where the decided table holds
  792,042, and any length of prefix. But a range is not in the answer's order, so every name match is read and sorted
  for each key: 31,630 for `sc`, 15,809 for `re`. The stored order is what bounds the read.
- **One row a word and an index for each length**, the leads kept on the word. About 65 MB less. Three statements and
  three plans in place of one, and yesterday's table rebuilt. Not taken: his rule is speed over saved data, and the one
  statement and its one kept plan were already measured.
- **Words as written** (`simple`) for the stored characters, so `ops` names only what begins with `ops`. More literal
  at exactly three letters, and then a different set from the one *Titles* matches at four. One reading at every
  length was worth more.
- **The text's own first characters stored.** A body holds 80 to 250 distinct starts of two and three letters, so
  the table would hold about ten million rows on his scale.

What moves: nothing. One answer is drawn, the names', the names' with the text below them, or the text's alone. The
text is asked before anything of the answer is drawn, never after.

## How

**Migration `0094`.** `sync.item_initial` holds, for each mirror row, one row for each distinct first character,
first two and first three of the lexemes of its weight-A vector, with `leads`. So `initial = k` holds exactly where
`fts_a @@ 'k':*` does, for a `k` of one to three characters that begins with a letter or a digit. One function says
what a row's are (`sync.name_prefixes`), and its one-character rows are the ones `0093` wrote, checked on his scale:
253,585 rows, none differing. So those stay where they are. The two statement triggers of `0093` write through it. A
lexeme that begins with `-`, `+` or `/` (the number of a ticket's key, a path) gives no row of two or three
characters: the launcher asks those lengths of letters and digits alone. `0093`'s function is dropped.

**The fill takes the key, the foreign key and the index off first** and builds them once over the whole table after.
Written into them a row at a time the same rows took 7.3 s, most of it 539,457 foreign-key checks, and left the
indexes a third larger.

**The statement is the one-letter statement** (`knobas_search::sql::letter_sql`). What was typed is a bind and nothing
else of the statement depends on it, so one, two and three characters are one text and one kept plan a connection.
The word asked for is `coalesce` of the `english` lexeme and the `simple` one. For one character the two
configurations give the same letter, so that answer did not change.

**The merge is in Rust** (`knobas_search::group::few` and `filled`), over the rows of two statements: the names', and
for few names the ranking of words exactly as it ran before. So the rows the text adds are the rows it answered with
yesterday, in its order, less those the names hold.

## What it costs

- **Disk.** 539,457 rows more on his scale (267,306 of two characters, 272,151 of three), 13.6 a row beside the 6.4
  there were. The table goes from 57 MB with its indexes to 181 MB (73 MB of rows, 46 of key, 62 of index) in a
  database of about 900 MB.
- **The upgrade.** One pass at the first start: 3.4 s on that mirror, the Mac at a load of 9 (the rows 1.5 s, the
  keys 0.6 s, the index 1.3 s). Other runs read 2.7 s at a load of 7 and 4.9 at 22. Over the 3 s the brief asked for
  on a busy machine, and probably 5 to 6 s on his PC, once.
- **The sync.** A write of a mirror row whose name or date changed writes about twenty narrow rows where it wrote six.
  On the scale clones, median of five, the two builds alternated: a date moved on 1,000 tickets spent 117 ms of the
  statement's 960 in the trigger before and 386 of 1,282 after, a new title on 1,000 tickets 111 of 922 before and
  335 of 1,127 after, and a stamp of 10,000 rows 12 ms both ways. About 0.27 ms more for a row that changed, and
  nothing for one that did not.
- **A key no name holds** asks the names first and then the text as before: `du` 14 ms to 22, `bec` 21 to 28, and a
  key that finds nothing anywhere 7 to 13.
- **The contract.** `SearchResponse.total` and `ResultGroup.total` are the rows the answer holds for one, two or three
  typed characters that names answer. The generated mirror's two doc comments say so.

# Part two: between a key and its answer

## What was there

A key into the empty box changed the panel at once: the board went, *Searching…* stood in its place, and the rows
drew when the answer landed. With a read of 10 ms that is a line shown for a frame or two.

And every key acted on the rows that were drawn. `pay` and Enter typed at speed opened the first row of what `p` had
found, because `pay`'s answer was 90 ms of pause and a read away. Nobody had ruled on it.

## Decided

1. **The list is what it was until the answer to the box's text lands.** The board after the first key, the answer
   before while a word grows. No line stands where rows stood, and the list is never empty between two answers.
2. **A wait is said once it has lasted 400 ms.** That is the app's one threshold for a read (`PLACEHOLDER_MS`, #703).
   It counts from the last change of the box's text, so typing a long word at speed is no wait. Past it the rows kept
   are dimmed and out of the pointer's reach, as any replaced read is in this app (`tf-wait-dim`, #2246). A list that
   drew nothing has nothing to dim and says what it waits for, in the words it had: *Searching…*, *Reading the
   mirror…*, *Looking up the link…*. An answer that lands ends the wait unsaid.
3. **A key that acts on a row waits for the answer.** Enter, ↑, ↓, Tab and a chord bound to an action, pressed while
   the answer to the box's text is owed, are kept and pressed again when it lands, in the order they came. So `pay⏎`
   opens the first row of what `pay` finds, and `pay↓⏎` the second, however fast they were typed. What a row of keys
   opens does not depend on the machine.
4. **A waiting key asks at once.** It says the typing is done, so a query that waited for the keys to pause is sent
   without the pause.
5. **What lets them go.** A change of the box's text, Esc, a close, or any other key but a modifier. A search that
   failed opens nothing, and an answer with no row has nothing to open. There is no time limit: past 400 ms the list
   says it is waiting, and every other key is a way out.
6. **A click opens the row it was aimed at, at once.** The pointer is on something a person saw. Past the threshold
   the dimmed rows take no click.
7. **An opening starts with nothing of the opening before.** The empty box is the board at once. An opening on words,
   a section's filter, draws when its answer lands and nothing until then, where the answer left in the session was
   drawn for a frame.

## What was weighed for Enter

The brief named three.

- **The row drawn, the previous text's first.** What the box did. With ruling 1 it gets worse: after `p` the board is
  still drawn, so Enter would open the newest recent item for any word typed fast.
- **Nothing until the answer.** Safe, and a key lost. Whether the person must press again depends on how fast they
  typed, which they cannot see.
- **The answer when it lands.** Taken. The keys mean what they would have meant a moment later.

The arrows wait too, since `pay↓⏎` is one thought. A chord bound to an action runs on the selected row, so it waits
by the same rule; a list's chord applies its text whatever is selected and does not.

## What it looks like

It went through the frontend-design skill first, as anything visible does. The plan it came to draws nothing new: no
spinner, no bar, no skeleton, no new words. The list keeps its rows, which is the change a person sees most, and the
two things that can appear late are the app's own, the dim a replaced read gets and the three wait lines the box
already had.

## How

`launcher/session.svelte.ts` keeps the text its list answers beside the text the box holds. The panel and its rows
are read from the first, and `owed` says the second is ahead of it: a query waits for the keys to pause, or one is
out. `late` is the 400 ms, restarted by every change of the text. `Launcher.svelte` keeps the keys that wait and
presses them again from an effect on `owed`.

One trap, for whoever touches it next: `loadHome` reads whether a board is held, and the launcher calls it from the
effect that opens the box. Read tracked, the effect ran again when the board arrived, read the board again, and so on
for ever, taking the typed text with it each time. It is read `untrack`ed, with a comment.

# Before and after

On the Mac (M2 Pro, on AC power), release builds of the windowless backend on fresh APFS clones of the saved scale
home, the head before (`fc6b7939`) against this one, the two alternated press for press. Other agents' builds ran
throughout, at a load average of 7 to 9.

The backend's own time on the read (`took_ms`), median of nine, under *Everything*:

| Read | Before | After |
|---|---|---|
| `p` / `r` / `c` | 10 / 9 / 9 ms | 11 / 9 / 9 |
| `pa` / `re` / `co` / `de` | 76 / 103 / 75 / 63 | 9 / 9 / 9 / 9 |
| `st` / `le` / `ba` | 61 / 55 / 62 | 9 / 9 / 9 |
| `pay` / `ret` / `con` / `dep` | 64 / 64 / 71 / 61 | 9 / 9 / 9 / 10 |
| `sta` / `led` / `bat` | 53 / 52 / 53 | 9 / 9 / 10 |
| `ops` | 60 | 9 |
| `in` (a stop word, answered nothing) | 6 | 9 |
| `du` / `bec` (no name: the text, as before) | 14 / 21 | 22 / 28 |
| `ze` / `the` (nothing anywhere) | 7 / 6 | 13 / 13 |
| `payo` / `retr` / `conf` | 54 / 58 / 53 | 55 / 60 / 53 |
| `retry` / `payout` / `iban` / `PAY-231` | 67 / 61 / 15 / 11 | 66 / 61 / 15 / 11 |

Under *Titles*: `pa` / `re` / `co` 24 / 52 / 26 ms to 8 / 8 / 9, `pay` / `ret` / `con` 14 / 19 / 22 to 8 / 8 / 8,
one letter 8 and `retry` 16 on both.

Of the 172 two- and three-letter starts of the scale seed's 130 body words, names answer 170 in 15 ms or less. The
two they do not (`du`, `bec`) are in the table.

From the key to its rows on screen, real input in headless Chrome (`keys.mjs`), median of seven presses, three
rounds, the baseline page on the baseline backend against this head's on its own, at a load average of 4 to 8. The
second and third letters are typed into a box that holds the letters before with their answer drawn. No row moved in
any press, and none drew nothing:

| Key | CPU | Before | After | Björn's line |
|---|---|---|---|---|
| one letter | 1x | 41 / 35 / 32 ms | 32 / 32 / 33 | 100 |
| | 4x | 50 / 54 / 58 | 55 / 57 / 53 | 100 |
| | 6x | 62 / 71 / 70 | 74 / 63 / 72 | 200 |
| the second letter | 1x | 96 / 105 / 97 | 34 / 30 / 38 | 100 |
| | 4x | 126 / 111 / 125 | 61 / 61 / 63 | 100 |
| | 6x | 145 / 144 / 152 | 73 / 85 / 72 | 200 |
| the third letter | 1x | 89 / 85 / 88 | 23 / 31 / 33 | 100 |
| | 4x | 103 / 104 / 110 | 60 / 53 / 59 | 100 |
| | 6x | 118 / 118 / 121 | 67 / 73 / 73 | 200 |

**Expected on his PC:** about 30, 60 and 140 ms at 1x, 4x and 6x for the second and third letters, which is what the
one-letter pass expected of the first. They make the same read, 9 ms on the Mac and so 15 to 20 there, and they draw
into a list that is already open. Before, the read alone was 85 to 200 ms there (the Mac's 55 to 103, times 1.5 to
2), over his line before the page drew anything. Not measured there: the brief kept this pass off the PC, and the 6x
figure is the least certain, as it was for one letter.

The aim was half his line on the Mac. It holds at 1x and at 6x. At 4x the Mac is at 53 to 63 ms, under his line and
not under half of it, as one letter is: the answer is in after 20 ms and the rest is the page drawing its rows.

`testenv/perf/budget-keys.json` holds `everything-two-letters` and `everything-three-letters` at one letter's lines,
70, 100 and 130 ms.

Five settled screens of the launcher (the board, `PAY`, `p`, `pa`, nothing found) were shot on both heads in both
themes and compared with `pixel-same.py`: the same, or 18 px apart behind the dialog, where the same build shot
twice differs by up to 540. The two screens meant to change are the wait: a search held 4 s shows the board at
150 ms and the board dimmed at 900, where the head before showed *Searching…* in an empty list at both.

# Same answers

- `tests/it/short_words.rs` (knobas-search), eight tests. The decided answer on rows written for it, under both
  scopes, in either case, with the page dealt. What the characters are read as: `ops` and `op` one answer, the names
  *Titles* matched, and a stop word kept. Few names followed by the text, a name never displaced, the room dealt to
  the kind with no row, and a tenth name ending it. No name at all answered by the text whole, thirteen matches
  counted and ten drawn. A note, which stores no characters, named by its own lexemes. The bounded read held to a
  ranking of every name match with each row's lead worked out from the row, for 56 starts, plain and four ways
  narrowed, at three page sizes, and the names held to what *Titles* matched wherever the ranking reads the
  characters as a word. A row's stored characters spelled out, and following a new title. One statement text for the
  three lengths, each kind read through the index and never sorted.
- `tests/it/one_letter.rs` still holds one character as it was decided, the stored rows after every kind of write,
  now at all three lengths, and 26 queries the names do not answer to the answer the ranking of words gives, under
  both scopes.
- `group.rs`'s own tests hold the merge: names first, a row once, ten a kind, the room dealt by place, then rank,
  then date, then id, and each kind's total the rows the page holds.
- `tests/schema.rs` (knobas-db): a row mirrored before `0094` holds its two- and three-character rows once it runs,
  its single characters where `0093` put them, and the table has its key, its foreign key and its index again.
- `session.test.svelte.ts`: the list keeps what it drew through the pause and the read; the wait counted from the
  last key and ended by an answer or a failure; an opening that draws nothing of the one before; `flush`.
  `Launcher.waits.test.svelte.ts`, twelve tests through the box over a board that holds a row: no line in the first
  key's frame and the same list element after; the dim at the threshold and not before; an opening on words; the
  board's first read; Enter before the answer; the arrows; a waiting key asking at once; typing on, Esc, a failed
  search and an empty answer; a click. `Launcher.keys.test.svelte.ts`: an action's chord and Tab before the answer.
- On the scale clones, 69 queries the names do not answer are byte-equal between the two builds under each scope,
  the one-letter answers among them.
- Forty-seven broken versions each fail a test. Twenty-six of the search: three letters ranked; characters that are
  not letters and digits taking the names; the characters read under `simple` alone; a stop word dropped; a note
  leading by its first letter alone; one character's few names followed by the text; the text asked under *Titles*;
  no name at all taken for few; a filled kind or a full page still asking the text; a row drawn twice; the text
  ahead of the names; the room dealt by rank alone, by id alone, with the dates unread; a kind past ten; the page past
  its limit; a total left as the statement gave it; a header kept as a row; the stored leads read by one character;
  a ticket's number stored at two and three characters; three characters never stored; the fill skipped; the
  foreign key not put back; the change trigger writing one character. Twenty-one of the box: the panel read off the
  typed text; the wait counted from the first key; an answer leaving the wait said; an opening drawing the one
  before, in the session and in the box; `flush` asking nothing; a scheduled query not owed; Enter, the arrows, Tab
  and a chord each acting at once; a waiting key not asking at once; typing on and Esc keeping the keys; a failed
  search opening; the keys never pressed again; *Searching* and the board's line said at once; the rows dimmed at
  once and never; a pasted link taking the panel before it answers.

# Left, with what it would take

- **A longer word that half the mirror holds is as dear as it was.** `payo` 55 ms, `retry` 66, `payout` 61 of
  backend on the Mac, so 80 to 130 on his PC, a key at a time from the fourth letter on. The scale seed makes this
  worse than a team's data would: its bodies are built from 130 words, each in about half the rows. But the cost is
  real wherever a word is broad: 3 µs a match on one process, since the planner reads `payout:*` as 3,792 rows where
  18,553 match and starts no worker, and on Windows a worker costs 31 to 37 ms to start anyway. Under it is the
  ranking itself, which reads every match to keep ten a kind. What would change that is an order that can be stored
  for a word, which a rank over a prefix is not, or a rule like this pass's for longer words, which story 26 rules
  out today (a body that says a word often may outrank a name). It needs its own ruling.
- **Characters no name holds, in a text that holds them broadly, are as dear as they were**, and 7 ms dearer. Two of
  the seed's 172 starts. The same ranking as above.
- **The withdrawn entities are read by a scan of every entity**, 2 ms of every ranked search and every board on his
  scale. An index of the 291 that are withdrawn (`knobas.entity (id) where deleted_at is not null`) makes it 0.05 ms,
  measured. Not added here: it is a migration on a table of the `knobas` schema, for 2 ms of 55.
- **A name that is exactly what was typed is not first.** A repository named `api` stands among those that begin with
  `api`, newest first. It would take a third tier in the stored order.
- **A ticket is not found by its number alone**, and four-letter stop words (`down`, `over`, `with`) still answer
  nothing while they are being typed. Both as before.
- **A press that changes nothing reads as a miss in `keys.mjs`.** `le` and `led` name the same rows, so `d` after
  `le` redraws nothing and the probe finds no frame to time. The cases press keys that change the list.

# Reversing it

Part one: `named_by_its_start` in `knobas-search/src/lib.rs` answering `false` for two and three characters sends
them to the ranking of words. `0094` stays, and its longer rows are then written and never read. Part two:
`Session.mode` reading `this.raw` in place of `this.answered` draws a panel for the typed text at once, and
`waitsForTheAnswer` in `Launcher.svelte` returning `false` lets every key act on what is drawn.
