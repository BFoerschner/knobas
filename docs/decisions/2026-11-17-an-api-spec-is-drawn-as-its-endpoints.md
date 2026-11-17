# 2026-11-17: An API spec is drawn as its endpoints (ux18)

**Asked (Björn, on PR #2479, through the orchestrator):** "Implement a swagger view for OpenAI files" (OpenAPI and
Swagger specs). Asked whether as an *Open in…* entry or as a renderer: **only as a file renderer**. A spec opens drawn,
as Markdown does, with *Source* one pick away in *Render as*; it is not listed under *Open in…*.

The two judgment calls went to Fable (xhigh) under Björn's standing instruction; its answers are below as given.

## What is a spec

OpenAPI 3.0 and 3.1 and Swagger 2.0, YAML or JSON, at any path and name (`openapi.yaml`, `api/v1/spec.json`,
`petstore.oas.yml`, a file with no extension), recognised by what it holds: a top-level `openapi: 3.x` or
`swagger: "2.0"` key (`render/openapi-sniff.ts`). A YAML top-level key is a line at column 0; a JSON one is a key of
the outermost object, found by a scan that tracks strings and depth. The sniff runs on every text file the Code page
opens, so it never parses and reads at most the first 256 KB; writers put the version key first. Anything else opens as
before: a `docker-compose.yml`, a `package.json`, a file whose `openapi:` is nested.

The registry gained one field for this, `sniff` (`render/registry.ts`): a renderer with one takes a file only when its
text passes, and never where no text is at hand (a binary file, a diff's file list), even under a Settings default
that maps a pattern to it. `*.yaml` mapped to OpenAPI draws the specs among them and every other YAML file as before.
OpenAPI sits after the built-ins and before Source, so a Markdown file that starts with `openapi:` stays Markdown, with
OpenAPI offered beside it.

## What it looks like

knobas's own look, not Swagger UI's (`tf/render/TfOpenApi.svelte`, `TfOasOperation.svelte`, `TfOasSchema.svelte`,
`tf/css/openapi.css`). The design pass (frontend-design) kept the page's grammar and spent its one loud thing on the
HTTP method: Archivo at width 62 and weight 800 in a column of its own, the way the width axis does the shouting for the
job key, so a list of thousands is scanned by verb and path. Swagger's coloured method pills were the default it
turned down: a method is not a state, and tone is for state only, so GET and DELETE are the same ink. Its boxed card
per operation was the other: rows are hairline rows on the sheet, as every list in the app is.

- **The head:** the API's title (`--t-2xl`, 700, width 85), its version as a label and the spec's version, the
  description as Markdown (the Markdown component), the servers as text in the code face. Nothing calls them: there is
  no *Try it out*. What the reading passed over (an operation that is not an object) is a fold under the head.
- **The band**, stuck to the top of the page's scroller: *Filter endpoints* (the small filter of Code's lists,
  `.tf-minif`, with `filterKeys`, so `Esc` clears it), every word matched against method, path, summary, tags and
  operationId; the count (*6 endpoints*, *1 of 6*), holding its room; then *Back to ⟨where you were⟩* after a ref was
  followed, and *Schemas*.
- **The endpoints**, by tag in the spec's order, an operation under each tag it names; those no tag holds by their
  path's first segment (`/health`), in the code face. A row: the method, the path in the code face with its
  `{parameters}` in `--ink-2`, the summary in `--ink-2`. Deprecated: the path dims and *Deprecated* in `--warn`.
- **An operation opened** in place, inset on `--sheet-2` with the fill's edge: its description (Markdown), its
  operationId and the security it asks for, *Copy link*, then *Parameters* (name in the code face, *required*, where,
  type, words, example), *Request body* and *Responses* by status, each media type's schema and examples (a segmented
  control where there are several). A path's parameters are merged into the operation's. Swagger 2.0's body parameter
  is the request body, its `produces` and `consumes` the media types, `host` and `basePath` the server.
- **A schema** is its one line (`integer, int64`, `array of Pet`, `string, or null`, `one of 3`) and its properties, one
  a row, each opening under a caret. A ref's name is a link: to a named schema it follows to that schema in *Schemas*
  (opened, focused, and *Back to* puts the reader back where the row stood on screen); a ref opens inline under its
  caret. A ref to a schema it sits inside does not open again, so a cycle (`Pet.parent: Pet`) ends at one level and its
  name follows.
- **Schemas**, the named ones (`components.schemas`, 2.0's `definitions`), each opening in place; the filter takes them
  by name.
- **A spec that does not parse**, or is not one knobas draws (no `info`, no `paths` in 3.0 or 2.0, `openapi: 3.2`),
  says why and on which line, with *Show source*.
- **Anchors:** `#/repo/<id>/at/<place>/<path>` (`placeHash` in `tf/repo/repo.ts`, as a line link's `line/<n>/<path>`)
  opens the file at an operation (`get /pets/{petId}`) or a schema (`schemas/Pet`). *Copy link* copies it.

## The two rulings

**1. A renderer of knobas's own, with js-yaml, parsed in a worker** (Fable). Swagger UI (`swagger-ui-dist`,
Apache-2.0, about 1.4 MB of minified JS plus its CSS, React), Redoc (MIT, about 1 MB, React with styled-components and
mobx) and the web components (RapiDoc, Stoplight Elements, Apache-2.0, React, large) each bring their own look and a
React runtime, fetch remote refs, ship *Try it out* or something like it, and give no control over windowing 5,000
operations or the 100 ms targets. What was added: **js-yaml 5.5.0** (MIT; 80 KB minified, 21 KB gzipped, in the spec
worker's chunk of 55 KB and nowhere at start-up). JSON is read with `JSON.parse`, and a JSON file that does not parse is
read again by js-yaml, which says on which line it stopped (WebKit's `JSON.parse` does not).

**2. A ref into another file of the same repository is resolved from the source at the spec's own commit, lazily**
(Fable: option B, with A as its fallback). The file read is the one the Code page already has (`entity_file_bytes`,
the byte read a Markdown file's images use), at the commit the spec was read at, so a ref never mixes two versions; a
clone at another HEAD would. Reading the same repository through knobas's own source is not the banned call to an API's
servers. Its bounds, as ruled:

- Nothing is read at open, on scroll or on hover. A read happens when the reader asks for it (*Show it here* on a
  body's or response's schema, a caret, an external path item opened), one hop a time, never transitively.
- The endpoint list, the filter and the anchors index the spec's own file. A path item kept in another file
  (`paths: /items: $ref: ./paths/items.yaml`) is listed by its path and read when opened.
- At most 5 MB a file and four reads at once; each file is read once a session, parsed in the same worker, and kept
  with the 64 most lately used (the spec itself always).
- Paths resolve against the referring file's folder, normalised, and one that climbs out of the repository is refused.
- While it reads, the node shows *Reading ⟨file⟩…* in its place and fills in below that line, so nothing above moves.
- A missing file, one over the cap, one that does not parse or a pointer that is not in it shows the reason, with the
  ref's name still a link that opens the file on the Code page (at the schema, where that file is a spec).
- A ref to a URL is named, in `--ink-2`, its title *outside the repository, not fetched*, and never read.
- Cycles across files are caught the same way as inside one: by the file and pointer of every schema a node sits
  inside.

## How it stays light

- **Nothing at start-up.** The view, its CSS and its port are one chunk (38 KB, 12 KB gzipped, CSS 8 KB) loaded with the
  first spec opened (`{#await import(...)}` in `TfRepoFiles`); the worker (55 KB with js-yaml) starts with it. What
  start-up holds is the sniff (1.3 KB) and the registry's entry. The fake's fixtures are in the development build only.
- **A fast YAML reader before js-yaml** (`render/yaml-fast.ts`). js-yaml took 45 ms of the 5,000-operation spec's open
  (2 MB; `JSON.parse` reads the same document in 8). The reader takes the block YAML spec tools write (mappings,
  sequences, one-line scalars and flows, literal and folded block scalars) in about half that, and answers exactly what
  js-yaml answers or nothing: anchors, tags, a scalar or a flow over several lines, an indentation indicator, a tab in
  an indent, a key YAML reads as a number, a second document, and every broken file go to js-yaml, which says where it
  breaks. Its tests hold it to js-yaml's answers on every fixture and shape. The worker keeps the last four specs
  parsed, so going back to one does not parse it again.
- **Off the main thread.** The worker parses the spec and keeps it; the page holds an index (a row per operation, the
  tags, the schemas' names) and asks for each operation, ref and path item when it opens. Where a worker cannot start,
  the same code runs on the page's thread, and a session whose worker died is taken over there with its files.
- **Little on the page's thread.** The PC's profile of the big open was mostly signal reads: each of 5,000 rows asked
  the reactive sets of what is open. The rows read plain copies taken once a pass, and the filter's haystacks are built
  at its first word and kept for the spec.
- **Windowed.** The endpoint list is one `Windowed` list of group heads, rows and opened details, so 5,000 operations
  draw what the screen shows. A row is one line at a fixed height; an opened operation is measured once drawn.
- **Nothing moves once drawn.** An operation opens once its schemas' refs inside the file are resolved (the worker
  answers within a frame), so it draws whole; a ref into another file is the one thing that fills in, below its own
  line. The band's count holds its room, and *Back to* comes at the band's end, after everything else in it.

## Timings

`testenv/perf/openapi.mjs` (the built app under `?fake-ipc&fake-openapi`, headless Chrome, the CPU throttled), five
runs a case, the median against the line, on Björn's PC (a headless Chrome on its WSL side, driven over a tunnel; the
PC's whole lines: a page 100 / 200 / 300 ms, a press 100 / 100 / 200 ms). In ms at 1x / 4x / 6x:

| Case | 1440 x 868 @ 1 | 2560 x 1440 @ 1.25 |
|---|---|---|
| A small spec opened from the README | 33 / 51 / 67 | 34 / 47 / 67 |
| A small spec, the page's first visit (from the Inbox) | 32 / 59 / 77 | 31 / 56 / 78 |
| The 5,000-operation spec opened from the README | 49 / 165 / 224 | 56 / 176 / 243 |
| A filter key on 5,000 operations | 10 / 40 / 62 | 12 / 49 / 69 |
| An operation opened | 8 / 15 / 22 | 25 / 16 / 24 |
| A `$ref` followed to its schema | 10 / 14 / 19 | 6 / 15 / 21 |
| An operation opened on the 5,000 | 10 / 26 / 40 | 28 / 27 / 41 |
| Scrolling the 5,000 (fps, no task over 60 ms) | 61 / 61 / 61 | 61 / 61 / 59 |

Before the fast reader and the plain sets, the 5,000-operation open was 82 / 179 / 281 ms on the Mac (over its
half-lines) and 59 / 216 / 316 on the PC. Code's other files open as before (PC, 1440, before / after): a Markdown
README 56 / 57, 45 / 43, 68 / 66; `Cargo.toml` 20 / 27, 30 / 30, 46 / 40. The README's second line grows about 15 px
some 50 ms after it draws, on both builds: not this change, and left to the app-wide movement audit. Nothing the view
drew moved after it drew in any case.
