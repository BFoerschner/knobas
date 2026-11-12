# Building an external adapter

An **external adapter** is how a system knobas has no built-in adapter for gets into knobas: one WebAssembly component, published as a release on a forge, that a reader installs from inside the app. People looking for this often search for "knobas plugins"; the glossary's word is external adapter ([`CONTEXT.md`](../../CONTEXT.md#external-adapter)), and these pages use it.

The component runs in a sandbox with no disk, no environment and no keychain. Its one way out is HTTP to the hosts its manifest declares. knobas attaches the reader's credential to those requests itself, so the component never holds a secret ([ADR-0028](../adr/0028-an-external-adapter-is-a-sandboxed-wasm-component-installed-from-a-release.md)). Inside the sandbox it is an ordinary source: it syncs items into the mirror, answers live reads, performs writes, and fills the [roles](../../CONTEXT.md#role) it claims, so its items appear on the same surfaces as Jira's or Confluence's.

The worked example is [BFoerschner/knobas-adapter-xwiki](https://github.com/BFoerschner/knobas-adapter-xwiki), an XWiki adapter that fills the wiki role, writes back, and is checked against a real XWiki. Every guide below points into it.

## Reading order

| Page | Read it when |
|---|---|
| [quickstart.md](quickstart.md) | you start: a crate, a component, a manifest, an install |
| [concepts.md](concepts.md) | a word on another page means nothing to you yet |
| [sync.md](sync.md) | you write `sync` and `refresh`: items, cursors, deletions, errors |
| [http.md](http.md) | you write the first request |
| [writes.md](writes.md) | the adapter changes things in the source |
| [docs-sources.md](docs-sources.md) | the adapter claims the wiki role |
| [testing.md](testing.md) | before every release: the checks knobas runs, and how to run them first |
| [release.md](release.md) | you publish: the manifest, the release, install and update |
| [checklist.md](checklist.md) | before you tag, and when an install is refused |

Two reference pages hold the exact shapes:

- [reference/interface.md](reference/interface.md): the `knobas:adapter` WIT world, version 1.5.0, every function and every JSON type.
- [reference/descriptor.md](reference/descriptor.md): everything an adapter declares about itself, field by field, with the role minimums.

## What knobas gives you, and what it does not

| You get | From |
|---|---|
| The WIT world `knobas:adapter@1.5.0` | `crates/knobas-wasm-host/wit/adapter.wit` |
| A Rust SDK: bindings, the SPI types, one trait to implement, the export macro | `crates/knobas-adapter-sdk` |
| The shared HTML reader, for docs sources | `knobas_adapter_sdk::html` |
| The checks: install's static checks, the contract battery, the role data check, the docs contract | `knobas-wasm-host`, `knobas-source` |
| A reference component the host is tested with | `crates/knobas-wasm-fixture` |

There is no `knobas adapter check` command (#2029 was closed as not planned). The same checks run from a small host-side crate in your own repository; [testing.md](testing.md) shows it. Only the Rust SDK is maintained. Another language works if it builds a `wasm32-wasip2` component against the WIT and writes the JSON in the exact serde form ([reference/interface.md](reference/interface.md#building-without-the-rust-sdk)).

knobas cannot certify an external adapter against its real product, as [ADR-0013](../adr/0013-the-real-instance-is-the-witness-a-mock-certifies-nothing.md) does for the built-in ones. The checks prove the contract holds. That the adapter reads its product correctly is the author's claim, and the author's tests against a real instance are what back it.
