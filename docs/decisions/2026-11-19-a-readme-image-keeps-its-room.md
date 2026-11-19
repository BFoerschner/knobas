# 2026-11-19: A README's image keeps its room

**Asked (the ux21 moves pass's leftover, for ux21):** an image a repository's README reads from the repository
(`payout-service`'s `docs/img/logo.png` on the fixture) pushed the text under it about 15 px the first time it loaded in
a session. Its size was kept for later draws (`tf/markdown/image-sizes.ts`), but nothing knew it before the first load.

## Where images draw

Every Markdown surface draws through `TfMarkdown` (`docs/decisions/2026-11-17-nothing-moves-once-drawn.md`):

- **Relative images** (`data-md-path`) read their bytes through the surface's byte read: a README and any Markdown file
  in the Files tab, an OpenAPI spec's prose (`TfRepoFiles`). From the checkout where it holds the commit, else the forge.
- **Attachment images** (`attachment:<id>`) read a `data:` URL from the ticket's previews, once in view: a ticket's
  description (`TfJob`) and its comments (`TfConversation`).
- **`data:` images** carried whole in the text, on any surface (PR bodies, diff comments, commit messages).
- A remote (`http`) image loads nothing (the CSP), and Docs pages draw no images, so neither can move.

## How the size is known now

- **The head of the bytes** (`render/image-size.ts`): PNG, GIF, JPEG (turned as its EXIF orientation says, as the
  webview draws it), WebP (all three kinds), BMP and SVG (pixel sides, else the `viewBox` fitted to 300x150 as an `<img>`
  draws it), read with nothing decoded. The `<img>` gets its `width` and `height` before its `src`. A `data:` image is
  sized from its URL as the text draws.
- **Kept between sessions**: every size seen goes to `localStorage` (`knobas.image-sizes`, the latest 500), keyed by
  where the image comes from (`imagesOf`: the repository or the ticket, then the path or attachment). The key used to be
  the README's directory alone, so two repositories' `docs/img/logo.png` shared a size.
- **Never seen**: what stands from the first image of unknown size down is held unseen in its room (`tf-heldbelow`)
  until its size is in, at most `IMAGE_HOLD_MS` = 300 ms; the text above it reads at once. `onheld` tells the page, which
  holds what stands under the text: the ticket's attachments and conversation under its description, and a comment's
  later comments and the box. **Fable decided** the hold (option B of three, 300 ms over 150 or 500): a local read
  answers in 5 to 30 ms, 300 covers it at 6x and most forge reads, and 500 would let the hold alone pass the slowest
  page target.
- **Known before the bytes** (kept, or named in the text): the image stands as a blank SVG of its size until its bytes
  load. The webview draws an `<img>` with no `src` as its alt text whatever its `width` and `height` say, so the kept
  sizes of the 2026-11-17 pass never held the room: on a return the logo drew 115x24 as its alt and became 96x32.
- An image whose text names both sides keeps them; one that names one side gets the other from its shape. An
  attachment is fitted to its 280 px figure in its shape (a kept size used to be stretched by the figure's
  `max-height`). Its load checks the size, and an image that changed since it was kept is set right then.

## Proof

- Tests (`image-size.test.ts`, `TfMarkdown.test.svelte.ts`, `ticket-page.test.svelte.ts`) fail on 23 mutants: no size
  from the bytes, no hold, a hold that never ends, a hold of the image's own block only, no store, a store written only
  later, the key without the repository, no attachment fit, no `data:` size, one named side ignored, named sides
  overwritten, a kept size never corrected, no stand-in for a kept size or for named sides, the stand-in's load taken
  for the image's, the EXIF turn, a Huffman table taken for a frame, WebP sides off by one, an SVG shape not fitted,
  `onheld` tracked by the text's draw (a loop of redraws the page test caught), and the ticket's conversation, a
  comment's later comments and the box not held.
- `moves.mjs`' `repo-readme-image` (fixture: `payout-service`, its bytes 150 ms after the views, no size kept). On the
  base `6deaf60a`, every one of 18 opens moved (1x / 4x / 6x, first and return, at 1440x868@1, 2560x1440@1.25 and
  5120x1440@1): the logo 115x24 to 96x32, the text under it 15 px. After: nothing moved in 18 opens.
- The pages this touches, `moves.mjs --only repo,ticket` (every repository place and tab, the ticket and its tabs, the
  Tickets board) at 1x, 4x and 6x: nothing moved in 306 opens, 102 at each of 1440x868@1 (Chrome on the WSL box),
  2560x1440@1.25 and 5120x1440@1 (Chrome on the Mac). `repo-commit` drew nothing on any of them, as before this pass.

## Seen, not this pass's

A Mermaid fence shows its source until the engine answers, about 400 ms after the text on a first load at 1x, then the
drawing replaces it: the fences under it move 53 px at 2560 and 5120 wide, on the base and after alike, first open and
return. **Fable decided** to leave it to its own ticket (a 300 ms hold would still miss a cold drawing, and a kept
diagram size needs its own keys and proof, #2510): `repo-readme-image` draws with no `Worker` (`noWorkers`, so the fences stay
source), and the same case with workers is that ticket's failing test.

**Reversing it** is `render/image-size.ts` and `image-sizes.ts` back to the session's map, `TfMarkdown`'s holder and
stand-in taken out, and the pages' `onheld`: a first load then moves the text under the image again, and a kept size
draws as alt text until the bytes load.
