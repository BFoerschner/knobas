# Concepts

The words knobas uses for what an adapter does, each with the place it is defined. [`CONTEXT.md`](../../CONTEXT.md) is the canonical glossary; this page is the part of it an adapter author needs, in the order you meet it.

## The adapter and its sources

**Adapter kind.** What you build: one component, one `adapter_kind` (`xwiki`). A slug, constant across instances, and unique in a knobas: install refuses a kind a built-in or an installed adapter already has.

**Source.** One configured instance of your adapter: a base URL, a credential, a config object. A reader may add several sources of one kind. Each has an id the reader picks (`tidewiki`), and that id is the namespace of every entity the source emits.

**Instance.** What the host hands your component for one call: the source's id, kind, display name, base URL and config. Never the secret. The host makes a new component instance for every call, so nothing survives between calls but the cursor.

**Descriptor.** What your adapter says about itself: kinds, declared paths, roles, writes, auth methods, config schema. knobas builds the Add-source form from it, checks it at install, and reads every item through it. [reference/descriptor.md](reference/descriptor.md).

## What a source emits

**Item, entity, kind.** A sync hands over items. Each is one record of a **kind** your descriptor declares (`page`), addressed by an **entity id** `<source id>:<key>`. The key is yours: stable, unique within the source, any text after the first `:` (`tidewiki:Tidewater.Engineering.WebHome`).

**Payload.** The raw source record the item carries, in the source's own shape. Everything knobas does not normalise lives here. Readers outside your adapter reach into it only through **declared paths**.

**Declared path.** Where your payloads keep a fact knobas reads: a status, a project key, a parent chain, labels. A list of JSON object keys from the payload root. A path that finds nothing is a **miss**, and a miss draws nothing; knobas never guesses a spelling.

**Project.** The source's own grouping of items, in its own word: a Jira project, a Confluence space, an XWiki top-level space. Declared as `project_key` and `project_name` paths.

**Parent.** The item an item sits directly inside, which you can open: a parent page, a repository, an epic. Declared as a path to one object or to the whole chain, outermost first.

**Mirror.** knobas' local copy of every source's items. Search, the launcher, rooms and inboxes read the mirror, not your adapter.

## Roles

**Role.** A surface knobas draws for every source that fills it: code host, ticket system, CI, wiki, monitoring. A role is stated as a minimum: the kinds you emit, the paths each kind declares, the reads you answer, the writes you may offer. Claim it in the descriptor and knobas checks the claim twice: statically at install, and against real records on the source's first sync. A source whose items fit no built-in role declares a **custom role** `<adapter_kind>:<name>`, whose item view your `view` read draws.

## Syncing

**Cursor.** An opaque string your `sync` returns: where the next sync starts. `None` asks for a **full sync**.

**Full sync, exhaustive kind, sweep.** A full sync of a kind declared `full_sync_exhaustive` hands over the whole current corpus of that kind, so knobas may tombstone every item of it the run did not touch. It does so only after a run it started without a cursor, which for an established source practically never happens: an adapter hands deletions over itself ([sync.md](sync.md#deletions)).

**Upserted.** The items one run wrote, new or changed. A run that hands nothing and returns the cursor it was given is idle, and knobas logs nothing for it.

**Refresh.** A read of one item by id, which knobas makes right after a write lands so the mirror shows what the write made before the next run.

## Writing

**Write op.** A change knobas asks your adapter to make, from a fixed vocabulary (`comment`, `update_page`, `create_page`, …). You declare which ones you perform; knobas offers only those and refuses any other before it reaches you. A write that belongs to no role is a **source action**, declared by your own id.

**Write queue.** knobas queues every write and sends it when the source answers. What your error says decides what happens next: sent again, held for a person, or shown as refused. [writes.md](writes.md).

## Docs sources

**Projection.** A function of one payload that draws a page in knobas' docs vocabulary: blocks, inlines, comments, sections. knobas calls it every time a page is opened, inside your component, with no network. Nothing projected is stored, so a better projection redraws the whole mirror without a re-sync. [docs-sources.md](docs-sources.md).
