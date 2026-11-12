# Writes

An adapter that changes things in its source declares each change it performs, and knobas offers exactly those. A write is queued, sent when the source answers, and read back once it lands. What your adapter answers decides each of those steps.

## Declaring

```rust
capabilities: vec![Capability::Write],
write_ops: vec!["comment".into(), "update_page".into(), "create_page".into()],
```

`write_ops` names `WriteOp` variants by their identifiers, in the order the action bar draws them. `Capability::Write` and a non-empty `write_ops` go together: one without the other fails battery clause 5. The full list of ops and their fields is in [reference/descriptor.md](reference/descriptor.md#write_ops). An op is one per operation, never per adapter: a reply on a page is `comment`, the same op as a reply on a ticket.

A role lists the writes it may offer. The wiki's are `create_page`, `update_page` and `comment`; each is optional and declared on its own.

A write outside every role's vocabulary is a **source action**: declare `source_action` in `write_ops`, your action ids in `source_actions`, and offer them from your `view` with the dialog fields they need. knobas queues it without knowing what it does and hands `WriteOp::SourceAction { entity, id, fields }` back to you. The fixture component's `book_repair` is the example (`crates/knobas-wasm-fixture/src/lib.rs`).

## What reaches `write`

Only declared ops. The write queue and the WASM host both check `SourceDescriptor::declares` before a call crosses into the component, and refuse anything else by name. Still refuse an undeclared op yourself with `SourceError::Protocol`; it is the defence behind theirs, and clause 5 calls `write` with every op you did not declare.

`write` runs with the same HTTP rules and deadline as `sync`. The target is an entity id of your own source, usually `entity`; `CreatePage` names its container `parent`.

```rust
fn write(&self, op: WriteOp) -> Result<WriteReceipt, SourceError> {
    match op {
        WriteOp::UpdatePage { entity, base_version, body } => self.update_page(&entity, base_version, &body),
        WriteOp::CreatePage { parent, title, body, .. } => self.create_page(&parent, &title, &body),
        WriteOp::Comment { entity, body } => self.comment(&entity, &body),
        other => Err(SourceError::protocol(format!("the xwiki adapter does not perform {}", other.identifier()))),
    }
}
```

## Receipts

Answer `WriteReceipt::none()` unless the write made something that reading the target back cannot identify:

- a create (`create_page`, `create_ticket`, …): `WriteReceipt::id(<key>)`, the **key** of the new item's entity id, without the source id. knobas reads `<source id>:<key>` back with a refresh.
- something inside an entity, such as a comment's id: `WriteReceipt::id(<its id>)`. knobas does not read it as an entity.

The XWiki adapter answers a new page's full name for `create_page` and the comment's number for `comment`.

## What your error means to the queue

| Answer | What the queue does |
|---|---|
| `Ok(receipt)` | the write landed; knobas refreshes the item, then runs the source |
| `Unreachable` | nothing happened; sent again on a backoff |
| `Unconfirmed` | it may have landed; held, and a person checks the source and sends it again or discards it |
| `Moved { base, current }` | the target changed since the write was made against it; held with both, so the reader compares and chooses |
| `Unauthorized` | the credential failed; the source needs a new one |
| `Protocol` | refused, with your message |

Two of these need care.

**Unconfirmed, not Unreachable, for a write that left.** Sending a comment twice posts it twice. Once the request is out, a timeout or a dropped connection is `Unconfirmed`. Only a connection that never opened is `Unreachable`. The XWiki adapter's `http::send` decides by the `wasi:http` error code and the method ([http.md](http.md#errors-from-the-transport)).

**Moved, for an edit against an old version.** `UpdatePage { base_version }` is the version the reader's edit was made against, from your projection's `version`. Read the target's current version before writing. When it differs, answer `Moved { base: <the version the source holds now>, current: <its body at that version> }` and write nothing. *Apply anyway* re-sends the edit against `base`, so spell `base` the way `base_version` can be read back. The XWiki adapter compares XWiki's major version; its REST API has no conditional write, so a change landing between its read and its `PUT` is not caught.

## Writing what the reader typed

A reader types plain text, and your source stores markup. Escape the text so nothing typed is read as markup: the XWiki adapter's `markup::escape` puts `~` before every character pair that starts XWiki 2.1 markup (`**`, `//`, `[[`, `{{`, …) and before a line-start `*`, `=` or `|`, and a test holds that typed text round-trips through the markup back to the same text. A docs source's section edits and new page bodies come through the docs writes, which [docs-sources.md](docs-sources.md#the-docs-writes) covers.

## Testing writes

The contract battery never performs a write you declare, because it runs against your real instance. Test them yourself, end to end, against a test instance: the XWiki adapter's `cargo run -p check -- writes` creates a page, edits a section, sends a stale edit and expects `Moved`, adds a comment, and reads all of it back through a refresh. [testing.md](testing.md).
