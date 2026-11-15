# 2026-11-15: What was still over on the PC with ux13: first opens, typed keys, the expanded board, a repository

**Asked (Björn, on PR #2479, through the orchestrator):** the ux13 Windows pass, measured in knobas's own WebView2
window on his PC (5120x1440 at 239 Hz), left these over a line: Builds' first open (282 ms at CPU 4x, 415 at 6x; at
full screen 111 at 1x and 333 at 4x), the Tickets board's first open (202 at 4x), the expanded board's keys (109 to
117 at 4x), one typed key of the Tickets list (145 at 4x, 214 at 6x), Code (114) and Monitors (119) filters, and a
repository's first open after the upgrade (4.5 s at 1x, 1.3 s at 4x, 384 ms at 6x, its README read behind four
branch reads). And the gates owed on the two fixes that pass pushed (`0998d62d` Builds' clip painted, `1a061ba2`
Commits read after a sync). His rule: directness, performance and responsiveness over data scarcity; a decision goes to
one Fable agent. The Mac aims at half each line, since the PC takes up to twice its time.

**How it was measured.** Headless Chrome on the Mac (M2 Pro, on AC power) at CPU 1x, 4x and 6x over fresh APFS copies
of the saved scale home with its secrets moved out, through release builds of the windowless backend with debug
assertions, the front end built as the probes build it. Before and after alternated, each side on its own fresh
copy, two rounds where a number is given twice. Traces with `moment.mjs`, profiles with `profile.mjs` against a build
with source maps. Other agents' work kept the machine at a load of 5 to 20, at moments 75 and 145; each run says its
load and the runs in a spike were thrown out. The repository's first open ran against `standin-forge.mjs --repos` (30
ms a request) with the Gitea source's credential kept, the ux13 build (`v3.2.9-ux13`) beside this head.

## The gates owed on `0998d62d` and `1a061ba2`

In a clean clone at `0998d62d`: `just check-front` 5,137 passed and 1 failed, a Tickets frame test that passed three
times out of three alone (it failed at a load of 75; fixed below); the press trips with `--strict` 272 of 272, the 16
`rows-still` among them; `just frame-budget` on the fixture 43 within and 2 over (`tickets-list` and `monitors` at 6x,
52.3 and 51 frames a second, at a load of 12 to 19), and `--real` 63 within and 2 over (`builds-main` 54.5 and
`monitors` 53.8 at 6x), 24 of 24 within when the three were run again at a load of 8; `just open-budget --real` 225 opens, 1
over, Builds' 4x first open drawn at 231 (the item below). Builds' scroll before and after `0998d62d`, alternated:
`builds-main` at 6x 58 and 59.8 frames a second before, 59 and 58.5 after; `pipeline-history` 44.5 and 60 before,
55.5 and 59.5 after. No cost of the painted clip shows on the Mac, where nothing scrolls on the main thread for ClearType.

## 1. First opens: the sections are built ahead of their press

**Why.** A first open cost about twice a later mount of the same page and four times a return to a kept one, on the
saved database at 4x: Builds 231, 107 and 59 ms drawn; the Tickets board 191, 94 and 45; Monitors 253, 62 and 41; Code
101, 78 and 43. A trace of Builds' first open at 4x is one 219 ms task: the page's code run for the first time (V8's
compile and caches, Svelte's templates made), its styles worked out once, and three forced layouts on the way (the
windowed lists' ancestors, the scroll keeper on the page being left, the rail's `scrollIntoView`).

**Asked of Fable:** (A) the shell opens the sections hidden while idle, into Pages kept open's free rooms; (B) per-page
warm-ups off screen with made-up data, as `diff/warm.ts`; (C) nothing. **Ruling: A**, with conditions: count a moving
pointer as input; hold the page at the address its nav entry goes to; say it in the setting; try one laid-out frame
before a page hides and keep it only if it pays.

**Taken.** `tf/ahead.ts`: once the window is ready, Tickets, Builds, Code, Monitors, Inbox and Docs, one at a time,
each after a second with no key, press, pointer move, wheel or scroll and an idle moment, at `navHref`'s address, never
the section shown, each into a free room behind every page held (`holdBehind`). A held page is a kept page; a role view
held ahead now holds the run count it mounted at (it held 0, so its first show re-read after any earlier run); a Jira
view marked new stops saying so only once shown. A windowed list placed while its page is hidden draws a screenful from
its top, not its overscan alone, so the first show builds no rows (one shown before reads its hidden page as it did:
kept whole, Builds' return at 6x was 95 ms where it was 58). Settings › Performance says the rooms start with the
sections and that 0 turns it off.

**Not taken.** The laid-out frame (a held page at real size, `visibility: hidden`, for two frames): Builds at 6x 138 and
126 ms became 176 and 149, the board 174 and 139 became 213 and 156.

| First open, drawn ms | 4x before | 4x after | 6x before | 6x after |
|---|---|---|---|---|
| Builds | 177, 159 | 75, 93 | 260, 246 | 126, 133 |
| Tickets board | 290, 235 | 113, 158 | 259, 239 | 158, 175 |
| Monitors | 101, 108 | 69, 67 | 180, 172 | 114, 114 |
| Code | 148, 140 | 60, 74 | 163, 178 | 102, 104 |

A return costs more where the held page drew its screenful: Builds at 6x 61 to 81 ms before, 94 to 110 after; Code at 6x
49 to 62 before, 73 to 92 after; all far inside 300. **Expected on the PC** (its first opens ran 1.0 to 1.6 times the
Mac's): Builds about 90 to 150 ms at 4x and about 200 at 6x, the board 100 to 160 at 4x; owed in his window, with the
start page's own draw no slower than before (Fable's gate).

## 2. Typed keys: the filtered lists window from 40 rows

**Why.** Each over key was the one that narrowed its list under the count from which it windows (300 for the Tickets
list, 100 for Code's pull request groups and Monitors), so every match was built and laid out in that key's frame:
1,018 elements restyled and 2,021 layout objects on Code's second key; a Monitors row is 71 nodes. The script a key ran
was small; the rows it built were the cost.

**Taken.** `from: 40` for the three lists, as Builds' rail and the Inbox already window.

| Worst key to paint, ms | 4x before | 4x after | 6x before | 6x after |
|---|---|---|---|---|
| Code's filter | 103, 129 | 31, 32 | 166, 118 | 55, 59 |
| Monitors' filter | 73, 116 | 50, 43 | 111, 154 | 67, 61 |
| Tickets list's filter | 40, 40 (99.8 in a first run) | 28, 33 | 158, 134 | 136, 67 |

**Expected on the PC** (1.1 to 1.45 times the Mac here): every key under 100 at 4x, the list's slowest near 100 to 200
at 6x.

## 3. The expanded board's keys: fifty a column first

**Why.** A key on the board with every column's *Show more* pressed four times (1,243 cards) read 610 KB in 54 to 62
ms; its answer reached the page at 90 to 110 ms at 4x, its rows 15 to 25 ms later. The normal board reads in 39.

**Asked of Fable:** (A) read what fills the screen, fifty a column, then the whole once the keys pause, as the list after
*Show N more* already does; (B) a cheaper backend read; (C) leave it. **Ruling: A**, unlaned first, fifty whatever the
words, no whole read where the quick answer is whole, the whole read keeping the first fifty and every row in place.

**Taken.** `board-reader.svelte.ts`: so. The whole read sorts nothing again. Found on the way: a card names its source
when another source's card holds its key, and Gitea and Forgejo mirror the same repository, so a twin only the whole
read brought grew a drawn card by a line, which the probe counted as a move; a card now keeps whether it names its
source until the person's next read. `keys.mjs` times `rows` by the reads a key sends at once, and `moved` still holds
the whole read to rows below the screen.

| Expanded board, rows ms | 1x before | 1x after | 4x before | 4x after | 6x before | 6x after |
|---|---|---|---|---|---|---|
| a letter | 70, 67 | 66 | 94, 98 | 61 | 102, 97 | 73 |
| `-project:` | 75, 78 | 53 | 91, 96 | 70 | 102, 100 | 76 |

No row moved. **Expected on the PC** (1.2 times the Mac here): about 75 to 85 ms at 4x.

## 4. A repository's first open after the upgrade

**Why on the PC.** Four branch views read at once for the header's *New pull request*, 1.0 to 2.8 s on Gitea, and the
README behind them. `e4fdb822` decides the button from the mirror and `f2514e0c` stores the README; on the Mac the
first open now sends no branch view, and the README goes in the first round beside the branch list and the last commit.

| `scale-data/warehouse-loader` first open, drawn ms | 1x | 4x | 6x |
|---|---|---|---|
| ux13 | 329 | 282 | 387, 383 |
| this head | 142 | 167 | 410, 316 |

At 6x the page itself is most of it: its reads leave about 100 ms after the press (the page's own mount), and the README
guess waits for the checkout to say it holds no copy; remembering the repository's sections to send it sooner would move
it 20 ms, so it was left. The stand-in's git ran 300 to 600 ms a request when six ran at once under load, which no real
Gitea does for a file it holds. **Expected on the PC:** no 4.5 s at 1x (the branch views are gone); the first fill
still waits one round of the forge for the top folder and the README until the stored-trees pass has run, after which
the landing reads nothing from the forge. Found for the forge agent: the landing reads `branches/main` twice, and
the early compare for *New pull request* starts once the tree draws, beside the README.

## Found and fixed on the way

- A Tickets frame test (`a column a filter grows under 24 cards ...`) failed under load: `settle`'s five zero timeouts
  outlasted jsdom's frame. It runs its own frames now.

## Owed on Windows

1. First opens of Builds, the board, Monitors and Code at 1x, 4x and 6x, the start page's own draw with the sections
   held ahead, and Settings › Performance's words.
2. Typing into the Tickets list, Code and Monitors at 4x and 6x; the expanded board's keys at 4x.
3. A repository's first open on the upgraded home before and after its first sync.

## Gates

On this pass's head, in a clean clone: see the PR comment for this pass.
