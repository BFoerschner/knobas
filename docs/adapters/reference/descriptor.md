# The descriptor

Everything an external adapter says about itself is one `knobas_source::SourceDescriptor`, which the component answers as JSON from `adapter.descriptor`. knobas draws the Add-source form from it, the action bar from its writes, item chips from its kinds and every role surface from its claims. Nothing in knobas keeps a table per adapter, so anything the descriptor leaves out, knobas does not know.

This page lists every field, the JSON it takes and the rules each check holds it to. How the descriptor crosses the interface is in [interface.md](interface.md#descriptor-func---json).

Source: `SourceDescriptor` and its neighbours in `crates/knobas-source/src/lib.rs`; `crates/knobas-source/src/role.rs`; `crates/knobas-spi-types/src/payload.rs`. Decisions: [ADR-0028](../../adr/0028-an-external-adapter-is-a-sandboxed-wasm-component-installed-from-a-release.md), [ADR-0029](../../adr/0029-the-shell-draws-a-role-never-a-source.md). Glossary: [`CONTEXT.md`](../../../CONTEXT.md).

## When the descriptor is read and checked

| When | What is read | What refuses it |
|---|---|---|
| Load | The component is asked twice with no network: as instance `wasm-probe`, then as an instance whose id and kind are the `adapter_kind` the first answer named. The second answer is the adapter's template. | A call that traps or passes a limit, or JSON that does not decode as a `SourceDescriptor`. |
| Install and update | The template, against the release manifest. | `adapter_kind` or `adapter_version` unlike the manifest's; a manifest host template `{key}` whose key is neither `base_url` nor a `config_schema` property; a role claim that fails `role::check`; a `wiki` claim from a component that does not export `docs`. |
| Each time a source is built | The descriptor for the configured instance. | An `id` that is not the instance's id or an `adapter_kind` that is not the instance's kind; `OAuth` as the source's auth method; `UserPassword` with no `username` in the config. |
| Each write | The descriptor the source was built with. | An op whose identifier is not in `write_ops`, or a source action whose id is not in `source_actions`. |
| A source's first sync | The declared paths of each claimed role, against the records the sync emitted. | Nothing stops the sync; the shell does not draw that role's surface for the source, and its health line names the path that missed. |
| The contract battery, in the author's own tests | The whole descriptor against the adapter's behaviour. | Each clause cited below. Install does not run the battery. |

Two consequences of the load probe. `adapter_kind` must be a constant: an adapter that echoes `instance.kind` answers `wasm-probe` to the first probe and is refused at install for disagreeing with its manifest. `id` must echo `instance.id`, so the template's id is the adapter kind. The descriptor is also asked with an empty base URL and `config` `{}`, so it must not depend on the configuration and must not fail without one.

The battery and how to run it against a component are in [testing.md](../testing.md).

Source: `from_bytes`, `describe` and `build` in `crates/knobas-wasm-host/src/lib.rs`; `check`, `check_descriptor` and `check_docs` in `crates/knobas-wasm-host/src/manifest.rs`; `WasmSource::write` in `crates/knobas-wasm-host/src/source.rs`; `credential` in `crates/knobas-wasm-host/src/net.rs`; `battery` in `crates/knobas-source/src/contract.rs`; `crates/knobas-source/src/role/data.rs`; ADR-0028, *The checks*.

## JSON conventions

Field names are the Rust names, in snake case. Fields without a default must be present, even when empty. The enums the descriptor uses serialize as follows.

| Type | JSON |
|---|---|
| `Capability` | `"Search"`, `"Write"`, `"Webhooks"`, `"Import"`, `"Refresh"` |
| `AuthMethod` | `"UserPassword"`, `"Pat"`, `"ApiToken"`, `"OAuth"` |
| `Frame` | `"pipeline"`, `"run"`, `"saved_view"` |
| `Markup` | `"text"`, `"markdown"`, `"jira_wiki"` |
| `Tone` | `"ok"`, `"warn"`, `"err"`, `"info"`, `"muted"` |
| `Trigger` | `"change"`, `"schedule"`, `"person"`, `"chain"` |
| `CloneFrom` | `"base_url"`, or `{"config": "<key>"}` |
| `PayloadPath` | an array of object keys, `["space", "key"]` |

Source: the serde attributes in `crates/knobas-source/src/lib.rs`, `crates/knobas-spi-types/src/payload.rs`, `crates/knobas-spi-types/src/tone.rs` and `crates/knobas-source/src/clone_urls.rs`.

## A worked example

A small wiki adapter, `tidewiki`: one kind, `page`, the `wiki` role, a username and password, and a configuration with a username and the spaces to sync. This JSON decodes as a `SourceDescriptor` and passes `role::check`, `url_forms::check` and the instance-id rule.

```json
{
  "id": "tidewiki",
  "adapter_kind": "tidewiki",
  "name": "Tidewiki",
  "capabilities": ["Write"],
  "adapter_version": "0.3.0",
  "auth_methods": ["UserPassword"],
  "accepts_account": false,
  "min_sync_interval_secs": 120,
  "write_ops": ["create_page", "update_page", "comment"],
  "entity_kinds": [
    {
      "id": "page",
      "label": "Page",
      "plural": "Pages",
      "monogram": "TW",
      "full_sync_exhaustive": true,
      "refreshes": true
    }
  ],
  "config_schema": {
    "type": "object",
    "properties": {
      "username": {
        "type": "string",
        "title": "Username",
        "description": "The account knobas signs in as. The password goes to the keychain."
      },
      "spaces": {
        "type": "array",
        "items": { "type": "string" },
        "title": "Spaces",
        "description": "The space keys to sync, one per line. Empty syncs every space."
      }
    },
    "required": ["username"]
  },
  "payload_paths": [
    {
      "kind": "page",
      "project_key": [["space", "key"]],
      "project_name": [["space", "name"]],
      "parent": [
        { "kind": "page", "at": ["ancestors"], "names": [["title"]] }
      ],
      "position": [["position"]]
    }
  ],
  "roles": [
    { "role": "wiki", "reads": ["doc_content"] }
  ],
  "url_forms": [
    { "kind": "page", "template": "/page/{}", "fields": [["id"]] }
  ]
}
```

What each part does:

- `id` is shown as the template answers it. For a configured source it is whatever id the person chose, such as `tidewiki-ops`.
- `Write` goes with a non-empty `write_ops`. The three ops are the wiki role's whole write vocabulary; each is optional.
- `UserPassword` makes the host send `Authorization: Basic` built from the config's `username` and the stored password. The schema therefore asks for `username`, and never for the password.
- `spaces` is an array of strings, which the Add-source form draws as one value per line.
- The `page` kind declares `project_key` and `parent`, the two paths the wiki role requires. `parent` reads the page's `ancestors` array, outermost first, so `under:` is transitive.
- The `wiki` claim names `doc_content`, its one required read, which the component answers by exporting `docs`. Install refuses the claim without that export.
- `url_forms` lets a pasted `https://wiki.example.org/page/4711` open the page whose payload `id` is `"4711"`, on a source whose base URL is `https://wiki.example.org`.
- `Refresh` is absent. Declaring it needs a `refresh` export that answers for `page`.

Every field this example leaves out takes its default. The fields of a page's `payload` are the adapter's own; the declared paths say where knobas finds what it reads.

## The fields

| Field | JSON | Required | Default |
|---|---|---|---|
| `id` | string | yes | |
| `adapter_kind` | string | yes | |
| `name` | string | yes | |
| `capabilities` | array of `Capability` | yes | |
| `adapter_version` | string | yes | |
| `auth_methods` | array of `AuthMethod` | yes | |
| `accepts_account` | boolean | no | `false` |
| `min_sync_interval_secs` | integer | no | `60` |
| `write_ops` | array of string | yes | |
| `entity_kinds` | array of `KindInfo` | yes | |
| `config_schema` | JSON Schema object | yes | |
| `payload_paths` | array of `KindPaths` | no | `[]` |
| `roles` | array of `RoleClaim` | no | `[]` |
| `source_actions` | array of string | no | `[]` |
| `clone_from` | array of `CloneFrom` | no | `[]` |
| `url_forms` | array of `UrlForm` | no | `[]` |
| `code_pages` | `CodePages` object | no | absent |

### `id`

The instance id, and the `EntityRef` namespace of every item the instance emits. Echo `instance.id`. The template's `id` is the adapter kind.

The host refuses a build whose descriptor names another id. The battery, clause 1, requires a non-blank id that is no reserved namespace and passes the [instance-id rule](#instance-ids), and every emitted item's namespace to be this id.

### `adapter_kind`

Which adapter this is, a constant of the component. It must pass the [instance-id rule](#instance-ids), since the manifest's `adapter_kind` is checked with it, and it must equal the manifest's `adapter_kind`. Install refuses a disagreement with `the descriptor names adapter_kind "<descriptor>", and the manifest "<manifest>"`.

### `name`

The human-readable name for the source list. No check reads it.

### `capabilities`

| Value | Rule |
|---|---|
| `Search` | Reserved. The SPI has no server-side search, and nothing outside tests reads it. |
| `Write` | Declared exactly when `write_ops` is non-empty. The battery, clause 5, refuses either direction alone: `Write` with no ops leaves the UI nothing to offer, ops without `Write` make the source read as read-only. |
| `Webhooks` | Nothing outside tests reads it. |
| `Import` | Nothing outside tests reads it. |
| `Refresh` | Declared exactly when `refresh` answers something other than `not-offered` for some kind the sync emits. The battery, clause 13, holds both directions. The host removes it from the descriptor of a component that does not export `refresh`. Which kinds it covers is each kind's `refreshes`. |

The app reads `Refresh` to choose between a refresh and a run after a landed write, and between *Refresh* and *Sync now* on an item's detail.

### `adapter_version`

The adapter's own version, a string. It must equal the manifest's `adapter_version`, which the manifest refuses when blank. Nothing checks its form.

### `auth_methods`

The ways the adapter can authenticate. The Add-source form offers them; the chosen method's secret goes to the OS keychain and never into the configuration. The host attaches the credential itself, only to requests whose scheme, host and port are the source's base URL, replacing any `Authorization` the component set.

| Method | What the host sends |
|---|---|
| `Pat` | `Authorization: Bearer <secret>` |
| `ApiToken` | `Authorization: Bearer <secret>` |
| `UserPassword` | `Authorization: Basic` of `<config.username>:<secret>`. A config without a non-empty string `username` refuses the build with `user + password authentication needs a username in the source configuration`. |
| `OAuth` | Nothing. The build is refused with `OAuth is not supported for an external adapter; use a token or user + password`. |

A source with no auth method or no stored secret sends no credential. More in [http.md](../http.md).

Source: `credential` and the module docs in `crates/knobas-wasm-host/src/net.rs`.

### `accepts_account`

Whether the adapter can use a second credential, a username and password stored beside the main secret. For an external adapter, leave it `false`: the account never reaches the component, since the `instance` record carries none, and the host attaches no account to any request.

Source: `convert::instance` in `crates/knobas-wasm-host/src/convert.rs`, `credential` in `crates/knobas-wasm-host/src/net.rs`.

### `min_sync_interval_secs`

The shortest sync interval in seconds a source of this adapter may be scheduled at. The source commands refuse an interval under it, and the settings UI offers nothing under it. Absent is 60, `DEFAULT_MIN_SYNC_INTERVAL_SECS`.

### `write_ops`

The writes the adapter performs, as `WriteOp` identifiers. The UI draws its actions from this list. The host refuses any op whose identifier is not listed before it reaches the component. The battery, clause 5, refuses an identifier the SPI does not know and checks that every undeclared op is refused with `Protocol`. A role's writes are declared here one by one, like every write.

A `WriteOp` crosses as JSON with the variant name as its one key, for example `{"UpdatePage":{"entity":"tidewiki:4711","base_version":7,"body":"..."}}`. The target is the field the queue orders against and holds on. A field marked `?` decodes with a default when absent. The host writes every field of the op it sends, `null` for an optional value it does not have, except `message`, which it leaves out when there is none.

| Variant | Identifier | Fields | Target | Receipt id is an entity |
|---|---|---|---|---|
| `Comment` | `comment` | `entity`, `body` | `entity` | no |
| `Transition` | `transition` | `entity`, `status`: the status picked, in the source's spelling | `entity` | no |
| `CreateTicket` | `create_ticket` | `entity`: the project, `title`, `body`, `ticket_type` | `entity` | yes |
| `CreateBranch` | `create_branch` | `entity`: the repository, `name`, `from_ref` | `entity` | no |
| `CreatePullRequest` | `create_pull_request` | `entity`: the repository, `title`, `body`, `head`, `base` | `entity` | yes |
| `Approve` | `approve` | `entity`, `body`, which may be empty | `entity` | no |
| `TriggerBuild` | `trigger_build` | `entity`: the build configuration | `entity` | yes |
| `RerunBuild` | `rerun_build` | `entity`: the build | `entity` | yes |
| `LogWork` | `log_work` | `entity`: the ticket, `started`: RFC 3339 UTC, `seconds`: integer worked, `comment` | `entity` | no |
| `CreatePage` | `create_page` | `parent`: a mirrored page, `space`: the space key, `title`, `body` in the source's markup | `parent` | yes |
| `UpdatePage` | `update_page` | `entity`: the page, `base_version`: integer the edit was made against, `body`: the whole new body | `entity` | no |
| `PauseMonitor` | `pause_monitor` | `entity`: the monitor | `entity` | no |
| `ResumeMonitor` | `resume_monitor` | `entity`: the monitor | `entity` | no |
| `CreateMonitor` | `create_monitor` | `entity`: `<source id>:monitors`, `name`, `url`, `kind?`: `http`, `tcp`, `ping` or `docker`, absent is `http` | `entity` | yes |
| `MergePullRequest` | `merge_pull_request` | `entity`, `style`: `merge`, `rebase` or `squash`, `delete_branch?`: absent is `false`, `message?` | `entity` | no |
| `SetParameter` | `set_parameter` | `entity`: the build configuration, `name`, `value`, `base`: string or `null` | `entity` | no |
| `RunBuild` | `run_build` | `entity`: the build configuration, `branch`: string or `null`, `parameters`: `[{"name", "value"}]`, `clean_checkout?`, `first_in_queue?`: absent is `false` | `entity` | yes |
| `SubmitReview` | `submit_review` | `entity`, `verdict`: `approve`, `request_changes` or `comment`, `body`, `comments?`: review comments | `entity` | no |
| `DeleteBranch` | `delete_branch` | `entity`: the branch | `entity` | no |
| `EditPullRequest` | `edit_pull_request` | `entity`, `title?`, `body?`, `state?`: `open` or `closed`; only the fields set are written | `entity` | no |
| `UpdatePullRequestBranch` | `update_pull_request_branch` | `entity`, `style`: `merge` or `rebase` | `entity` | no |
| `CommitFile` | `commit_file` | `repo`: the repository, `branch`, `new_branch?`, `path`, `content`: the whole new text, `message`, `base_sha` | `repo` | yes |
| `SourceAction` | `source_action` | `entity`, `id`: the adapter's own action id, `fields?`: string to string | `entity` | no |
| `ReplyToThread` | `reply_to_thread` | `entity`: the pull request, `thread`, `comment`: a review comment | `entity` | no |
| `ResolveThread` | `resolve_thread` | `entity`: the pull request, `thread`, `resolved`: boolean | `entity` | no |
| `RequestReviewers` | `request_reviewers` | `entity`: the pull request, `reviewers`: array of logins | `entity` | no |

A review comment is `{"path": string, "line": integer, "side": "old" or "new", "body": string}`. Every target is an `EntityRef` string. For a create, the target is the container the new thing goes into. "Receipt id is an entity" says whether a `remote_id` in the write receipt is the local part of the created item's entity id, which is what knobas refreshes after the write lands. How to perform each op, and what to answer, is in [writes.md](../writes.md).

The table in the doc comment on `WriteOp` lists fewer variants than the enum has. `WriteOp::identifier` is the complete list.

Source: `WriteOp`, `WriteOp::identifier`, `WriteOp::delivery`, `WriteOp::target`, `WriteOp::receipt_names_an_entity`, `SourceDescriptor::declares` and `monitor_target` in `crates/knobas-source/src/lib.rs`; `known_write_ops` and clause 5 of `battery` in `crates/knobas-source/src/contract.rs`.

### `entity_kinds`

The kinds of item the adapter emits, each a `KindInfo`. The UI draws a new source's launcher groups, chips and monograms from this alone. Every emitted item's `kind` must be one of these ids; the battery, clause 1, refuses an undeclared kind.

| Field | JSON | Meaning |
|---|---|---|
| `id` | string | The kind, as `sync-item.kind` names it: `page`, `ticket`. |
| `label` | string | The singular label. |
| `plural` | string | The plural label. |
| `monogram` | string | The two-character monogram on the item chip. |
| `full_sync_exhaustive` | boolean, required | Whether a sync with no cursor emits the complete current corpus of this kind. |
| `refreshes` | boolean, default `true` | Whether `refresh` reads an item of this kind by id. Read only when the descriptor declares `Refresh`. |

`full_sync_exhaustive` is what licenses the sweep. After an exhaustive full sync, the engine tombstones every row of this source and this kind whose `synced_at` predates the run, which is the only way a hard delete upstream reaches knobas. Declare `false` for a kind whose full sync is a window, such as the newest N builds, or every older one is tombstoned on every run. A kind whose sync is budgeted is non-exhaustive. No check can verify the flag; only the adapter's own tests against its source hold it honest.

`refreshes` is per kind. The battery, clause 13, fails a kind declared refreshed whose refresh answers `not-offered`, and a kind declared not refreshed whose refresh answers anything else.

The wiki role requires a kind whose id is exactly `page`. The Docs frame draws only `page`, and the host builds search text from the projection only for `page` items.

Source: `KindInfo` in `crates/knobas-source/src/lib.rs`; `check_refresh` in `crates/knobas-source/src/contract.rs`; `PAGE_KIND` in `crates/knobas-source/src/docs.rs`.

### `config_schema`

A JSON Schema for the source's non-secret configuration. The Add-source form is generated from it, and what the person enters is stored as `source_config.config` and handed to the component as `instance.config`. It never holds a secret: the secret lives in the OS keychain, keyed by the chosen auth method.

The form reads a subset of JSON Schema. The schema must be `"type": "object"` with `properties`, or the form has no fields. Each property, in declaration order, is drawn by its `type`:

| Property | Drawn as |
|---|---|
| `"enum"` of strings | a select |
| `"type": "string"` | a text field |
| `"type": "integer"` or `"number"` | a number field, with `minimum` and `maximum` |
| `"type": "boolean"` | a toggle |
| `"type": "array"` with `"items": {"type": "string"}` and no `enum` on the items | a list, one value per line |
| anything else | a JSON text area |

`title` is the label, or the key humanised; `description` is the help line; `default` fills an untouched field; the schema's `required` array marks required fields. A type that is a list with one named member and `"null"`, such as `["string", "null"]`, is drawn as that member.

A property whose name ends in a word that means a credential makes the form throw instead of drawing: `password`, `passphrase`, `token`, `secret`, `credential`, `credentials`, `key` or `pat`, matched against the last word of a snake-case or camel-case name. `api_key`, `space_key` and `authToken` all throw. Rename such a property.

The host reads the schema too:

- Its `properties` names are the keys a manifest host template `{key}` may name, beside `base_url`. The host resolves a source's allowed hosts over the template's schema, so the schema must be the same for every instance.
- `UserPassword` needs a non-empty string under `username` in the config.
- A property named `clone_urls` must be an array of strings; the battery, clause 6, refuses any other type.
- A `clone_from` entry `{"config": "<key>"}` must name a property declared with `"type": "string"` exactly. `["string", "null"]` does not count.

What the adapter learns at *Test connection* goes back through `connection-info.discovered`: a map from a `config_schema` property name to a value. The Add-source dialog fills an empty field of that name with it. A key the schema does not declare fills nothing, and nothing in it may be secret, since it ends up in the stored configuration.

Source: `schemaFields`, `controlFor` and `SECRET_HEADS` in `app/src/lib/sources/schema-form.ts`; `declared_keys` in `crates/knobas-wasm-host/src/allow.rs`; `check_descriptor` in `crates/knobas-wasm-host/src/manifest.rs`; `Policy::for_source` and `credential` in `crates/knobas-wasm-host/src/net.rs`; `declared` and `config_declares` in `crates/knobas-source/src/clone_urls.rs`; `ConnectionInfo` in `crates/knobas-source/src/lib.rs`.

### `payload_paths`

Where the adapter's records keep what knobas reads but does not normalize. One `KindPaths` per entity kind that has any. A field a kind does not declare is a miss for every reader, never a guess.

#### How a path resolves

A `PayloadPath` is an array of object keys walked from the payload's root. A `null` on the way, a missing key or a non-object stops the walk with nothing. The empty path addresses the record itself, which no reader accepts. Every scalar field is a list of candidate paths, most specific first, and the first candidate that lands on a value of the right type wins. A string must be a JSON string that is not blank after trimming. A flag must be a JSON boolean; the word `"true"` is not one. A number must be a JSON number, finite and not negative; the string `"13"` is not one.

The battery, clause 6, holds a declaration to the adapter's own corpus: every `kind` names an `entity_kinds` entry, and a declared path lands on a value of its type or on nothing. Where no record of a kind resolved a path, no record may show the path naming a key that an object the source wrote does not have; that is how a typo such as `fields.priorityy.name` is caught. A kind the corpus holds none of is not judged.

#### The `KindPaths` fields

`kind` is required. Every other field defaults to empty.

| Field | Each candidate resolves to | What reads it | Role that requires it |
|---|---|---|---|
| `kind` | an `entity_kinds` id | | |
| `status_name` | string | the mini board's columns; matched by `blocked_statuses` and `done_statuses` | ticket-system `ticket`, monitoring `monitor` |
| `priority` | string | the card's priority | ticket-system `ticket` |
| `assignee` | string, the account as the source spells it | the inbox, against the configured usernames | ticket-system `ticket` |
| `reviewers` | `ListPath` to strings | the inbox's review requests | code-host `pr` |
| `merged` | boolean | the merged flag | code-host `pr` |
| `project_key` | string | the project room's scope; a page's space in docs search | ticket-system `ticket`, wiki `page` |
| `project_name` | string | the room's label | |
| `project_path` | `ProjectPath` to an array of objects | a project taking in its subprojects | |
| `blocked_statuses` | not a path: status names | which statuses mean blocked, matched ignoring case | |
| `done_statuses` | not a path: status names | which statuses mean done; a job context anchored on such an item is archived | |
| `parent` | `ParentPath` to an object or an array of objects | `under:`, the path line, the Docs tree | ticket-system `ticket`, wiki `page`, code-host `branch`, `pr` and `commit` with parent kind `repo` |
| `state` | `StatePath` to a string or a boolean its map names | the row's state chip | code-host `pr`, CI run |
| `head` | string, the branch's title | the Code tile's proposed branches | code-host `pr` |
| `commit` | string, a full object id | the builds of a commit | code-host `branch`, `pr`, `commit` |
| `revision` | `ListPath` to full object ids | the commits a build ran on | |
| `head_repo` | string | the repository a pull request comes from | |
| `default_branch` | string | the repository's default branch | code-host `branch` |
| `issue_type` | string | `issuetype:` | ticket-system `ticket` |
| `labels` | `ListPath` to strings | `label:` | ticket-system `ticket` |
| `sprint_state` | `ListPath` to `active`, `future` or `closed` | `sprint:current` | |
| `estimate` | number, seconds | the job card | |
| `reporter` | string, an account | the ticket page's *Reporter* | |
| `created` | string, an ISO 8601 instant | the ticket page | |
| `time_spent` | whole number, seconds | the ticket page's *Logged* | |
| `worklog` | `WorklogPath` to an array of objects | the reader's share of `time_spent` | |
| `comment_count` | whole number | the ticket card | |
| `description` | string | the ticket's reader, drawn by `markup` | optional for ticket-system `ticket` |
| `markup` | not a path: `text`, `markdown` or `jira_wiki`, default `text` | how `description` and comments are drawn | |
| `position` | whole number, lowest first | the Docs tree's order among siblings | |
| `frame` | not a path: `pipeline`, `run`, `saved_view` or `null` | which shell frame the kind is part of | CI pipeline and run, ticket-system saved view |
| `system_name` | not a path: string or `null` | the heading a saved view is listed under | |
| `source_query` | string | a saved view's query | ticket-system saved view |
| `board_type` | string | a saved view's board type | optional for a saved view |
| `sub_query` | string | the query a saved view narrows with | optional for a saved view |
| `sprint` | object `{"id", "name", "end_date"}` | a saved view's active sprint | optional for a saved view |
| `name` | string | a pipeline's name | CI pipeline |
| `group` | string | the group a pipeline sits in | |
| `group_path` | `ListPath` to names, outermost first | the Builds list's project tree | |
| `pipeline` | string | a pipeline's id on both a pipeline and its runs | CI pipeline and run |
| `number` | string | a run's number | CI run |
| `branch` | string | the branch a run built | CI run |
| `started` | string, an ISO 8601 instant | when a run started | CI run |
| `finished` | string, an ISO 8601 instant | when a run finished | CI run |
| `trigger` | `TriggerPath` to a string its map names | how a run was started | |
| `started_by` | string, an account | who started a run | |
| `agent` | string | the machine a run ran on | |
| `current_step` | string | what a run in flight is doing | |
| `tests_failed` | whole number | the run's test summary | |
| `tests_passed` | whole number | the run's test summary | |
| `tests_ignored` | whole number | the run's test summary | |
| `language` | string | a repository's language | |
| `teams` | `ListPath` to names | a repository's teams | |
| `owner` | string | a repository's owner | |
| `monitor_type` | string | how a monitor checks | |
| `response_time` | number, milliseconds | knobas' monitor samples, the Monitors tab | monitoring `monitor` |
| `base` | string, the branch's title | the branch a pull request is opened into | code-host `pr` |
| `attachments` | `AttachmentPath` to an array of objects | the ticket page's Attachments | |

An ISO 8601 instant may be RFC 3339, extended with a colonless offset such as `2026-08-12T10:00:00.000+0200`, or basic such as `20260822T102000+0000`.

#### The structured paths

A `ListPath` is `{"at": PayloadPath, "entry": PayloadPath}`. `at` leads to an array, and `entry` to the string inside each element; an empty `entry`, the default, means the elements are the strings. Every candidate's strings are joined, not just the first candidate's.

A `ParentPath` is `{"kind": string, "at": PayloadPath, "names": [PayloadPath]}`. `kind` is the parent's entity kind, declared because the parent need not be mirrored. `at` leads to one object, the direct parent, or to an array of objects, the whole chain outermost first with the direct parent last. An empty array is an answer: no parent. `names` are where the names the parent answers to sit inside each parent object; `under:<name>` matches them exactly, ignoring case, and the first is the one a path line shows. A mirrored parent is found by reading these same `names` off its own record, so a source must name a parent in the parent's own record shape.

```json
{"kind": "page", "at": ["ancestors"], "names": [["title"]]}
```

A `ProjectPath` is `{"at": PayloadPath, "key": [PayloadPath], "name": [PayloadPath]}`. `at` leads to an array of project objects, outermost first, ending in the item's own project. `key` and `name` are candidates inside each object; a project with no readable key is left out.

A `StatePath` is `{"at": PayloadPath, "values": [StateValue], "open"?: boolean, "text_at"?: PayloadPath}`. `at` leads to a string or a boolean, and the candidate matches only when `values` names that value; otherwise the next candidate is tried. A `StateValue` is `{"value": string or boolean, "tone"?: Tone, "text"?: string}`, matched exactly after trimming. A value with no tone still matches and draws a neutral chip. The chip's text is the value's `text`, else the string at `text_at`, else the value itself. `open: true` says the instance invents the words at this path, so the words the mirror has seen there are offered for a tone override.

```json
{"at": ["state"], "values": [
  {"value": "open", "tone": "info"},
  {"value": "closed", "tone": "muted"}
]}
```

The resolved chip is a `StateChip`, `{"text": string, "tone": Tone or null}`. A `Tone` is `ok` for done and good, `warn` for needs a look, `err` for failed, `info` for in motion or open, `muted` for over.

A `TriggerPath` is `{"at": PayloadPath, "values": [{"value": string, "trigger": Trigger}]}`, matched exactly after trimming.

A `WorklogPath` is `{"at": PayloadPath, "author": [PayloadPath], "seconds": [PayloadPath]}`. `at` leads to an array of entries; an entry whose author or whole number of seconds misses is left out.

An `AttachmentPath` is `{"at": PayloadPath, "id", "name", "mime", "size", "author", "created", "content", "thumbnail"}`, each but `at` a list of candidates inside one element. An element without a readable `id`, `name` or `content` is left out.

A kind with `"frame": "saved_view"` is a ticket view. Beside its declared paths, the frame reads fixed keys of the payload: `kind`, which is `board` or `filter`, `id`, `columns` as `[{"name", "statuses", "limit"}]`, `keys`, the tickets' keys in the source's order, and `query`.

Source: `crates/knobas-spi-types/src/payload.rs`; `needs_of` and `Shape` in `crates/knobas-source/src/role/data.rs`; `check_payload_paths` in `crates/knobas-source/src/contract.rs`; `crates/knobas-spi-types/src/tone.rs`.

### `roles`

The roles the adapter claims: which of the shell's surfaces it fills. A `RoleClaim` is `{"role": string, "reads": [string]}`; `reads` defaults to `[]` and names every read the role requires and whichever of its optional reads the adapter answers. A surface reads the claim, never the adapter kind.

`role::check` holds each claim to its role's minimum at install and in the battery, clause 10. It checks kinds, paths and reads. It does not check writes; each write is declared on its own in `write_ops`.

#### The built-in roles

| Role | Kinds and the paths each must declare | Required reads | Optional reads | Writes it may offer |
|---|---|---|---|---|
| `code-host` | `repo`: none. `branch`: `parent` of kind `repo`, `commit`, `default_branch`. `pr`: `parent` of kind `repo`, `state`, `head`, `base`, `commit`, `reviewers`, `merged`. `commit`: `parent` of kind `repo`, `commit`. | `pull_request_diff`, `commit_diff`, `repository_tree`, `file_contents`, `line_threads`, `clone_urls` | none | `create_branch`, `create_pull_request`, `comment`, `approve`, `submit_review`, `merge_pull_request`, `edit_pull_request`, `update_pull_request_branch`, `delete_branch`, `commit_file`, `reply_to_thread`, `resolve_thread`, `request_reviewers` |
| `ticket-system` | `ticket`: `status_name`, `priority`, `assignee`, `issue_type`, `labels`, `parent`, `project_key`; `description` optional. Optional kind: the first emitted kind with `frame` `saved_view`, which must declare `source_query` and may declare `board_type`, `sub_query`, `sprint`. | `reachable_transitions` | `translate_query` | `transition`, `comment`, `create_ticket`, `log_work` |
| `ci` | The first emitted kind with `frame` `pipeline`: `name`, `pipeline`. The first emitted kind with `frame` `run`: `pipeline`, `state`, `number`, `branch`, `started`, `finished`. | `run_log` | `pipeline_settings`, `older_runs`, `run_tests`, `test_history`, `run_artifacts`, `artifact_preview`, `artifact_bytes`, `download`, `run_chain` | `run_build`, `rerun_build`, `trigger_build`, `set_parameter` |
| `wiki` | `page`: `project_key`, `parent`. | `doc_content` | none | `create_page`, `update_page`, `comment` |
| `monitoring` | `monitor`: `status_name`, `response_time`. | none | none | `create_monitor`, `pause_monitor`, `resume_monitor` |

A path counts as declared when its list has at least one candidate; for `parent` of kind `repo`, at least one `ParentPath` whose `kind` is `repo`. A read is named by an identifier. Where the SPI has a method, the identifier is its name: `run_log`, `reachable_transitions`. The code-host reads are answered by the blocks of `view`, `doc_content` by the `docs` export, and `clone_urls` by `clone_from`.

An external adapter has no way to answer `artifact_bytes`: the interface carries no function for it, as [interface.md](interface.md#what-the-interface-does-not-carry) says. The claim passes the check and the read answers nothing.

On a source's first sync, knobas runs the same required paths against the emitted records. A path that misses there does not stop the sync, but the shell does not draw that role's surface for the source. An absent `parent` key is not a miss, since a ticket may have no parent; an absent `parent` of kind `repo` is.

#### Custom roles

A source whose items fit no built-in role claims a custom role named `<adapter_kind>:<name>`, where `<adapter_kind>` is the descriptor's own and `<name>` is non-empty and holds only lowercase ASCII letters, digits and `-`. A custom role has no reads: its claim names none, and its view is the `view` read, which opens when one of its items is opened. It has no surface of its own.

#### Why a claim is refused

| Message | Cause |
|---|---|
| `role "<role>" is neither a built-in role (code-host, ticket-system, ci, wiki, monitoring) nor a custom role named "<adapter_kind>:<name>"` | An unknown role, or a custom role in another adapter's namespace or with a bad name. |
| `role "<role>" is claimed twice` | The same role twice in `roles`. |
| `role "<role>" needs kind "<kind>", which descriptor.entity_kinds does not declare` | A required kind is missing. For a frame-named kind the message names the frame. |
| `role "<role>" needs kind "<kind>" to declare the path <path>` | A required path has no candidate. |
| `role "<role>" needs the read "<read>", which its claim does not declare` | A required read is missing from `reads`. |
| `role "<role>" has no read "<read>"` | A read the role does not have, or any read on a custom role. |
| `role "<role>" reads "clone_urls", and descriptor.clone_from names no address to clone from` | `clone_urls` claimed with an empty `clone_from`. |
| `descriptor.clone_from names config key "<key>", which config_schema does not declare as a string` | A `{"config": key}` entry whose property is not `"type": "string"`. |
| `descriptor.clone_from names an address to clone from, and no "code-host" claim reads "clone_urls"` | `clone_from` without the claim. |

The first refusal found wins, in claim order. At install each is prefixed `the adapter's role claims are refused: `. A `wiki` claim from a component without `docs` is refused separately: `the adapter claims the wiki role, and its component exports no docs projection (knobas:adapter/docs, 1.5.0), so knobas could draw none of its pages`.

Source: `MINIMUMS`, `check`, `check_kind`, `is_custom` and `ClaimError` in `crates/knobas-source/src/role.rs`; `crates/knobas-source/src/role/data.rs`; `Refusal` in `crates/knobas-wasm-host/src/manifest.rs`; ADR-0029.

### `source_actions`

The adapter's own ids for writes outside every role's vocabulary, such as `"add_label"`. Each is performed as a `WriteOp::SourceAction` with that `id`. The list is non-empty exactly when `write_ops` lists `source_action`. Each id is non-blank and declared once. The host refuses a source action whose id is not listed before it crosses. The battery, clause 11, holds all of these and asks the adapter to refuse an undeclared id with `Protocol`.

A source action's dialog is a view action whose `write_op` is `source_action` and whose fixed `values` hold the action's id under the key `id`; no dialog field may be named `id`. See [writes.md](../writes.md).

Source: `SourceDescriptor::source_actions`, `SOURCE_ACTION` and `SOURCE_ACTION_ID` in `crates/knobas-source/src/lib.rs`; `check_source_actions` in `crates/knobas-source/src/contract.rs`.

### `clone_from`

Which of an instance's typed addresses a repository is cloned from, in order of preference: `"base_url"`, or `{"config": "<key>"}` for the string the instance's config holds under a key. *Clone* appends the checked `owner/repo.git` to the first that ADR-0023's rules admit. It is non-empty exactly when a `code-host` claim reads `clone_urls`.

Source: `CloneFrom` in `crates/knobas-source/src/clone_urls.rs`; `check_clone_from` in `crates/knobas-source/src/role.rs`.

### `url_forms`

Other addresses the source serves an item's page under, beside its `web_url`, so a pasted address of any form opens the item. A `UrlForm` is `{"kind": string, "template": string, "fields": [PayloadPath]}`. `template` is the address after the source's base URL. `{}` stands for one value, which ends at the next `/`, `?`, `&` or `#` and is form-decoded before it is compared. `{*}` stands for the rest of the address, compared as written. `fields[i]` is where the payload holds the `i`-th placeholder's value.

The battery, clause 12, holds each form to `url_forms::check`:

- `kind` names an `entity_kinds` id;
- `template` starts with `/`, or is `{*}` alone;
- it has at least one placeholder, and exactly as many as `fields` has entries;
- `{*}` comes last;
- no two placeholders are adjacent;
- no field is the empty path.

Source: `crates/knobas-source/src/url_forms.rs`.

### `code_pages`

For a code host: where it serves the page of a repository, a file, a commit, a range and a pull request, after the instance's base URL. *Open in › Browser* and the `{url}` placeholder open these. It is an object with five templates; absent means *Browser* is not offered.

| Field | Placeholders it may carry | Gitea's |
|---|---|---|
| `repository` | `{repo}` | `/{repo}` |
| `file` | `{repo}`, `{ref}`, `{file}`, `{line}` | `/{repo}/src/{ref}/{file}#L{line}` |
| `commit` | `{repo}`, `{sha}` | `/{repo}/commit/{sha}` |
| `range` | `{repo}`, `{base}`, `{head}` | `/{repo}/compare/{base}...{head}` |
| `pull_request` | `{repo}`, `{base}`, `{head}`, `{pr}` | `/{repo}/pulls/{pr}` |

The battery, clause 14, holds each template to `code_pages::check`: it starts with `/`, closes every `{`, and carries only its page's placeholders.

Source: `crates/knobas-source/src/code_pages.rs`.

## `SourceError`

Every fallible function answers a `source-error`, the WIT form of `knobas_source::SourceError`. The class is what the person sees, so classify the same failure the same way in `test-connection` and in `sync`. A failure that came from a response carries the HTTP status.

| Variant | Raise it when | What follows |
|---|---|---|
| `unauthorized(status)` | The credential was refused: `401` for a credential the source rejected, `403` for a request it refused. `none` when the adapter raised it without asking anyone, because there is no secret to send. | The UI offers *Re-enter*. |
| `unreachable(detail)` | The source did not answer, and nothing it did depends on that: a connect, DNS, TLS or timeout failure on a read; a write that never left. | A write that failed so goes again on the write queue's backoff ladder. Everywhere but the write queue, `unconfirmed` reads as this too. |
| `unconfirmed(fault)` | A write that may be applied only once reached the source, and no answer says whether it acted: a timeout after the request went out, a gateway's `502` or `504`, a connection closed without an answer. | Holds the write for a person to check, unless the op's delivery says a second arrival is harmless; then it retries. |
| `protocol(fault)` | Anything else: an answer knobas cannot make sense of, a refusal whose status is not about the credential, or the adapter's own fault, with `status` `none`. Also an undeclared write, an undeclared source action, `reachable-transitions` from a source with no workflow, and an unknown view section. | The call fails with the message. |
| `sink(detail)` | Never on its own account. Return the error `sink.item` gave, unchanged. | The sync fails with it, whatever `sync` returns. |
| `moved(moved)` | A write was made against a version of its target that the source no longer holds, so the source did not apply it. `base` is the version the source holds now, in its own spelling; `current` is the target's text at that version. | Holds the write with both. *Apply anyway* re-sends it against `base`. Never retried on its own. |

`fault` is `{status: option<u16>, message: string}`. The host raises `protocol` itself for everything it refuses or cannot decode, and for a component that traps or passes a limit. Writes and their errors are covered in [writes.md](../writes.md); mapping HTTP outcomes is in [http.md](../http.md).

Source: `SourceError` and `Source::test_connection` in `crates/knobas-source/src/lib.rs`; `convert::error` in `crates/knobas-wasm-host/src/convert.rs`.

## Instance ids

An instance id, and therefore `adapter_kind`, must match `[a-z][a-z0-9-]{0,31}` and must not be a namespace knobas keeps for its own entities: `note`, `ctx`, `asset`, `route` or `monitor`, compared ignoring case. The first violation is the message:

| Message | Cause |
|---|---|
| `an instance id must not be empty` | `""` |
| `an instance id must be at most 32 characters, this one is <n>` | too long |
| `an instance id must start with a lowercase letter, '<c>' does not` | `Jira`, `1jira`, `-jira` |
| `an instance id may hold only lowercase letters, digits and '-', '<c>' may not appear` | `jira_eu`, `jira:eu` |
| `"<id>" is a namespace knobas keeps for its own entities` | `note` |

The person chooses the instance id at *Add source*, and it is baked into every entity id the source writes, so it cannot be renamed.

Source: `validate_instance_id` in `crates/knobas-source/src/instance.rs`; `RESERVED_NAMESPACES` in `crates/knobas-spi-types/src/entity.rs`; `Manifest::parse` in `crates/knobas-wasm-host/src/manifest.rs`.
