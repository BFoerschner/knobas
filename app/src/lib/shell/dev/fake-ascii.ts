/**
 * `?fake-ascii` (the Cascadia Mono pass, 2026-11-21): ASCII art, box drawing and arrows in a code file
 * (`docs/states.txt` in the working tree, so its diff too), a run's log and a Docs page's code block, where
 * programming ligatures would join `->`, `=>`, `---` and `|--` into signs. Nothing changes without the flag.
 */

export const asciiArt = (): boolean => new URLSearchParams(globalThis.location?.search).has("fake-ascii");

/** The drawing as it was at the working tree's base. */
export const ASCII_BEFORE = `Payout retry states
┌───────────┐  transient  ┌───────────┐
│ submitted │ ──────────> │  waiting  │
└─────┬─────┘             └─────┬─────┘
      │ ok                      │ 24 h
      v                         v
┌───────────┐             ┌───────────┐
│   paid    │ <---------- │  retried  │
└───────────┘             └───────────┘

+---------+-------+-------+
| code    | retry | after |
+---------+-------+-------+
| 503     | yes   | 30 s  |
| TIMEOUT | yes   | 60 s  |
+---------+-------+-------+
`;

/** The drawing in the working tree: a state and an arrow more, and the tree of files. */
export const ASCII_AFTER = `Payout retry states
┌───────────┐  transient  ┌───────────┐
│ submitted │ ──────────> │  waiting  │
└─────┬─────┘             └─────┬─────┘
      │ ok                      │ 24 h
      v                         v
┌───────────┐   gave up   ┌───────────┐
│   paid    │ <========== │  retried  │
└───────────┘             └─────┬─────┘
                                │ 3 tries
                                v
                          ╔═══════════╗
                          ║  alerted  ║
                          ╚═══════════╝

+---------+-------+-------+
| code    | retry | after |
+---------+-------+-------+
| 503     | yes   | 30 s  |
| TIMEOUT | yes   | 60 s  |
+---------+-------+-------+

payout-service
|-- batch.rs   -> builds the SEPA file
|-- retry.rs   => decides when to send again
\`-- psp.rs     <- the PSP's answers
--- => -> <- <= >= != == === :: |> <!-- www 0O 1lI
`;
