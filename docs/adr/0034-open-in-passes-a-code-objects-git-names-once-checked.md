---
status: accepted
---

# "Open in" passes a code object's git names, once checked

Amends [ADR-0016](0016-a-spawned-command-takes-no-argument-from-the-mirror.md). Spec #2234 (step 6c) offers "Open in" on a commit, a range, a pull request and a file at a line, not only on a checkout, with the placeholders `{sha}`, `{base}`, `{head}`, `{pr}`, `{branch}` and `{url}`, and a built-in *Browser* target from the source's declared web URL. Each of those values is one a remote system wrote, which ADR-0016 keeps out of every spawned command. Decided 2026-10-30 (implementer on #2274, ruling the conflict between the ticket and ADR-0016 under the spec): **a code object's git names reach an argument only after knobas holds each to a grammar that cannot be read as anything but a name, and its URL is built from the source's typed base URL, never read from the mirror.**

- `{sha}` is 7 to 64 hex digits. `{base}`, `{head}` and `{branch}` are git ref names of `[A-Za-z0-9._/-]` that start with none of `-`, `.` or `/`, hold no `..` or `//`, and do not end in `/` or `.lock`; a base may end in one `^` (the diff address's `<sha>^`). `{pr}` is a number. A value that fails is refused by name and nothing runs (`knobas_core::checkout::open::fill`).
- The check is made where a value becomes an argument, so a fork's head (`owner/repo:branch`) refuses only a command that uses it.
- `{url}` and *Browser* open the page `knobas_app::checkout::open::page` builds as [ADR-0023](0023-knobas-clones-a-repository-into-the-clones-root.md) builds a clone URL: the instance's typed base URL, the page template the adapter declares in its descriptor (`SourceDescriptor::code_pages`), and the values above, each percent-encoded so none leaves its path segment. An item's stored `web_url` is never read. *Browser* hands the page to the opener, which still takes only `http` and `https`.
- Everything else of ADR-0016 stands: the program is a template the person saved, there is no shell, nothing spawned writes to a working tree, and `{title}`, `{author}` and every other field stay unknown placeholders, refused by name.
- An application declares the objects it accepts by having a command for each: the repository command it always had, a file command (or the repository command at a file), and optional commit, range and pull request commands. The applications saved before #2274 are repository and file applications, unchanged.

## Rejected

- Keep the object values out and pass only the checkout. A commit or a range is then not something an application can be opened on, and the spec asks for exactly that.
- Pass the values unchecked, since there is no shell. Without a shell a value is still an argument, and `--upload-pack=…` or `--output=…` is an option to git and to most of the tools a person would put here.
- Open the item's stored `web_url` in the browser. It is a remote system's choice of address; building from the typed base URL is the rule ADR-0023 set for the one URL knobas already hands a program.

## Consequences

- The IPC seam witnesses the placeholders per object and the refusals (`crates/knobas-app/tests/it/checkout_ipc.rs`, `each_object_expands_its_placeholders_in_its_own_command`, `an_object_the_application_does_not_accept_or_a_refused_value_runs_nothing`, `the_browser_page_of_each_object_is_built_from_the_typed_base_url`); the grammar is pinned in `knobas_core::checkout::open`'s tests.
- An adapter that declares no `code_pages` gets no *Browser* and no `{url}`; the contract battery holds a declaration to its placeholders (clause 14).
- The desktop witness covers *Browser* opening the page as an OS-level target (spec #2234, *Testing Decisions*).
