---
status: accepted
---

# Renderer and pipeline commands run under the rules of "Open in"

Amends [ADR-0016](0016-a-spawned-command-takes-no-argument-from-the-mirror.md). Spec #2234 (step 5) lets a person draw formats knobas doesn't know with a program of their own, such as `plantuml -tsvg -pipe`, and wrap a file in their own preprocessing steps. Those programs run on the content of a mirrored file, which ADR-0016 kept away from every spawned command. Decided 2026-10-30 (implementer on #2262, under the spec's ADR list): **a renderer or pipeline command is set up like an "Open in" application, and the content it works on reaches it on stdin only, never as an argument.**

- The program and every argument are what the person typed in Settings › Renderers, split into words with quotes as grouping only (`knobas_core::checkout::open::words`). It takes no placeholder. The webview names a saved command by its id and sends the content; it never sends a program or an argument (#1643's rule for *Try it*, so the test pane runs a saved renderer).
- **No shell.** The argument vector goes straight to `exec`: `;`, `|`, `$VAR` and backticks are literal text. A person who wants a shell types `sh -c "…"`, and then the shell's words are theirs.
- **Stdin is data.** A file's text or bytes, a Markdown fence or a pasted sample goes to the program on stdin. The rule ADR-0016 guards, that no remote system picks what runs or what it is told to do, still holds: the content is input the person's own program reads, as an editor reads a file it was opened on.
- **Limits.** Output is stdout, refused past a cap (16 MiB for a renderer) rather than cut; stderr is kept to 64 KiB. Every run has a timeout, 1 to 120 s, 10 s unless set, and is killed at it. A failure, a timeout, a program that is not there and output over the cap are answers the viewer draws (the source and what the command said), not errors.
- **Where it runs.** In the system temp directory, never in a checkout, so ADR-0016's rule that nothing spawned writes to a working tree holds; with the parent environment less every `KNOBAS_*` variable (#1643), through `knobas_db::console` (no window on Windows).
- **What comes back is untrusted.** SVG and HTML output is sanitized (DOMPurify) and drawn in a frame with `sandbox=""`: no script runs in it and its origin is opaque, so nothing it holds reaches the app. SVG is drawn inside that frame as an `<img>`, which runs nothing. An image output is an `<img>` of a `data:` URL.
- The list is one `knobas.setting` row (`render.commands`), as the *Open in…* list is: no migration.

## Rejected

- A placeholder such as `{file}` for the content. The path of a temp file is harmless, but a template language for renderers invites `{name}` and `{branch}`, each a mirrored value as an argument.
- Running the command through `sh -c` so pipes work. One `sh -c` the person types costs them nothing and keeps the shell out of every other command.
- Drawing output in the app's own DOM after sanitizing. A sanitizer bug would then be a script in the app; the sandboxed frame is the second wall.

## Consequences

- The witness is the real programs at the IPC seam (`crates/knobas-app/tests/it/renderer_commands.rs`): `cat` handing back HTML with a `<script>`, a metacharacter argument reaching `printf` literally, `sleep` stopped at a 1 s timeout, a failing `sh -c` answering its stderr. The sanitizer and the frame are witnessed in vitest (`render/command.test.ts`, `tf/render/TfCommandOutput.test.svelte.ts`).
- The frame's document inherits the app's CSP (`style-src 'self'`), so inline styles in HTML output are not applied in the bundled app. SVG keeps its styles, being an image.
- Pipeline steps (spec #2234, *Pipelines*) that are the person's commands run on these terms with their own caps.
