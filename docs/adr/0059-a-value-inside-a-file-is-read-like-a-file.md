---
status: accepted
---

# A value inside a file is read like a file

Stands beside [ADR-0032](0032-the-app-keeps-live-reads-in-memory-keyed-by-what-they-stand-at.md) and [ADR-0033](0033-renderer-and-pipeline-commands-run-under-the-rules-of-open-in.md), and extends the file pipelines of spec #2234. Björn, on PR #2479, about a base64 attachment inside an XML file: "maybe add the ability to render fields per pipeline, like in general? … maybe as a rightclick context? or a button." A file pipeline stood between a whole file's bytes and its renderer. A document holds files of its own: an attachment in an order, a token, a document escaped inside another. Decided 2026-11-23 (Fable, xhigh, under Björn's standing instruction; `docs/decisions/2026-11-23-an-xml-file-is-read-as-a-document.md`): **a value of a file goes through the file pipelines and is drawn by any renderer; it gets there by a step that takes it out of the file, in the backend; and what it decodes to is kept in memory and nowhere else.**

- **One pipeline.** A value pipeline is a file pipeline whose first step, `Take`, cuts the value out of the file's bytes and reads XML's references or JSON's escapes. So it runs where the steps run, under their caps (200 MiB a step, 10 s), and is kept as they are, by (sha, path, steps). The web view sends a byte range, never the value, and never a program.
- **Any renderer, and no renderer's own.** The value viewer is the file viewer with the pipeline bar. A renderer that can name a byte range of its file can open it. What came out is drawn by the renderer picked, under that renderer's rules: the sanitizer, the frame that runs no script, pdf.js with scripting off.
- **A guess is said and can be changed.** knobas offers a first chain where the file or its schema says what a value is, or where the value's decoded start is something it knows. It never decodes a value nobody looked at, and never without showing the chain.
- **Kept for a field by a person.** A chain can be kept under a field's name (how its names end) and a file pattern, in the settings row the named pipelines live in. A backup carries it and a share export does not.
- **Nothing decoded rests.** What a chain gave is a live read in the app process, bounded and dropped at quit: in no table, no index, no log, no backup. It reaches the disk only where a person presses *Download*.

## Rejected

- **Decoding in the web view.** Megabytes of somebody else's bytes in the page's heap, a second set of steps to keep in step with the first, and no cap.
- **The value sent to the backend.** The file is already there.
- **A viewer of its own for each renderer.** The XML view's base64 dialog, then the JSON view's, then the API explorer's.
- **Decoding by itself**, a thumbnail of every attachment a file holds. Work and content nobody asked for.

## Consequences

- Eight more variants of `PipeStep`, and `encoding_rs` named directly (the lock holds it already). No migration, no SPI method, no `WriteOp`, no WIT change.
- One command, `entity_file_piped_save`: *Download* saves what a chain gave to the Downloads folder, the run read again where it is kept, so the decoded bytes never cross to the web view and back (the build, 2026-11-23).
- A named pipeline gains `fields`, the fields it is kept for; a pipeline kept for a field may have no step (an XML string is drawn by the XML view as it is). The row is the named pipelines' own, so a backup carries it and a share export, which holds no setting row, does not.
- `Take`'s places count from after a byte-order mark: the web view reads the file's text without one.
- A file pipeline can now be run on part of a file, by anything that can name the part.
- The witnesses are `file_pipeline`'s unit tests (each step, `Take`'s bounds), `tests/it/file_pipeline.rs` (kept the second time, nothing on disk after a decode), `value-guess.test.ts` (the look-alikes) and `TfValueViewer.test.svelte.ts`.
