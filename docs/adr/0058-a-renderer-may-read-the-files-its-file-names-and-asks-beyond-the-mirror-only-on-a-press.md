---
status: accepted
---

# A renderer may read the files its file names, and asks beyond the mirror only on a press

Stands beside [ADR-0029](0029-the-shell-draws-a-role-never-a-source.md), [ADR-0048](0048-a-repositorys-files-are-indexed-outside-postgresql.md) and [ADR-0057](0057-knobas-sends-a-request-a-person-composed-and-keeps-its-secret-for-one-origin.md). Björn, on PR #2479: "I want Fable to build me a renderer for .xsd Files. It should work out references automatically between files (like the opentrans, bmecat etc.) if they're findable." A renderer drew one file. The API explorer already read a file its spec named, one at a time, when a person opened the ref. An XML Schema is not readable that way: of the 1,380 leaves of an openTRANS order with every optional part, 1,001 are declared in BMEcat's file. Decided 2026-11-23 (Fable, xhigh, under Björn's standing instruction, amended the same day by Björn's answers; `docs/decisions/2026-11-23-an-xml-schema-is-explored-across-its-files-and-an-example-is-made-from-it.md`): **a renderer may read, unasked, the files its file names in the mirror, at the commit shown, under bounds it states, and draws them as one model; it asks an address outside the mirror only when a person presses, once, and says what it kept.**

- **A set, not a file.** The schema explorer reads the file opened and every file it reaches by `include`, `import`, `redefine` and `override`, in the worker, and keeps one model. At most 5 MiB a file, 400 files and 64 MiB a set, 6 reads at once. A set that meets a bound says which.
- **Found in a fixed order, and said.** What the person pointed to or had downloaded, beside the file, the repository's XML catalog, by name, by namespace, another mirrored repository, then two W3C schemas that come with knobas. A file must declare the namespace asked for. Every file's row says how it was found.
- **Another repository only where it is the only answer.** A reference nothing in its own repository answers is taken from another mirrored repository where exactly one declares the namespace, read at that repository's default branch's head, and its row says which and at what. Where two declare it, a person is asked once.
- **Never fetched by itself; fetched on a press.** Opening a file opens no connection. An address a file names (a schema's location, an image) is offered where nothing in the mirror answers, and one press sends one `GET` through ADR-0057's client: no secret, no cookie, no retry, a redirect followed on the same origin only, a bound on its size. A schema fetched this way is kept as a file in the profile with its address, its hash and its day, used from then on, and listed in Settings with *Download again* and *Forget*. An image fetched this way is a live read in memory (ADR-0032), dropped at quit. A host a person chose to trust for images is asked again without the offer, and still only on the press that opens the image.
- **The repository is indexed on demand.** Which `.xsd` declares which namespace is read only when a reference is not found beside its file, from at most 300 files, kept in the page's memory by content, and read again when a file's content changes. Nothing of it is in the database.
- **What knobas makes is labelled.** An XML example made from a schema says so in its own text, with the seed that makes it again. Nothing made is saved unless the person saves it.
- **A file of somebody else's is never trusted with the engine's unbounded parts.** Its entities expand under a budget and its external ones are never read. Its patterns are matched by knobas's own matcher under a step budget, not by `RegExp`. A query typed against it is read by knobas's own XPath under a budget. The check is libxml2 compiled to WebAssembly in the worker, given the set's texts from memory, and a test holds that it asks for nothing else.

## Rejected

- **A followed import as a navigation to the other file's own view.** The reader loses the rail at every step.
- **Fetching an address by itself**, when a file opens or a line scrolls into view. A view that tells a stranger's server it was looked at, and one commit that draws differently by the day.
- **An image drawn by the web view from its address.** It would send the web view's own cookies and referrer, and the CSP refuses it for that reason.
- **Another repository taken where two declare the namespace.** knobas cannot know which the author meant.
- **A file whose namespace is wrong, taken with a warning.** BME's own `bmecat_2005.xsd` and the one openTRANS ships differ in exactly that.
- **An index of every repository's schemas in the database.** A table and a sync pass for what a page can build and forget.
- **A validator of knobas's own**, which would judge knobas's examples by knobas's reading of the schema, and **a native C library in the app process**.

## Consequences

- One worker, two settings rows (`xsd.locations`, `render.image_hosts`), a folder in the profile (`schemas/`), four commands (`xsd_locations`, `xsd_set_locations`, `xsd_fetch`, `image_fetch`), one read in `knobas-files`, one shipped dependency (`libxml2-wasm`, MIT, loaded with the first check) and two W3C schema files. No migration, no SPI method, no `WriteOp`, no WIT change.
- Opening one file may now cause up to 400 reads of the same repository. They are the reads *Files* already makes, at the same commit, through the same port.
- The same commit always draws the same set, except where a file comes from another repository, whose head moves, or from an address, which was downloaded on a day. Each such row says so.
- knobas now asks servers that are neither sources nor a request a person composed: an address a mirrored file names. The guard is the press, the one `GET`, and the row that says what was kept.
- The witnesses are `xsd-locate.test.ts` (the order, the namespace rule, one other repository taken and two asked, no address joined as a path), `xsd-index.test.ts` (no read when every reference is beside its file), `xsd-check.test.ts` (the checker asked only for files of the set), `xsd-witness.test.ts` (every example of every element of the fixtures passes libxml2), and `tests/it/` for `xsd_fetch` and `image_fetch` (nothing sent when a file opens, one request on the press, no cookie, the bounds).
