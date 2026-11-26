# Docs sources: the wiki role

A source that claims the **wiki** role fills knobas' Docs view: a page tree per space, a reader that draws each page, comments, versions, labels, section edits and new pages. knobas draws all of it from one shared vocabulary and nothing source-specific, so a page from your wiki reads like a page from Confluence. The decision is [ADR-0040](../adr/0040-a-docs-source-projects-at-read-time-through-a-shared-reader.md); this page is how to meet it.

The worked example is the XWiki adapter's [`adapter/src/docs.rs`](https://github.com/BFoerschner/knobas-adapter-xwiki/blob/main/adapter/src/docs.rs).

## The claim

| The wiki role needs | Declared as |
|---|---|
| a kind `page` | `entity_kinds` |
| its space | `project_key` (and `project_name`) on `page` |
| its parent | `parent` on `page`, the chain outermost first, each answering to its title |
| the projection | `RoleClaim::new(WIKI, ["doc_content"])`, and the `docs` export |
| optional writes | `create_page`, `update_page`, `comment` |

Install refuses a wiki claim from a component that does not export `docs`. With the Rust SDK, exporting it is implementing `Adapter::project`; the `docs` interface is always in the SDK's world, so the static check that matters is that your projection answers.

## The projection

```rust
fn project(kind: &str, payload: &serde_json::Value) -> Option<DocContent>
```

knobas calls it **every time a page is opened**, and once per `page` during sync to build its search text. Nothing it answers is stored, so a better projection in a new release redraws every mirrored page without a re-sync. The rules follow from that:

- **A plain function of the payload.** No instance, no network: the host refuses every request a `docs` call sends, whatever the manifest says. Whatever the page draws must be in the payload when the sync writes it. The XWiki adapter's payload carries the rendered page, the comments, the tags, the display names of everyone it mentions and the titles of every synced page it links to.
- **Fast.** At most 5 s per call, under the adapter's memory cap. One that traps, runs out either or answers JSON that does not decode is no content, and the page draws as the source's failure.
- **`None` for any kind but `page`**, and for a payload that is not one of your pages. The docs contract asks with a kind no descriptor declares.
- **knobas fills what the mirror knows.** Leave empty, and expect cleared: an include's `entity_id`, `blocks` and `unfilled`; a page link's and a ticket's `entity_id`; a ticket's title and status; a mention's `me`.
- **A URL embed only through `EmbedContent::url`**, which refuses anything but an absolute `http(s)` URL. Draw anything else as a placeholder.

### What `DocContent` holds

| Field | What |
|---|---|
| `blocks` | the page body |
| `comments` | oldest first: id, author account, display name, RFC 3339 time, blocks |
| `version` | the source's version number of this revision, or `None` |
| `editor_name` | who made it, by display name |
| `labels` | the page's labels in the source's order |
| `excerpt` | the blocks an excerpt include draws, or `None` |
| `sections` | one per top-level heading of `blocks` ([Sections](#sections)) |
| `history` | newest first, at least this revision; the oldest is read for *Created by* |

### The vocabulary

Blocks: `heading`, `paragraph`, `list` (items may be tasks), `code`, `table` (cells hold blocks and span rows and columns), `panel` (info, note, warning, tip), `collapsible`, `include`, `embed` (HTML to run in a sandboxed frame, or a URL to frame), `placeholder`. Inlines: `text` with strong, emphasis and code marks, `link`, `page_link`, `issue`, `mention`, `break`. Every string is source text: knobas never renders it as markup, except an embed's HTML, which it runs and never draws in its own document. The types are `knobas_source::docs` (`knobas_adapter_sdk::spi::docs`); adding to them is a frozen-surface change (ADR-0040).

A part you cannot draw is a `placeholder` named after it, never dropped, so the reader learns the page has something knobas did not draw.

## The shared HTML reader

Most wikis can give you HTML. Do not write an HTML parser: the SDK carries the one knobas reads Confluence with, `knobas_adapter_sdk::html`.

- `html::parse(markup)` reads markup into a tolerant tree of `Node`s and never fails: an unmatched close tag closes to its opener or is ignored, implied end tags close as a browser closes them, comments are dropped.
- `html::blocks(&nodes, &hook)` (or `html::to_blocks(markup, &hook)`) reads the tree into blocks. It knows plain HTML: headings, paragraphs, lists, tables with spans, `pre`, links, text marks, line breaks. `div`, `section`, `span` and the other wrappers read as their children.
- Every element it does not know goes to your **hook**:

```rust
impl Hook for XWiki<'_> {
    fn element(&self, element: &Node, reading: &Reading<'_>) -> Hooked {
        match element.name().unwrap_or_default() {
            "xw:macro" => self.macro_(element, reading),
            "xw:link" => self.link(element, reading),
            "xw:image" | "img" => self.image(element),
            _ => Hooked::Decline,
        }
    }
}
```

`Hooked::Blocks` puts blocks where the element stood (inside a sentence, the sentence breaks around them), `Hooked::Inlines` puts inlines in the sentence, `Hooked::Unwrap` reads the element's children in its place, and `Hooked::Decline` draws a placeholder named after the element. `reading.blocks(children)` reads nested content back through the reader with your hook, and `reading.text(..)` makes text under the marks in force.

Two techniques from the XWiki adapter carry over to most wikis:

**Make the source's structure into elements first.** The reader drops comments, and a hook sees only elements it does not know. XWiki's annotated HTML keeps its macros as comment pairs, `<!--startmacro:info|-|title="Careful"|-|…-->…<!--stopmacro-->`, around plain `div`s the reader would read straight through. `annotated::elements` turns each pair into `<xw:macro name params content>…</xw:macro>` before parsing, so the hook maps an info macro to a panel and a code macro to a code block with the macro's own source as its text, not the highlighted spans it rendered to.

**Rewrite the tree when a known element means more.** XWiki marks monospace as `<span class="monospace">`, which the reader would unwrap to plain text. The adapter renames those spans to `code` in the parsed tree before `html::blocks` reads it. `Node::Element`'s fields are public for this.

## Includes and page links

knobas resolves an `include` and a `page_link` **at read time, from the mirror**, by the target's title and space key: the page of this source whose declared `project_key` is `space` and whose title is `title`. It draws an include three levels deep at most and names a cycle. So the projection must give a title and a space key, not the source's own reference.

A projection has no network to look a reference up. Resolve references at sync instead and keep the answer in the payload. The XWiki adapter reads every link and include in a rendered page, takes what XWiki resolved each to (the rendered address, an include's `data-xwiki-source`), and stores the synced pages among them by full name with their title and top-level space. A link to a page outside the synced spaces stays a link to the wiki; a link to a page that does not exist is its words.

Do not draw the include's target yourself, even when the source rendered it inline. knobas fills it from the mirror as it is now, and a target you drew would be the version your page was synced with.

## Search text

For a `page`, knobas builds the search text from your projection with one shared function: title and lead text, a line per section heading, a line per comment, link lines last, the words of tables, collapsibles and an embed's visible text, never script or style. The `body_text` your sync sends for a `page` is ignored. A better projection reindexes nothing until a page is synced again.

## Sections

The reader numbers a page's sections by its **top-level heading blocks**: the first heading opens section 1. `DocContent.sections` must match: one entry per top-level heading, numbered from 1 in order, each with either the editor's text for it or a refusal (`table`, `code`, `macro`, `formatting`). A section is what follows its heading up to the next top-level heading.

When your body and your edits come from different representations, keep them aligned. The XWiki adapter draws from the rendered HTML and edits the XWiki 2.1 source, because an edit must send back the whole source with one section replaced. It finds the source's top-level headings (outside every macro and group), compares them with the drawn ones, and when the two disagree (a heading a script printed, one inside a group) it refuses every section rather than edit the wrong one. Headings inside a macro's rendered output are drawn as bold paragraphs, so they open no section.

The editor's text is plain: a paragraph per block, a blank line between blocks, `- ` per item of a flat list. Refuse any section that text cannot hold without loss. The XWiki adapter also refuses a paragraph that holds a line break, since XWiki keeps a single newline as one.

## The docs writes

```rust
fn edit_section(kind: &str, payload: &Value, number: u32, text: &str) -> Result<SectionEdit, EditRefused>
fn body_from_text(text: &str) -> String
```

Both run as `project` does: no instance, no network, 5 s.

- `edit_section` answers the page's **whole** new body in your markup, section `number` replaced by `text` and every other byte kept, with `base_version` the version the payload holds. knobas queues it as `update_page`, which your `write` performs. Refuse a section the projection refuses, for the same reason, and answer `EditRefused::Missing` for section 0, one past the last, another kind, or a payload with no body or no version.
- `body_from_text` answers a new page's body in your markup from the editor's text: `## ` starts a section, `- ` a list item, a blank line a paragraph. knobas queues it as `create_page`. The SDK's default, `html_from_text`, is escaped HTML; a wiki whose markup is not HTML overrides it.

## The docs contract

`knobas_wasm_host::docs_contract::check(&adapter, &descriptor, &items)`, over the items your full sync handed over, holds all of this together. It fails naming the first breach:

1. a wiki claim comes with the `docs` export;
2. the sync emits a page, and every page is drawn;
3. no other kind is drawn, nor a page's payload asked as a kind nobody declares;
4. sections are numbered 1, 2, … and each has its text or its refusal;
5. with `update_page` declared: a refused section is refused by the edit for the same reason, an editable one is edited against the version drawn, and sections 0, one past the last and another kind's are missing;
6. with `create_page` declared: a new page's body is not empty.

[testing.md](testing.md) runs it against a live instance.
