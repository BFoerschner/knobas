// Generated from the Rust types by ts-rs; do not edit. Regenerate with `cargo test -p knobas-app --lib ipc_mirror` (crates/knobas-app/src/ipc_mirror.rs).
import type { ActivityRow, LinkEntry, Tone } from "./entity";
import type { QueuedWrite } from "./sources";

/**
 * One asset an alert's monitor watches, as the estate's list of open alerts
 * names it.
 *
 * A list and not a field, because `monitored-by` is an ordinary link and
 * nothing stops two assets naming one Uptime Kuma monitor -- a VM and the
 * container on it can honestly both be watched by one HTTP monitor on the
 * product. **Empty is a real answer**: a monitor attached to nothing still
 * opens an alert, and hiding it would make the estate quietest about exactly
 * the monitors nobody has finished wiring up.
 *
 * Three fields and not an [`AssetRow`]: this is a line in a list, and an
 * `AssetRow` would drag the rollup and the linked-work count through a read
 * whose whole answer is *which machine, and where it lives*.
 */
export type AlertAsset = { 
/**
 * `asset:<uuid>` -- the address the list opens.
 */
id: string, name: string, 
/**
 * Its ancestors, outermost first, or `null` at the top of the estate --
 * [`DependsOnRow::path`]'s field and its spelling.
 */
path: string | null, };

/**
 * What state an open [`OpenAlert`] was opened in: the trouble a monitor
 * is in.
 *
 * Two words, not four: an alert exists only for `down` and `warn`, which
 * is migration `0022`'s `monitor_alert_state_chk` and
 * [`knobas_sync::alerts::OPENS`]. Deliberately **not** [`AssetStatus`],
 * although the two words are spelled the same there: an asset's status
 * has `up` and `none` in it because "nobody has said" is a thing an asset
 * can be, and an alert that could be `none` would be a row saying nothing
 * is wrong. [`tests::the_alert_states_are_the_ones_the_engine_opens`]
 * holds the two lists together. The spellings are shared with a status
 * on purpose, so a reader can compare an alert with the health it caused.
 *
 * - `down`: the monitor did not answer.
 * - `warn`: the monitor answered slower than the response-time threshold
 *   -- knobas' own state, derived at sample time (#443).
 *
 * The states are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/assets.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type AlertState = "down" | "warn";

/**
 * One asset already in the tree that an apply would change.
 *
 * An asset with nothing to change is **not** here -- it is in
 * [`ImportPreview::known`] with the rest -- so this list is the third group
 * the Import dialog draws and its length is the honest answer to *what would
 * this do*.
 */
export type AssetChange = { id: string, 
/**
 * The name the asset carries **in the tree**, which is what the reader is
 * looking at. The import does not rename: a name is the one thing about
 * an asset a person is certain to have chosen deliberately, and a file
 * re-titling twenty-three of them is not a change anybody asked for by
 * pressing *Import*.
 */
name: string, 
/**
 * Every property whose file value differs, whether it would be set or
 * kept. Both, in one list, because *"the file says `cx23` and it stays
 * `cx33` because you typed that"* is the sentence story 24 is about, and
 * a list carrying only what changes cannot say it.
 */
properties: Array<PropertyChange>, 
/**
 * Monitor names the file lists that this asset does not carry yet.
 *
 * Additive: an import never removes a name, because a file is one of the
 * two things that puts one there and the M4.1 sync is the other.
 */
monitors: Array<string>, };

/**
 * Everything the fixed right pane draws for one asset.
 */
export type AssetDetail = { asset: AssetRow, 
/**
 * Known keys first, custom after. See [`AssetProperty`].
 */
properties: Array<AssetProperty>, 
/**
 * The environment in force here and the asset that sets it (story 8);
 * `null` when nothing at or above this asset sets one.
 */
effective_environment: Inherited<Environment> | null, 
/**
 * The owner in force here and the asset that sets it (story 9).
 */
effective_owner: Inherited<string> | null, 
/**
 * The path down to this asset, **outermost first and excluding itself**.
 * Empty for an asset at the top of the estate.
 */
held_by: Array<AssetRow>, 
/**
 * What sits directly under it, in the column's own order.
 */
holds: Array<AssetRow>, 
/**
 * The routes **this asset exposes** (#432), by name.
 */
exposes: Array<RouteRow>, 
/**
 * The routes whose target is on this asset's containment path -- landing
 * on it, on something that holds it, or on something it holds -- nearest
 * first (#432). [`ROUTES_REACHABLE`] argues that rule.
 *
 * A route whose [`target_id`](RouteRow::target_id) is this asset's own id
 * lands here; any other target is the asset on the path it lands on, and
 * [`target_name`](RouteRow::target_name) names it. That comparison is the
 * whole of "reached through something else", so there is no second field
 * carrying it.
 */
reachable_via: Array<RouteRow>, 
/**
 * This asset's own lines from the activity stream, newest first.
 */
history: Array<ActivityRow>, 
/**
 * Every confirmed link this asset takes part in, either end, newest
 * first -- story 36's *Link to…* read back (#435).
 *
 * `knobas_core::link::entries_of`'s answer, unfiltered and unsorted here:
 * the panel that draws it is the one a ticket's detail draws
 * (`app/src/lib/detail/LinksPanel.svelte`), and a second population --
 * "work links" against "asset links" -- would be a second rule for the
 * reader to learn on a surface whose whole point is that linking an asset
 * is the same gesture as linking a ticket. What the *badge* counts is
 * narrower and is [`AssetRow::linked_work`].
 */
links: Array<LinkEntry>, 
/**
 * The Uptime Kuma names of the monitors watching this asset, as an
 * [`import`](apply_import) kept them (#439, `0020`).
 *
 * **Names, not monitors.** A name becomes a `monitored-by` link the
 * moment the mirror holds a monitor called that -- and until then it is
 * all knobas has, because no adapter emits the `monitor` kind before
 * M4.1. So this list is what the estate file said and the [`links`] list
 * is what has been resolved out of it; a name in both is a name whose
 * monitor has arrived.
 *
 * On [`AssetDetail`] and not on [`AssetRow`], for [`properties`]' reason:
 * a Miller column draws neither, and putting it on the row would put it
 * on every column of every walk to serve one pane.
 *
 * [`links`]: AssetDetail::links
 * [`properties`]: AssetDetail::properties
 */
monitors: Array<string>, 
/**
 * The monitors watching this asset, by name -- the pane's *monitoring*
 * section (spec #427 story 33, issue #445).
 *
 * [`monitors`] and this are the two halves of one sentence: that list is
 * what the estate file *said*, this is what has been found. A name in
 * both is a name whose monitor has arrived; a name in only the first is
 * one still waiting, and it is what the import's preview reports as
 * [`UnresolvedMonitor`].
 *
 * Read out of [`links`] rather than by a second walk of `knobas.link`, so
 * this section and the *Linked* panel under it cannot disagree about what
 * is attached.
 *
 * [`links`]: AssetDetail::links
 * [`monitors`]: AssetDetail::monitors
 */
monitoring: Array<AttachedMonitor>, 
/**
 * The sources a *Create monitor for this asset* would go to -- empty when
 * there are none, which is when the pane offers nothing (spec #427 story
 * 70, issue #453).
 *
 * **A property of the profile and not of the asset**, carried on this
 * read because this read is the one the pane makes. The alternative was a
 * command of its own, which would be a second round trip per selection
 * for a list that is almost always empty or one long.
 *
 * **Empty is the ordinary answer, and it is the criterion.** *"Without
 * the account the action is absent"*: an Uptime Kuma configured with only
 * its API key declares no write ops at all, so it is not in this list and
 * the pane draws no control -- rather than drawing one that queues a
 * write only to have `submit_write` refuse it by name.
 *
 * Filled by `commands::assets::get_asset` rather than by [`get`], for the
 * reason `MonitorRow::actions` is filled there: the answer is in the
 * keychain and this module reads the database.
 */
monitor_targets: Array<MonitorTarget>, };

/**
 * One field of an asset, and what it is being set to.
 *
 * A tagged union rather than a struct of optionals, and for the reason
 * `time::TimerTarget` is one: an edit is *one* field changing, every edit
 * writes its own history line, and a bag of `Option`s would make "cleared"
 * and "not mentioned" the same value on the wire. `null` on the three
 * nullable fields is therefore an unambiguous **clear**.
 *
 * **The line is a `from` and a `to` for every variant but one** (issue #453).
 * [`Monitors`](AssetEdit::Monitors) appends to a list rather than setting a
 * field, so its line says `added` and there is no `from` to state: an asset's
 * monitor names are added by two surfaces and taken away by neither, and a
 * `from` carrying the whole previous list would be a copy of the column in
 * the log. That variant argues the exception where it is declared; this is
 * the header saying it exists, so a reader does not take the rule above for a
 * rule the enum keeps.
 *
 * - `name`: rename.
 * - `status`: set the asset's own status.
 * - `environment`: set or clear the environment.
 * - `owner`: set or clear the owner.
 * - `property`: set or clear one property.
 * - `monitors`: add Uptime Kuma monitor names to [`AssetDetail::monitors`]
 *   (issue #453).
 *
 *   **The one variant that appends rather than sets**, and the only one
 *   whose field is a list. It is the same write the import makes -- `0020`'s
 *   column, [`ADD_MONITORS`]' `array_cat`, an `edited` line spelling
 *   `field: "monitors"` -- reached from the other of the two surfaces that
 *   put a name there. *Create monitor for this asset* records the name here
 *   **before** it queues the write, because the name is what attaches the
 *   monitor when the next poll mirrors it (`knobas_sync::attach`), and a
 *   create whose write landed with no name on the asset would be a monitor
 *   nothing joins to anything.
 *
 *   **Append, because a name is never taken away by anything but the
 *   reader** -- which is the import's own rule ([`ADD_MONITORS`]: *"an
 *   import never removes a monitor name"*) and the reason a set would be
 *   wrong here too: this edit knows about one name, and assigning would
 *   drop the six an estate file put there. Removing one is not this
 *   ticket's, and the pane offers no control for it.
 *
 *   A name the asset already carries is dropped rather than refused, so
 *   pressing *Create* twice for one name leaves one name and writes no
 *   history line: `array_cat` cannot deduplicate and [`monitor_plan`]
 *   matches by name, so a duplicate would be an entry a reader could never
 *   get rid of.
 *
 * The edits are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/assets.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type AssetEdit = { "field": "name", value: string, } | { "field": "status", value: AssetStatus, } | { "field": "environment", 
/**
 * `null` clears it, so the asset inherits again.
 */
value: Environment | null, } | { "field": "owner", 
/**
 * `null` clears it.
 */
value: string | null, } | { "field": "property", key: string, 
/**
 * `null` removes the key. Blank text is refused: clear it instead.
 */
value: PropertyValue | null, } | { "field": "monitors", added: Array<string>, };

/**
 * One row of the pane's property list.
 *
 * Typed properties come first, **in the type's declared order and including
 * the ones nobody has filled in** -- that is how a reader learns a VM is
 * meant to have an IP. Custom keys follow, in key order, and carry
 * `custom: true` so the pane can rule a line between the two groups without
 * keeping its own copy of the schema.
 */
export type AssetProperty = { key: string, 
/**
 * What the pane calls it: the type's label for a typed key, the key
 * itself for a custom one.
 */
label: string, 
/**
 * `null` for a declared property with no value yet.
 */
value: PropertyValue | null, 
/**
 * Whether this key is the reader's own rather than one the type
 * declares: `false` for every typed property, `true` for every custom
 * key.
 */
custom: boolean, };

/**
 * One row of a Miller column, and the shape every read hands back.
 *
 * `Row` and not `Node`: `CONTEXT.md`'s **Asset** entry lists *node* among the
 * words to avoid, and every list line this app already draws is a `…Row`
 * (`EntityRow`, `LinkRow`, `ActivityRow`, `NoteRow`, `ContextRow`). The tree
 * shape is `parent_id`'s, not this type's name's.
 *
 * `has_children` is what the column draws its chevron from and what tells the
 * Tree that there is a next column to open. It is computed by the read rather
 * than stored: a stored count is a second copy of the parent field, and the
 * two would disagree the first time a move failed halfway.
 */
export type AssetRow = { 
/**
 * `asset:<uuid>`, and also the id of this asset's `knobas.entity` row.
 */
id: string, 
/**
 * The asset that holds this one; `null` at the top of the estate.
 */
parent_id: string | null, 
/**
 * One of [`knobas_core::asset::TYPES`].
 */
type_id: string, 
/**
 * What that type is called -- resolved here so no surface keeps a copy of
 * the table.
 */
type_label: string, 
/**
 * The type's two-character chip; `"??"` for a type this build has lost.
 */
monogram: string, name: string, 
/**
 * The asset's **own** status. See [`AssetStatus`].
 */
status: AssetStatus, 
/**
 * The environment set **on this asset**; `null` where it inherits (#431).
 */
environment: Environment | null, 
/**
 * The owner set **on this asset**; `null` where it inherits (#431).
 */
owner: string | null, 
/**
 * Whether anything sits under it.
 */
has_children: boolean, 
/**
 * The asset's **effective health**: the worst of [`status`](Self::status),
 * the monitors attached to it, and every descendant's own two, down over
 * warn over up over none (stories 37 and 38, #444). A monitor Kuma paused
 * contributes nothing.
 */
health: AssetStatus, 
/**
 * The worst status **strictly underneath** this asset; `none` when it
 * holds nothing, or nothing under it has been rated.
 *
 * A field of its own rather than something the badge reads off
 * [`health`](Self::health), because those two answer different questions
 * the moment an asset is worse than what it holds: a `down` VM holding
 * one `warn` container has `health = down` and `inside = warn`, and the
 * badge is about what is *inside*, so it is amber.
 */
inside: AssetStatus, 
/**
 * How many descendants carry `warn` or `down` -- the *N* in story 32's
 * "N problems inside" badge. `0` draws no badge.
 */
problems_inside: number, 
/**
 * How many **work items** this asset is linked to -- story 32's *other*
 * badge, the one #435 adds beside the problem count.
 *
 * A *work item* is an entity the mirror holds: a ticket, a build, a page,
 * a commit. Contexts, notes and other assets are knobas' own and are not
 * counted, which is the whole of what "linked **work**" means -- a
 * container held under a compose project links to the VM it runs on, and
 * that link is containment's neighbour rather than work.
 *
 * Confirmed links only, in either direction, and one per link rather than
 * one per item: a pair carries one active link per relation
 * (`link_pair_active_idx`), so an asset linked to one ticket as both
 * `deployed-from` and `documented-in` is two facts and reads as two. See
 * [`LINKED_WORK`].
 */
linked_work: number, };

/**
 * An asset's **own** status -- what a person recorded about it.
 *
 * Not its health: story 37's health is the worst of this and its monitors'
 * states, and its *effective* health also takes the worst descendant. Both
 * are computed at read time, by [`ROLLUP`] over this column and over the
 * newest sample of every attached monitor (#444). This is the stored fact
 * those reads are built on, and [`AssetRow`] carries all three.
 *
 * `none` is the default and means *nobody has said*, which is a different
 * thing from `up`. The four spellings are `0017`'s `asset_status_chk`, and
 * [`tests::every_status_is_one_the_schema_accepts`] reads the migration to
 * keep the two lists in step.
 */
export type AssetStatus = "up" | "warn" | "down" | "none";

/**
 * One built-in asset type.
 *
 * Read off the backend by `asset_types` rather than declared in the frontend,
 * so the sixteen types are one list. The **ids are wire values and never
 * change**; the labels are display and may be reworded.
 */
export type AssetType = { 
/**
 * The `knobas.asset.type_id` value. A wire value -- see the module docs.
 */
id: string, 
/**
 * What one of them is called.
 */
label: string, 
/**
 * Two characters, for the fixed-width chip a column row draws.
 */
monogram: string, 
/**
 * The typed properties, **in the order the pane shows them**. The order is
 * the declaration's; it is not sorted anywhere. `custom` declares none.
 */
properties: Array<TypedProperty>, 
/**
 * The child types conventionally suggested under one of these, by id, in
 * the order the create dialog offers them.
 *
 * A **suggestion and not a constraint** -- see the module docs: the
 * create dialog offers all sixteen whatever is in here. Empty is
 * a real answer: nothing usually goes inside a table, a network or a
 * `custom`, and an empty list is what makes the dialog say *any type*
 * rather than name one at random.
 *
 * Every id here is one [`TYPES`] declares, which
 * [`tests::every_suggestion_is_a_type_that_exists`] holds it to: a typo
 * would be a chip in the dialog that creates a type
 * `knobas_app::assets::create` refuses.
 */
suggests: Array<string>, };

/**
 * One monitor watching an asset, as the pane's monitoring section draws it.
 *
 * The attachment is the link; the state and the address are the mirror's, and
 * both are optional because both can honestly be missing -- a monitor Kuma
 * paused is a tombstone with neither (#442, `knobas_source_kuma::map`), and a
 * monitor whose source the reader disabled leaves the mirror without leaving
 * the link.
 */
export type AttachedMonitor = { 
/**
 * The monitor's entity id -- `<source>:<monitor id in Kuma>`, and the
 * address the *Linked* panel opens.
 */
entity_id: string, 
/**
 * Its name, which is the name an estate file uses to ask for it.
 */
name: string, 
/**
 * The state word the adapter last recorded: `up`, `down`, `pending`,
 * `maintenance`. `null` when the mirror holds no reading -- a tombstone,
 * or a state code the adapter has no word for.
 */
state: string | null, 
/**
 * Its own page in Uptime Kuma, straight from the mirror row's `web_url`
 * (spec #427 story 71). `null` when there is no page left to open.
 */
web_url: string | null, 
/**
 * Whether the monitor has left the mirror -- paused in Kuma, or deleted.
 *
 * `CONTEXT.md`'s word for this state is **tombstone**: "marking an item
 * deleted-at-source while keeping the row", against *withdraw*, which
 * that glossary gives to a queued write pulled back. The fact itself is
 * [`knobas_core::link::LinkEnd::deleted_at`], which the *Linked* panel
 * under this section renders with the older spelling, *withdrawn*.
 *
 * A fact the reader is shown rather than a filter, for that field's own
 * reason: a section that silently dropped a paused monitor would tell an
 * asset somebody deliberately silenced a check on that nothing watches it.
 */
tombstoned: boolean, };

/**
 * How a layer's tiles group in a view (prototype-6's *Group by*).
 *
 * - `none`: *No groups*.
 * - `environment`: *Environment*.
 * - `owner`: *Owner or team*.
 * - `parent`: *What it runs on*: the asset it sits inside.
 * - `project`: *Project*.
 * - `status`: *Status*.
 *
 * The groupings are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/assets.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type DepGroup = "none" | "environment" | "owner" | "parent" | "project" | "status";

/**
 * One thing Dependencies can draw as a tile.
 */
export type DepItem = { 
/**
 * The entity id: `asset:…`, `route:…`, `jira:PAY-231`.
 */
id: string, 
/**
 * One of the [`DepKind`] ids: an asset type id, or one of `ticket`,
 * `pr`, `page`, `repo`, `pipeline`, `route`.
 */
kind: string, name: string, 
/**
 * Broken (`err`), warning (`warn`), in flight (`info`), fine (`ok`), or
 * `null` where nothing says.
 */
health: Tone | null, 
/**
 * The source's own state word: a ticket's status, a pull request's state,
 * a pipeline's newest run. `null` for assets and routes, and where the
 * declaration misses (ADR-0007).
 */
state: string | null, 
/**
 * How many monitors are attached: `0` draws an asset or route hatched.
 * Always `0` for a mirror item.
 */
monitors: number, 
/**
 * The environment in force on an asset or route.
 */
environment: string | null, 
/**
 * The owner in force on an asset.
 */
owner: string | null, };

/**
 * One kind Dependencies places in a layer.
 */
export type DepKind = { id: string, 
/**
 * The built-in label (*Hypervisor*, ruling D15).
 */
label: string, 
/**
 * The two letters a tile draws.
 */
monogram: string, };

/**
 * One shared layer: its name and the kinds it holds, in order. The layers are
 * the same in every view (D14).
 */
export type DepLayer = { id: string, name: string, 
/**
 * [`DepKind`] ids: asset type ids, `ticket`, `pr`, `page`, `repo`,
 * `pipeline`, `route`.
 */
kinds: Array<string>, };

/**
 * One known relation between two items of the read.
 */
export type DepRelation = { from: string, to: string, 
/**
 * A confirmed link's relation, or [`INSIDE`], [`EXPOSES`], [`LANDS_ON`],
 * [`IN_REPOSITORY`] -- `inside`, `exposes`, `lands-on`, `in-repository`.
 */
relation: string, };

/**
 * The tile size (prototype-6's *Tiles*).
 *
 * - `auto`: small when a layer holds more than 30.
 * - `roomy`: always roomy.
 * - `compact`: always small.
 *
 * The sizes are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/assets.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type DepTiles = "auto" | "roomy" | "compact";

/**
 * One saved view.
 */
export type DepView = { id: string, name: string, settings: DepViewSettings, };

/**
 * Everything a view keeps, and nothing else (R-11.13).
 */
export type DepViewSettings = { 
/**
 * By layer id. A layer not named here shows, unfolded and ungrouped.
 */
layers: { [key in string]: LayerInView }, 
/**
 * The environment filter: empty is every environment.
 */
environments: Array<string>, 
/**
 * *Problems and what they touch*.
 */
problems: boolean, tiles: DepTiles, 
/**
 * Healthy groups fold above this many items in a layer.
 */
fold_at: number, };

/**
 * Every view, in order, and which opens first from a ticket and from the top
 * bar.
 */
export type DepViews = { views: Array<DepView>, opens_from_ticket: string, opens_from_top: string, };

/**
 * Everything the top-bar opening draws.
 */
export type Dependencies = { layers: Array<DepLayer>, kinds: Array<DepKind>, items: Array<DepItem>, relations: Array<DepRelation>, };

/**
 * One line of the *Depends on this* panel: an asset that breaks, where it
 * sits, and why it is on the list (#505).
 *
 * A carrier rather than fields on [`AssetRow`]: `path_text` is not on a
 * Miller column's read, and `relation` is true of this *answer* and not of
 * the asset, which is on somebody else's list under a different word.
 */
export type DependsOnRow = { asset: AssetRow, 
/**
 * The ancestors' names, outermost first, `" / "` between them; `null` for
 * an asset at the top of the estate.
 *
 * Story 54's *"each dependent's path in the tree"*, read off
 * `knobas.asset.path_text`.
 */
path: string | null, 
/**
 * The relation it came through -- [`DEPENDS_ON`] or [`RUNS_ON`] -- and
 * **`null` when it came through containment**.
 *
 * The other half of story 54, *"the relation it came through"*, and the
 * null is the tree: ADR-0014 makes holding a `parent_id` field and says
 * there is no `holds` relation, so there is no word to put here that a
 * reader could not also draw as a link. The panel reads the null as
 * *inside*.
 *
 * An asset reachable both ways reads as the edge that got there first.
 * **Across** steps that is the walk's own breadth-first order, so the
 * nearest edge wins; **within** one step it is [`NEXT_LAYER`]'s
 * `distinct on`, which prefers containment.
 */
relation: string | null, };

/**
 * What breaks if one asset goes down (`CONTEXT.md`, **Depends on this**;
 * spec §12.2, issue #505).
 *
 * Two lists and no count: the count the panel draws is `assets.len()`, and a
 * number beside a list it can be read off is a fact on the wire twice.
 */
export type DependsOnThis = { 
/**
 * Every asset that breaks, nearest first -- the whole closure, each with
 * its path and the edge it was reached by.
 *
 * Never the asset itself: the question is what *else* stops working.
 */
assets: Array<DependsOnRow>, 
/**
 * The routes that land on the asset or on any asset in
 * [`assets`](Self::assets), listed beneath and **not counted** (story 52,
 * so that the number is about assets; widened by #1553). Each row's
 * `target_id` says which of them it lands on.
 *
 * Not folded into [`assets`](Self::assets): a route is not an asset, and
 * what it adds to the answer is the way in that stops, not a thing that
 * stops.
 */
routes: Array<RouteRow>, };

/**
 * Which environment an asset belongs to.
 *
 * Stored on the asset, and story 8 inherits it from the nearest ancestor that
 * sets it -- that walk is #431's. The four spellings are `0017`'s
 * `asset_environment_chk`, kept in step with this list by
 * [`tests::every_environment_is_one_the_schema_accepts`].
 *
 * `null` where an asset's own field carries one means *not set here*: what is
 * in force is [`AssetDetail::effective_environment`], the nearest asset at or
 * above that sets one.
 */
export type Environment = "dev" | "stage" | "prod" | "shared";

/**
 * One entry of the preview's *already in the tree* or *new* group.
 *
 * Assets and routes in one shape and one list per group, because the question
 * a group answers is about the **file** -- what of it does knobas already
 * hold -- and a reader counting what is new counts entries rather than two
 * populations. Which of the two an entry is, is [`kind`](Self::kind).
 */
export type ImportEntry = { 
/**
 * The file's own id, which is the id the asset or route carries.
 */
id: string, 
/**
 * `"asset"` or `"route"` ([`NAMESPACE`] or [`ROUTE_NAMESPACE`]) -- the
 * word `knobas.entity.kind` holds, so the preview carries no vocabulary
 * of its own.
 */
kind: string, name: string, 
/**
 * The type's label for an asset; `null` for a route, which has no type.
 */
type_label: string | null, 
/**
 * Where the file puts it: the parent for an asset, the exposing asset for
 * a route. `null` only for an asset at the top of the estate.
 */
parent_id: string | null, };

/**
 * What an import did.
 *
 * Counted rather than listed: the preview is where a reader looks at the
 * detail, and this is what the summary line says and what the dialog reports
 * when it is over.
 */
export type ImportOutcome = { assets_created: number, routes_created: number, 
/**
 * Properties the file's value was written to.
 */
properties_set: number, 
/**
 * Properties left as they were because a hand edit claimed them.
 */
properties_kept: number, 
/**
 * Monitor names newly kept on an asset.
 */
monitors_kept: number, 
/**
 * `monitored-by` links drawn.
 */
monitors_linked: number, };

/**
 * What an import would do, before it has done any of it.
 *
 * The three groups the dialog draws -- already in the tree, new, and what
 * would change -- plus the monitor links, which are a write and therefore
 * have to be announced by the same read that announces the others, and the
 * names that found no monitor, which are the same announcement's other half.
 */
export type ImportPreview = { 
/**
 * What the file calls the estate.
 */
name: string, 
/**
 * Entries whose id knobas already holds. On a second import of an
 * unchanged file this is the whole file and every other list is empty.
 */
known: Array<ImportEntry>, 
/**
 * Entries an apply would create, **in the order it would create them**:
 * an asset before the assets it holds, then the routes.
 */
new: Array<ImportEntry>, 
/**
 * The known assets with something to change, and what.
 */
changes: Array<AssetChange>, 
/**
 * The `monitored-by` links an apply would draw.
 */
monitor_links: Array<MonitorLink>, 
/**
 * The monitor names the file's assets carry that the mirror does not
 * hold, by asset and then by name. See [`UnresolvedMonitor`].
 *
 * **Over the assets the file names, and no others.** A preview is a
 * sentence about a file, so a name kept on an asset the file has since
 * stopped listing is not reported here -- it is on the asset, where the
 * pane draws it, and the next file that names that asset again picks it
 * up.
 */
unresolved: Array<UnresolvedMonitor>, };

/**
 * A value **in force** on an asset, and the asset that sets it.
 *
 * Stories 8, 9 and 10 in one shape: what the value is, and *where to go to
 * change it*. The source is a whole asset's id and name rather than a bare
 * `inherited: bool`, because "inherited" without a name leaves the reader
 * hunting up the path for the ancestor that decided it -- and the pane's one
 * click to that ancestor is exactly what story 10 asks for. Whether it was
 * set *here* is [`source_id`](Self::source_id) equalling the asset's own id,
 * which the pane compares rather than being told twice.
 */
export type Inherited<T> = { 
/**
 * The value in force -- an [`Environment`] for one field of
 * [`AssetDetail`], an owner's name for the other.
 */
value: T, 
/**
 * The asset the value is set on -- this asset itself when it is set here.
 */
source_id: string, 
/**
 * That asset's name, so the pane needs no second read to label the link.
 */
source_name: string, };

/**
 * Where the servers an [importer](Importer) found and this estate does not hold
 * will land (#509, second ruling of 2026-09-08).
 *
 * **A struct and not a bare `Option<String>`, because three answers need three
 * spellings.** The argument is `Option<Landing>`, and the three are: *nothing
 * has been said yet* (the argument absent), *under this asset*
 * (`{ parent: "asset:…" }`), and *at the top of the estate*
 * (`{ parent: null }`). A bare `Option<String>` gave the last two one spelling
 * between them, and the collision was not academic: the Import dialog offers
 * the top, promises it in words, and sent `null` -- which the producer read as
 * *nothing said yet*, so it asked the same question again. On an estate with
 * **no assets** there is no other answer to give, so the only button on a
 * first run did nothing, for ever, for an importer whose whole purpose is to
 * populate a tree.
 *
 * `parent: None` is the module's own spelling of the top and not a new one:
 * [`create`] is *"create an asset under `parent_id`, or at the top of the
 * estate"*, [`move_to`] is *"or to the top of the estate"*,
 * [`ImportEntry::parent_id`] is *"`null` only for an asset at the top"*, and
 * [`FileAsset::parent`] is an `Option` the Import has written as a top-level
 * asset since #439. `testenv/estate.json` has exactly one parentless
 * asset. An importer that refused the one answer the Import accepts would be
 * the *"second set of rules"* spec #491's story 69 forbids.
 */
export type Landing = { 
/**
 * The asset the new servers go under, or `None` for the top of the estate.
 *
 * `#[serde(default)]` so `{}` reads as the top: an absent key and an
 * explicit `null` are one answer here, which is the opposite of the
 * distinction the whole type exists to make -- that one is between the
 * **argument** being absent and this field being `null`, and those cross
 * the bridge as different things.
 */
parent: string | null, };

/**
 * What a view keeps for one layer.
 */
export type LayerInView = { hidden: boolean, folded: boolean, group: DepGroup, };

/**
 * One `monitored-by` link an apply would draw.
 *
 * A row per link rather than a count, because the reader's question is *which
 * of my monitors did it find* -- and because a name that resolves and a name
 * that does not look identical in the file. A name the mirror does not hold
 * is not here: it is kept on the asset (`0020`) and resolved by the next
 * import or by the M4.1 sync, which is spec #427's own sentence.
 */
export type MonitorLink = { asset_id: string, asset_name: string, 
/**
 * The Uptime Kuma name, as the file spells it and as the mirror holds it.
 */
monitor_name: string, 
/**
 * The mirrored monitor's entity id -- the other end of the link.
 */
monitor_id: string, };

/**
 * One row of the Monitors tab's roster -- every monitor the mirror holds, with
 * what the tab draws beside it (spec #427 story 68, issue #448).
 *
 * **The whole roster in one read, and no per-monitor round trip.** A tab that
 * fetched a monitor's samples on demand would be one call per row on a
 * surface whose point is that every row is comparable at a glance; the counts
 * on the chips are a count of *these rows*, so the frontend cannot draw them
 * without the whole list anyway.
 *
 * The fields split three ways and it is worth knowing which is which. The
 * identity and the address are `knobas.entity`'s and `sync.item`'s. The
 * **reading** -- [`monitor_type`], [`target`], [`response_time_ms`],
 * [`uptime`], [`cert_days_remaining`] -- is read out of the mirrored
 * payload (see [`reading_of`]): the response time through the source's
 * declared `response_time` (#1224), the rest under ADR-0007's interim
 * discipline. And
 * [`state`] and [`samples`] are knobas' own timeseries: *warn* exists
 * nowhere in Uptime Kuma and is derived at sample time (`0021`), so a chip
 * counting warns has to count samples and not the mirror.
 *
 * [`monitor_type`]: MonitorRow::monitor_type
 * [`target`]: MonitorRow::target
 * [`response_time_ms`]: MonitorRow::response_time_ms
 * [`uptime`]: MonitorRow::uptime
 * [`cert_days_remaining`]: MonitorRow::cert_days_remaining
 * [`state`]: MonitorRow::state
 * [`samples`]: MonitorRow::samples
 */
export type MonitorRow = { 
/**
 * `<source>:<monitor id in Kuma>` -- the entity id, and the address a
 * detail slide-over opens at.
 */
entity_id: string, 
/**
 * The source that mirrored it. On the row because a second monitoring
 * source is one more configured source and no new code (`samples::KIND`
 * samples any source that emits the kind), so the roster has to be able
 * to say which Kuma a monitor came from.
 */
source_id: string, 
/**
 * Its name in Uptime Kuma, which is the name an estate file uses to ask
 * for it.
 */
name: string, 
/**
 * **The state knobas last recorded**: the newest [`sample`](MonitorSample)
 * where there is one, and the mirror's own `state` where there is not.
 *
 * The newest sample first and not the mirror first, because the mirror
 * carries Kuma's four words and the sample carries knobas' five: *warn*
 * is derived from the response-time threshold at sample time and exists
 * in no payload. A chip counting warns off the mirror would count none,
 * for ever. The mirror is the fallback for the one case a sample cannot
 * cover -- a monitor mirrored by a run that wrote no sample, or one whose
 * samples retention has swept -- and it is the same
 * `payload->>'state'` read the pane's monitoring section makes (#445).
 *
 * `null` is a genuine miss: a tombstone with no reading left, or a state
 * code the adapter had no word for.
 */
state: string | null, 
/**
 * `http`, `ping`, `docker`, `port` -- Kuma's own word for what kind of
 * check this is. `null` where the payload does not say.
 */
monitor_type: string | null, 
/**
 * **What it watches**: the URL where there is one, else the hostname with
 * its port where there is one. One field and not three, because it is one
 * column of the roster and the three are never all present -- an HTTP
 * monitor has a URL and no hostname, a ping has a hostname and no URL.
 */
target: string | null, 
/**
 * The reading Kuma last published, in milliseconds. `null` for a poll
 * that did not answer -- Kuma's `-1` sentinel, which the adapter already
 * carries as an absence.
 */
response_time_ms: number | null, 
/**
 * **When knobas last read this monitor** -- `sync.item.synced_at`, the
 * same instant the run's samples carry. RFC 3339 on the wire.
 *
 * Kuma's `/metrics` has no clock in it at all (#442): there is no "last
 * checked" to mirror, so the honest answer is when knobas looked. For a
 * tombstoned monitor it is the run that noticed it had gone.
 */
checked_at: string, 
/**
 * Kuma's sliding-window uptime ratios, by its own window labels (`1d`,
 * `30d`, `365d`), in label order.
 *
 * A list and not three fields, for the reason the adapter's payload is a
 * map: the windows are Kuma's choice and a release that adds a fourth
 * should widen the payload rather than need a field here.
 */
uptime: Array<UptimeRatio>, 
/**
 * Days until the watched certificate expires, for a monitor that watches
 * one. `null` for every monitor that does not, which is most of them.
 */
cert_days_remaining: number | null, 
/**
 * Its own page in Uptime Kuma (story 71); `null` when there is none left.
 */
web_url: string | null, 
/**
 * Whether it has left the mirror -- **paused** in Kuma, or deleted.
 *
 * *Tombstoned* is `CONTEXT.md`'s word and [`AttachedMonitor`]'s field
 * name; *Paused* is what the chip says, because pausing is what a reader
 * did to make it true. A tombstoned monitor is still a row here: it keeps
 * its samples, so the hours before it vanished are still drawable, and a
 * roster that dropped it would say nothing about a monitor somebody
 * silenced.
 */
tombstoned: boolean, 
/**
 * The assets this monitor is attached to by a `monitored-by` link, by
 * name -- with the path that tells two containers called `postgres`
 * apart. Empty for a monitor nothing is attached to, which the tab says
 * in as many words.
 */
assets: Array<MonitoredAsset>, 
/**
 * The write ops the tab may offer **on this monitor**, already filtered
 * to what its source declares and to what makes sense here (issue #452).
 *
 * `knobas_source::WriteOp` identifiers, and the inbox's precedent
 * exactly (`app/src/lib/inbox/actions.ts`: "the backend has already
 * filtered them to what that item's source declares"). Two filters, and
 * they answer different questions:
 *
 * * **The source's**, which is what an empty list usually means: an
 *   Uptime Kuma configured with only an API key declares no write ops at
 *   all, because pausing is a socket.io login the key cannot make. Adding
 *   the account to that source's keychain item fills this in for every
 *   row, with no other change anywhere.
 * * **The row's**: `pause_monitor` on a monitor that is live,
 *   `resume_monitor` on one that is [`tombstoned`](Self::tombstoned) --
 *   never both, because a monitor is one or the other.
 *
 * That second filter is the one worth stating, because through Kuma's
 * `/metrics` **paused and deleted are one observation**: a monitor that
 * left the mirror may have been paused or may have been deleted, and
 * nothing on this side can tell. So *Resume* is offered on both, and one
 * of them is refused by Kuma with its own words -- which is the source's
 * answer to give, not a guess to make here.
 */
actions: Array<string>, 
/**
 * Its samples inside the bar's 24-hour window, **oldest first**.
 *
 * Raw and not bucketed, because the bucketing is the tab's and is tested
 * there: the backend would otherwise own a number of segments that only a
 * stylesheet knows. Bounded by the poll interval's own floor -- Uptime
 * Kuma declares 45 s (`SourceDescriptor::min_sync_interval_secs`, #1187),
 * so at most 1 920 per monitor per day -- which is why the window and not
 * a `limit` is what bounds it.
 */
samples: Array<MonitorSample>, };

/**
 * One sample as the bar draws it: when it was taken and what it read.
 *
 * The response time is deliberately **not** here. The bar is coloured by
 * state, one segment per bucket, and a response time per sample would be
 * 1 440 numbers per monitor crossing the bridge for a chart this milestone
 * does not draw (spec #427, *Out of scope*: "charts beyond the 24-hour
 * bar"). The reading the tab does show is [`MonitorRow::response_time_ms`],
 * the latest one.
 */
export type MonitorSample = { 
/**
 * RFC 3339 on the wire.
 */
taken_at: string, 
/**
 * `up`, `warn`, `down`, `pending`, `maintenance`, or `null` for a poll
 * whose state did not resolve -- which the bar draws as a gap.
 */
state: string | null, };

/**
 * One source a new monitor could be created in (issue #453).
 *
 * Three fields and each is used by a different part of the form: the id is
 * what tells two Kumas apart in a list nobody has to read, the display name
 * is what the reader picked when they added the source, and the entity is the
 * [`WriteOp::CreateMonitor`](knobas_source::WriteOp::CreateMonitor) target
 * the form submits.
 *
 * **The entity is composed here and not in the webview**, which is the whole
 * reason this is a struct rather than a list of ids: what a create targets is
 * the SPI's to spell ([`knobas_source::monitor_target`]), and a frontend that
 * composed `` `${id}:monitors` `` would be a per-adapter table in a Svelte
 * component.
 */
export type MonitorTarget = { 
/**
 * The configured source's id, which is also its entities' namespace.
 */
source_id: string, 
/**
 * What the reader called it when they added it.
 */
display_name: string, 
/**
 * The `entity` a `create_monitor` against this source carries.
 */
entity: string, };

/**
 * One asset a monitor is attached to.
 *
 * Not an [`AssetRow`]: a roster row needs a name, an address and a path, and
 * an [`AssetRow`] carries a health rollup behind it -- a read per asset per
 * monitor for facts this column does not draw.
 */
export type MonitoredAsset = { 
/**
 * `asset:<uuid>` -- and the `#/asset/<id>` address the name links to.
 */
id: string, name: string, 
/**
 * The ancestors' names, outermost first, `" / "` between; `null` for an
 * asset at the top of the estate.
 */
path: string | null, 
/**
 * What its type is called, resolved here, [`AssetRow`]'s rule: the new
 * shell's *Watches* column names it beside the asset (#864).
 */
type_label: string, };

/**
 * The two numbers monitoring is shaped by (issue #443, spec #427's "Settings
 * keys for the sample retention, the response-time threshold").
 *
 * One DTO and two commands rather than four commands, the shape
 * `BackupSchedule` and `set_backup_schedule` have: they are edited on one
 * section of one dialog and saved together, and a surface that could save
 * half of them is a surface that can be interrupted between the halves.
 *
 * `u32` because neither is ever negative, and both are clamped into range by
 * `knobas_sync::samples` on the way in and on the way out -- a settings
 * dialog and a hand-edited `knobas.setting` row are both places a number
 * arrives from, and only one of them has a spinner on it. So what a caller
 * posts and what it gets back can differ, and the settings section draws the
 * answer.
 */
export type MonitoringSettings = { 
/**
 * How many days of samples knobas keeps. Default ninety, floor one.
 */
sample_retention_days: number, 
/**
 * The response time, in milliseconds, above which an otherwise-up
 * monitor samples as *warn*. Default 1500.
 */
response_time_warn_ms: number, };

/**
 * One open alert, as the top strip counts it and the Assets view lists it
 * (spec #427 stories 57 and 58, issue #444).
 *
 * **No entity and no address of its own**, which spec #427 says in as many
 * words: the id here is the row's `bigint`, what [`state`](Self::state) is
 * about is the monitor, and what a reader opens is the asset. #446's ack and
 * #449's cards address the row by this id.
 */
export type OpenAlert = { 
/**
 * The alert row's id. A number and not a string, because it is not an
 * address -- see the type's own note.
 */
id: number, 
/**
 * The monitor this alert is about: its entity id, and the *Linked*
 * panel's address for it.
 */
monitor_id: string, 
/**
 * The monitor's name, which is the name an estate file uses to ask for
 * it.
 *
 * The name and **not** its page in Uptime Kuma: nothing this ticket draws
 * opens one, and a field on the wire that no surface reads is a field
 * whose shape nothing can be wrong about. #449's cards are where a link
 * into Kuma belongs, and [`AttachedMonitor::web_url`] is where one
 * already is.
 */
monitor_name: string, state: AlertState, 
/**
 * RFC 3339 on the wire.
 */
opened_at: string, 
/**
 * When somebody said they had seen it, or `null`; RFC 3339 on the wire.
 * Written by #446's ack; on the wire from this ticket because *acked* is
 * part of what an open alert is, and a list that could not say so would
 * draw every alert as unseen.
 */
acked_at: string | null, 
/**
 * The assets this alert's monitor watches -- see [`AlertAsset`].
 */
assets: Array<AlertAsset>, };

/**
 * What one run of a [producer](Producer) answered.
 *
 * Three states and not a struct of optionals, because two of the three fields
 * would be meaningless in each: a `file: None, needs: None` is a state no
 * producer is ever in and no caller should have to write code for. The tag is
 * `state`, which is [`PropertyValue`]'s `kind` arrangement one surface over.
 *
 * **Here and not in [`hcloud`]**, since #510 gave it a second producer. The
 * argument is the one this module already makes for [`Token`] and [`remember`]
 * -- *"beside [`Producer`] rather than inside `hcloud`, because nothing here
 * names a live system: the second importer would otherwise import its
 * credential handling from a module named after the first"* -- and a type
 * spelling every producer's answer is the same case. **The move is the Rust
 * path only**; the wire is unchanged, tag and arms and field names.
 *
 * Two of those field names now read narrower than what they carry:
 * [`Ready::new_servers`](Produced::Ready) holds *container* names when the
 * Docker importer filled it. They were declared by #509's §10.8 entry and
 * ratified a day before #510 was implemented, and renaming a declared wire
 * field is a §10.8 conversation of its own rather than a thing this ticket was
 * asked for -- so the names stand, said here so the next reader does not take
 * the narrowness for a claim. What the *dialog* draws is producer-neutral,
 * because the rendered sentence is nobody's frozen surface.
 *
 * - `token_needed`: nothing is stored under this importer's keychain account
 *   and the caller sent no token, so the dialog asks for one, **once**: the
 *   backend stores it after the run succeeds, so the next run comes back with
 *   a file instead. **Answered before any request is made**, so a reader who
 *   has never used this importer is asked once rather than being shown a 401
 *   from a call made with nothing. Reachable only from a producer that *has*
 *   a credential. The Docker importer has none -- it spawns a CLI that reads
 *   this machine's own contexts -- and `produce_estate_file` refuses a token
 *   sent to it rather than keeping one nothing would read.
 * - `landing_needed`: assets the estate does not hold, and nowhere said to
 *   put them. The reader is asked **once per run**: the answer comes back as
 *   `land_under` on the next call, and everything named here lands under it.
 *   One question and not one per asset, because *where the things a provider
 *   account holds go* is one decision -- and because the estate this was
 *   written against answers it once, with a site. Reachable only from a
 *   producer whose findings have no natural parent. Docker's have one by
 *   construction -- a container lands under the engine whose context found
 *   it (spec #491, story 68) -- so this state is the hcloud importer's alone
 *   today.
 * - `ready`: the estate file, in the checked-in shape, ready for
 *   [`preview_import`].
 *
 * The states are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/assets.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type Produced = { "state": "token_needed" } | { "state": "landing_needed", 
/**
 * The servers that are not in the tree, by name, in the order the
 * live system listed them.
 */
servers: Array<string>, } | { "state": "ready", 
/**
 * The file's text, in the checked-in shape -- there is nothing in it
 * that says which producer made it (spec #491, #508).
 */
file: string, 
/**
 * What this file would create, by name; empty when the estate already
 * holds everything the live system showed, which is what `just
 * estate-live` asserts of both importers.
 *
 * **Servers from hcloud and containers from Docker**, under a name
 * #509 gave it when hcloud was the only producer. See this type's own
 * header for why the name stands.
 */
new_servers: Array<string>, 
/**
 * What this run knew of and could **not** read, by name -- for Docker,
 * a `container_engine` asset carrying no `docker_context`.
 *
 * Its own field rather than a silence, because an engine with no
 * context is indistinguishable in the file from an engine holding no
 * containers, and the reader is the only one who can tell them apart.
 * Always empty for hcloud: one token either sees a server or does not
 * know it exists.
 */
skipped: Array<string>, };

/**
 * One property of one asset already in the tree whose file value differs from
 * the stored one.
 *
 * Only the ones that **differ**: a preview listing every property of every
 * known asset is a preview nobody reads, and *what would change* is the
 * group's own name. A key the file does not mention is not here either -- the
 * file is authoritative over what it says and silent about the rest, so an
 * import never removes a property.
 */
export type PropertyChange = { key: string, 
/**
 * The type's label for a declared key, the key itself for a custom one --
 * [`AssetProperty::label`]'s rule, so the two lists read alike.
 */
label: string, 
/**
 * What the asset carries now; `null` for a key it does not carry yet.
 */
from: PropertyValue | null, 
/**
 * What the file says.
 */
to: PropertyValue, plan: PropertyPlan, };

/**
 * What a property value may be.
 *
 * The four the spec names -- text, number, date, url -- and no fifth. The
 * *secret* kind is deferred with its keychain convention (spec #427, Out of
 * Scope), so there is nowhere in an asset a credential can be typed.
 *
 * A tagged union on the wire, `{"kind":"number","value":8080}`, and the same
 * shape in the `properties` jsonb: the kind is a fact about the value and
 * storing it beside the value is what lets a custom key be read back as the
 * thing it was entered as. A bare JSON scalar would make `"8080"` and `8080`
 * the same property and would leave a date indistinguishable from a string.
 */
export type PropertyKind = "text" | "number" | "date" | "url";

/**
 * What an apply would do with one property the file names.
 *
 * Two values and no third, because there are two ways a file's value can meet
 * a stored one: nobody has claimed the property, so the file wins, or
 * somebody has, so they do.
 *
 * - `set`: the stored value is replaced by the file's.
 * - `kept`: the stored value stays -- the reader's own value surviving,
 *   because a person edited this property by hand and the file never silently
 *   undoes that (story 24).
 *
 * The plans are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/assets.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type PropertyPlan = "set" | "kept";

/**
 * One property value, carrying the kind it was entered as.
 *
 * Tagged, and the tag is stored: `{"kind":"number","value":8080}` on the wire
 * *and* in the `properties` jsonb. A bare JSON scalar would make `"8080"` and
 * `8080` the same property and would leave a date indistinguishable from a
 * string, which is the whole of what story 6's four kinds are for.
 *
 * - `text`: free text -- a hostname, an image tag, a size.
 * - `number`: a port, a row count.
 * - `date`: a calendar date.
 * - `url`: an http(s) URL.
 *
 * The kinds are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/assets.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type PropertyValue = { "kind": "text", 
/**
 * Never blank: clearing a property is `null`, not `""`.
 */
value: string, } | { "kind": "number", 
/**
 * Finite; JSON has no other kind of number.
 */
value: number, } | { "kind": "date", 
/**
 * `YYYY-MM-DD`. Refused by the backend if it is not one.
 */
value: string, } | { "kind": "url", 
/**
 * Refused by the backend if it carries no scheme.
 */
value: string, };

/**
 * Everything the pane draws for one route.
 *
 * Its own history (story 11 applied to the other entity this module writes)
 * and the row. **No held-by path of its own**: a route sits on the asset that
 * exposes it, and [`RouteRow::asset_id`] is where the Tree opens -- which is
 * what makes `#/route/<id>` land at the exposing asset with the route
 * selected rather than in a surface of its own.
 */
export type RouteDetail = { route: RouteRow, 
/**
 * This route's own lines from the activity stream, newest first.
 */
history: Array<ActivityRow>, 
/**
 * The sources a monitor could be created in (#976), [`AssetDetail::monitor_targets`]'
 * rule: filled by the command and empty from [`get_route`].
 */
monitor_targets: Array<MonitorTarget>, };

/**
 * One field of a route, and what it is being set to.
 *
 * [`AssetEdit`]'s shape for [`AssetEdit`]'s reason: one field at a time, one
 * history line each with a `from` and a `to`, and `null` on the two nullable
 * fields is an unambiguous **clear**. The asset that exposes a route is not
 * among them -- see the module docs: a route is the address of the thing that
 * answers it, so re-exposing one elsewhere is a different route with a
 * different history.
 *
 * - `name`: rename the route.
 * - `url`: set the URL or endpoint.
 * - `target`: set or clear the asset it lands on.
 * - `visibility`: set who can reach it.
 * - `property`: set or clear one of the route's own properties.
 * - `monitors`: add the names of monitors the route waits for (#976),
 *   [`AssetEdit`]'s `monitors` append on the route's own column (`0042`):
 *   *Create monitor* on a route records the name here and the poll draws the
 *   `monitored-by` link.
 *
 * The edits are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/assets.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type RouteEdit = { "field": "name", value: string, } | { "field": "url", 
/**
 * Carries a scheme; a bare host is refused.
 */
value: string, } | { "field": "target", 
/**
 * The asset it lands on; `null` makes it an endpoint that lands on
 * nothing knobas knows, which is a legal route.
 */
value: string | null, } | { "field": "visibility", value: Visibility, } | { "field": "property", key: string, 
/**
 * `null` removes the key.
 */
value: PropertyValue | null, } | { "field": "monitors", added: Array<string>, };

/**
 * One route, as both ends read it.
 *
 * The same shape in `exposes` and in `reachable_via`, deliberately: it is one
 * row in the database and the two lists are two `where` clauses over it, so a
 * shape per direction would be two things to keep in step for no fact gained.
 * Both names are resolved by the read, so neither end has to fetch the other
 * to draw a line.
 *
 * **Where it lands** is [`target_id`](Self::target_id): equal to the asset's
 * own id means it lands here, and anything else is named by
 * [`target_name`](Self::target_name) -- an ancestor when the asset's
 * `held_by` holds it, and something inside otherwise. That comparison is what
 * story 31's dashed wire is drawn from.
 */
export type RouteRow = { 
/**
 * `route:<uuid>`, and also the id of this route's `knobas.entity` row --
 * which is what makes `#/route/<id>` an address (spec §2).
 */
id: string, 
/**
 * The asset that exposes it. Never null: a route with nothing answering
 * it is not a row this model can hold.
 */
asset_id: string, 
/**
 * That asset's name.
 */
asset_name: string, 
/**
 * The asset it lands on, or `null` for an endpoint that lands on nothing
 * knobas knows.
 */
target_id: string | null, 
/**
 * That asset's name, `null` with the target.
 */
target_name: string | null, 
/**
 * What the route is called -- *Gitea*, *TeamCity*.
 */
name: string, 
/**
 * The URL or endpoint, carrying its scheme.
 */
url: string, 
/**
 * Who can reach it. `internal` is what an unclassified route reads as.
 */
visibility: Visibility, 
/**
 * The reader's own keys, by key. A route has no type, so every one of
 * them is custom -- see [`custom_properties`].
 */
properties: Array<AssetProperty>, };

/**
 * One declared property of a type.
 *
 * Serialized as it stands: `asset_types` puts the table on the wire so the
 * create dialog and the pane's property editor read *one* list. Without it
 * the frontend would carry a second copy of sixteen types -- and, worse,
 * would have to guess which kind an unfilled typed property takes.
 *
 * The `kind` is what a pane's editor needs and what an asset's property row
 * cannot carry: a declared key nobody has filled in arrives with
 * `value: null`, and there is no kind in a `null`.
 */
export type TypedProperty = { 
/**
 * The key it is stored under in `knobas.asset.properties`.
 */
key: string, 
/**
 * What the pane calls it.
 */
label: string, 
/**
 * What a value for it must be.
 */
kind: PropertyKind, };

/**
 * What the *Not monitored* roster draws: an asset nothing watches, with where
 * it sits and what it is (spec #427 story 68, issue #449).
 *
 * Not an [`AssetRow`], and the reason is the same one [`MonitoredAsset`]
 * gives: this is a line in a list of gaps, and an `AssetRow` would drag the
 * health rollup and the linked-work count through a read whose whole answer
 * is *which thing, where it lives, and what kind of thing it is*. It carries
 * the **path**, which `AssetRow` does not, because that is what tells two
 * containers called `postgres` apart -- and it is the field the criterion
 * asks for by name.
 */
export type UnmonitoredAsset = { 
/**
 * `asset:<uuid>` -- the `#/asset/<id>` address the name opens.
 */
id: string, 
/**
 * One of [`knobas_core::asset::TYPES`], and what the tab's type filter
 * narrows by.
 */
type_id: string, 
/**
 * What that type is called -- resolved here, [`AssetRow`]'s rule, so the
 * filter's labels are not a second copy of the table.
 */
type_label: string, 
/**
 * The type's two-character chip.
 */
monogram: string, name: string, 
/**
 * The ancestors' names, outermost first, `" / "` between; `null` for an
 * asset at the top of the estate. [`MonitoredAsset::path`]'s field and
 * its spelling.
 */
path: string | null, 
/**
 * The environment in force, own or inherited (#431), as the new shell's
 * *Assets nothing watches* tab draws it (#864); `null` where nothing
 * above it sets one.
 */
environment: string | null, 
/**
 * The owner in force, the same way.
 */
owner: string | null, 
/**
 * A route's URL (#976); `null` on an asset. A route is listed here as a row
 * of type `route` whose `id` is `route:<uuid>`, whose `path` is where the
 * exposing asset sits (the asset's own name last) and whose environment and
 * owner are that asset's, in force.
 */
url: string | null, 
/**
 * The newest *Create monitor* on its way for a name this asset or route
 * records (#1959), as the queue holds it; `null` where none is. So a
 * reload before the poll draws *Monitor queued* rather than offering the
 * create again, which Uptime Kuma would take as a second monitor.
 * [`ON_ITS_WAY`] says which creates are on their way.
 */
monitor_on_its_way: QueuedWrite | null, };

/**
 * One monitor name on one asset that answers to nothing in the mirror.
 *
 * [`MonitorLink`]'s negative, and a group of its own because the reader's
 * second question about the file's monitor names is *which of them found
 * nothing* -- the names spec #427 keeps on the asset for the next import to
 * resolve.
 *
 * **Neither [`ImportPreview::changes`] nor [`ImportPreview::monitor_links`]
 * can answer it.** `changes` lists what an apply would *write*, so a name
 * already on the asset is absent from it by construction; the second preview
 * of an unchanged file has an empty `changes`, an empty `monitor_links` and
 * every unresolved name still unresolved. Reported on every preview, then,
 * and not only on the one that first kept the name.
 */
export type UnresolvedMonitor = { asset_id: string, asset_name: string, 
/**
 * The Uptime Kuma name, as the file spells it. Nothing in the mirror
 * carries it -- which is a fact about *this moment*: the M4.1 sync or the
 * next import resolves it the instant Kuma publishes a monitor by that
 * name.
 */
monitor_name: string, };

/**
 * One uptime ratio, at the window Kuma computed it over.
 */
export type UptimeRatio = { 
/**
 * Kuma's own label: `1d`, `30d`, `365d`.
 */
window: string, 
/**
 * `0.0` to `1.0`, as Kuma publishes it -- not a percentage.
 */
ratio: number, };

/**
 * Who can reach a route.
 *
 * Two values, because the question a reader asks of a URL is *can somebody
 * outside open this* -- and every finer shade (which VPN, which network,
 * which allow-list) is a fact with a name of its own and belongs in a
 * property. `internal` is the default: it is the safe reading of a route
 * nobody has classified, and `0018`'s `route_visibility_chk` is the same two
 * spellings, kept in step by
 * [`tests::every_visibility_is_one_the_schema_accepts`].
 *
 * - `internal`: reachable from inside the estate. What an unclassified route
 *   reads as.
 * - `public`: reachable from outside it.
 *
 * The values are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/assets.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type Visibility = "internal" | "public";
