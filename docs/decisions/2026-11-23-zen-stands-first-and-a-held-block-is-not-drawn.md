# Zen stands first in a file's header, and a block held unseen is not drawn (2026-11-23)

The ux30 end gate. `openapi.mjs` and `jar.mjs` reported MOVED on "a Markdown README opened from Cargo.toml" at 1x at
1440 x 868 and 2560 x 1440 at 125 %, on the Mac. `just moves` on the file view's places then failed `repo-file`.

## The README: the probes counted what was not drawn

On the first open of the fixture's README in a session, its two Mermaid diagrams have no kept size and the engine is
warm, so the text holds the blocks from the first diagram down unseen in their room (`tf-heldbelow`,
`visibility: hidden`; `2026-11-19-a-readme-image-keeps-its-room.md`, `2026-11-20-a-diagram-keeps-its-room.md`). About
70 ms later the diagrams' sizes are in, and those five blocks show where they stand (in Chrome: the second diagram's top
1064 to 1143, the Rust fence 1226 to 1536, each going from held to shown in that same frame). Nothing seen moved:
everything above the first diagram stayed put, and every held block was below the window at 1440 x 868. Later opens
draw at the kept sizes with nothing held, which is why 4x and 6x never showed it (they run after 1x in the same Chrome).
ux29 (`v3.2.9-ux29`) does exactly the same, and both probes flag it there too.

The two probes' `shown` used `checkVisibility()`, which counts `visibility: hidden` as visible. `moves-probe.js`, the
`just moves` gate, already skips a box held unseen ("is not drawn yet"). The probes now ask
`checkVisibility({ visibilityProperty: true })`, so a held block is neither drawn when first seen nor a move when it
shows. A control run that pushes the drawn README down 20 px after it draws still reports MOVED.

## The file header: Zen moved Render as and Pipeline

Moving between files, the file drawn stays (dimmed) while the next one is read. From a README to `Cargo.toml` the trail
went from *Render as Markdown*, *Pipeline*, *Zen*, *Open in…*, ⋯ to *Render as Source*, *Pipeline*, *Open in…*, ⋯. The
trail stands from its right edge, so *Zen* leaving slid *Pipeline* 48 px right, and the switch moved 70 px and narrowed
22 px. `moves-probe.js` allows one control to take the next file's words in the frame it lands if it keeps one edge (the
*Render as* swap). A second move breaks that allowance. ux29 passed `repo-file`. Zen mode came in ux31.

*Zen* now stands first in the trail, before *Render as*: *Zen*, *Render as …*, *Pipeline*, *Open in…*, ⋯. Both ways of
showing the file stand together, and the actions keep the right edge. *Zen* coming or going moves nothing to its right,
and *Render as* takes the next renderer's name with its right edge kept. The design system's Zen paragraph says so.

Reversing it: put the `{#if zenHere}` block back after *Pipeline* in `TfRepoFiles.svelte`, and `repo-file` fails again.
