<!--
  PROTOTYPE: the ⌘K launcher rethought (clickdummy, 2026-11-02). Not production.

  One question: does "⌘K again opens the selected item's actions" feel right,
  and do findable views and sets, `doc:` and an inline link close the gaps
  Björn named? Mounted by `App.svelte` instead of `Launcher.svelte` under
  `?launcher-next` (dev only); everything else in the shell is the real thing.

  What is real here: `TfDialog`, `QueryBox`, `Chips`, `Results`, `Row`,
  `Board`, `Help`, the `Session`, the `>` palette, the `?` card, Save as list,
  and every write (`create_link`, `save_link_set`) goes through the real IPC.
  What is a shortcut, each marked PROTOTYPE below:
    - `doc:` is rewritten to `kind:page` before asking (ruling P2 says the
      backend parses; the real change is one prefix in `query.rs`);
    - the Views and Sets groups are built here from `ticket_views` and
      `link_sets` (the real change indexes both in `knobas-search`);
    - the "Search Docs for …" row stores its words and opens `#/docs`; the
      Docs page does not read them yet;
    - the sheet's actions are a fixed list per kind, not adapter-declared.

  Screenshots: `?launcher-open=sepa` opens with that query, `&launcher-sheet=1`
  opens the sheet on the row at that index once results land.
-->
<script lang="ts">
  import { onMount, tick } from "svelte";

  import {
    createLink,
    createSmartList as defaultCreateSmartList,
    deleteSmartList as defaultDeleteSmartList,
    filterSuggestions as defaultFilterSuggestions,
    launcherHome as defaultHome,
    ipcErrorMessage,
    linkSets,
    renameSmartList as defaultRenameSmartList,
    saveLinkSet,
    search as defaultSearch,
    ticketViews,
  } from "../../ipc";
  import type { LinkSetMemberKind, LinkSetRow } from "../../ipc/entity";
  import type { ResultGroup, SearchHit, SearchQuery, SearchResponse, TicketViewRow } from "../../ipc/search";
  import type { CredentialHealth, SourceSyncStatus } from "../../ipc/sources";
  import { drawn } from "../../detail/relations";
  import type { HealthPhase } from "../../shell/health.svelte";
  import { canBeTarget } from "../../shell/timer";
  import { push } from "../../shell/toasts.svelte";
  import { entityAddress } from "../../tf/address";
  import { counted } from "../../tf/counted";
  import { opener } from "../../tf/inbox/inbox";
  import type { LinkTarget } from "../../tf/job/current.svelte";
  import { jobHash } from "../../tf/route";
  import TfDialog from "../../tf/settings/TfDialog.svelte";
  import { keyOf } from "../../time/format";
  import Board from "../Board.svelte";
  import Chips from "../Chips.svelte";
  import Help from "../Help.svelte";
  import QueryBox from "../QueryBox.svelte";
  import Results from "../Results.svelte";
  import Row from "../Row.svelte";
  import { type LauncherAction, filterActions, navigationActions } from "../actions";
  import type { InboxPreview, LauncherRow, TodayTime } from "../rows";
  import { Session, type SessionPorts } from "../session.svelte";

  let {
    open = $bindable(false),
    sources,
    runs = [],
    sourcesRead,
    inbox,
    today,
    actions = [],
    onnavigate,
    onclose,
    openEntity,
    onlink,
    ontimer,
    ports,
    now,
  }: {
    open?: boolean;
    sources?: CredentialHealth[] | undefined;
    runs?: SourceSyncStatus[];
    sourcesRead?: { phase: HealthPhase; error: string | null; retry: () => void } | undefined;
    inbox?: InboxPreview | undefined;
    today?: TodayTime | undefined;
    actions?: LauncherAction[];
    onnavigate: (hash: string) => void;
    onclose: () => void;
    /** The chosen job (`breadcrumbJob`), which the sheet links to. */
    openEntity?: { entityId: string; label: string } | undefined;
    /** Unused: the sheet writes the link itself, which is the thing being tried. Declared so `App.svelte` mounts both launchers alike. */
    onlink?: ((target: LinkTarget) => void) | undefined;
    ontimer: ((targetId: string, targetTitle: string) => void) | undefined;
    ports?: Partial<SessionPorts>;
    now?: Date | undefined;
  } = $props();
  // svelte-ignore state_referenced_locally
  void onlink;

  // ---- PROTOTYPE: the local corpus the backend does not index yet ----------

  let views: TicketViewRow[] = [];
  let sets = $state<LinkSetRow[]>([]);

  async function loadLocal() {
    try {
      [views, sets] = await Promise.all([ticketViews(), linkSets()]);
    } catch {
      views = [];
      sets = [];
    }
  }

  const PLACEHOLDER_GROUP = { updated_at: null, state: null, rank: 1, snippet: [] as SearchHit["snippet"] };

  /** A saved ticket view as a hit: its address is the key, its conditions the path line. */
  function viewGroup(needle: string, at: string): ResultGroup | null {
    const hits: SearchHit[] = views
      .filter((view) => `${view.name} ${view.group} ${view.query}`.toLowerCase().includes(needle))
      .map((view) => ({
        ...PLACEHOLDER_GROUP,
        entity_id: `view:${view.id}`,
        kind: "view",
        source_id: "local",
        title: view.name,
        synced_at: at,
        path: [view.group, view.layout, view.query].filter((part) => part !== "").join(" · "),
      }));
    return hits.length === 0 ? null : { kind: "view", label: "View", plural: "Views", monogram: "VW", total: hits.length, hits };
  }

  /** A link set as a hit: what it holds and where it applies, on the path line. */
  function setGroup(needle: string, at: string): ResultGroup | null {
    const hits: SearchHit[] = sets
      .filter((set) => `${set.name} ${set.description}`.toLowerCase().includes(needle))
      .map((set) => ({
        ...PLACEHOLDER_GROUP,
        entity_id: `set:${set.id}`,
        kind: "set",
        source_id: "local",
        title: set.name,
        synced_at: at,
        path: `${counted(set.members.length, "member", "members")} · ${counted(set.patterns.length, "pattern", "patterns")} · on ${counted(set.tickets, "ticket", "tickets")}`,
      }));
    return hits.length === 0 ? null : { kind: "set", label: "Set", plural: "Sets", monogram: "LS", total: hits.length, hits };
  }

  /** The last row of a `doc:` answer: the same words, in the Docs area. */
  function docsRow(response: SearchResponse, at: string): SearchResponse {
    const text = response.interpreted.text.trim();
    if (text === "") return response;
    const hit: SearchHit = {
      ...PLACEHOLDER_GROUP,
      entity_id: `docs:${text}`,
      kind: "docs",
      source_id: "local",
      title: `Search Docs for “${text}”`,
      synced_at: at,
      path: "Every space, with the outline and the hits in the page",
    };
    const group: ResultGroup = { kind: "docs-search", label: "Docs", plural: "Docs area", monogram: "PG", total: 1, hits: [hit] };
    return { ...response, groups: [...response.groups, group] };
  }

  // svelte-ignore state_referenced_locally
  const realSearch = ports?.search ?? defaultSearch;

  /** The search port with the prototype's three additions layered over the real answer. */
  async function searchNext(query: SearchQuery): Promise<SearchResponse> {
    const docs = /^doc:/i.test(query.raw.trim());
    // PROTOTYPE: the frontend rewrites grammar here, against ruling P2.
    const sent = docs ? { ...query, raw: query.raw.trim().replace(/^doc:\s*/i, "kind:page ") } : query;
    const response = await realSearch(sent);
    const at = (now ?? new Date()).toISOString();
    if (docs) return docsRow(response, at);
    if (response.interpreted.prefix !== null) return response;
    const needle = response.interpreted.text.trim().toLowerCase();
    if (needle === "") return response;
    const local = [viewGroup(needle, at), setGroup(needle, at)].filter((group): group is ResultGroup => group !== null);
    if (local.length === 0) return response;
    return {
      ...response,
      groups: [...local, ...response.groups],
      total: response.total + local.reduce((n, group) => n + group.total, 0),
    };
  }

  // svelte-ignore state_referenced_locally
  const session = new Session({
    search: searchNext,
    launcherHome: defaultHome,
    createSmartList: defaultCreateSmartList,
    renameSmartList: defaultRenameSmartList,
    deleteSmartList: defaultDeleteSmartList,
    filterSuggestions: defaultFilterSuggestions,
    ...ports,
    // The wrapper wins over an injected port: it wraps whatever was injected.
    ...(ports?.search ? { search: searchNext } : {}),
  });

  const health = $derived(sources ?? session.home?.sources ?? []);
  const sourcesPhase = $derived(sourcesRead?.phase ?? "ready");

  let box = $state<ReturnType<typeof QueryBox> | null>(null);
  let renaming = $state<string | null>(null);

  // ---- the sheet: what the second ⌘K opens -------------------------------

  const RELATIONS: { id: string; label: string }[] = [
    { id: "related", label: "related" },
    { id: "implements", label: "implements" },
    { id: "documents", label: "documents" },
    { id: "runs-on", label: "runs on" },
    { id: "blocks", label: "blocks" },
    { id: "depends-on", label: "depends on" },
  ];

  interface SheetItem {
    id: string;
    label: string;
    detail: string;
    /** The one item with the relation chips on it. */
    link?: boolean;
    run: () => void | Promise<void>;
  }

  interface Sheet {
    rowId: string;
    row: LauncherRow;
    /** What the heading names. */
    title: string;
    selected: number;
    relation: number;
    /** `sets` is the second level: pick a set for *Add to a set…*. */
    level: "root" | "sets";
  }

  let sheet = $state<Sheet | null>(null);

  interface Entity {
    entityId: string;
    title: string;
    kind: string;
  }

  /** The entity a row stands for; a view, a set, a list and the docs row are not one. */
  function entityOf(row: LauncherRow): Entity | null {
    switch (row.kind) {
      case "hit":
        if (row.hit.kind === "view" || row.hit.kind === "set" || row.group.kind === "docs-search") return null;
        return { entityId: row.hit.entity_id, title: row.hit.title, kind: row.hit.kind };
      case "recent":
        return { entityId: row.row.entity_id, title: row.row.title, kind: row.row.kind };
      default:
        return null;
    }
  }

  /** What a row is called in the footer and the sheet's heading. */
  function nameOf(row: LauncherRow): string {
    const entity = entityOf(row);
    if (entity) return entity.kind === "ticket" ? keyOf(entity.entityId) : entity.title;
    if (row.kind === "hit") return row.hit.title;
    if (row.kind === "list") return row.list.label;
    return "";
  }

  /** The set member kind an entity can be, or `null` for a kind sets do not hold. */
  function memberKindOf(kind: string): LinkSetMemberKind | null {
    switch (kind) {
      case "page":
      case "repo":
      case "pipeline":
      case "asset":
      case "monitor":
        return kind;
      default:
        return null;
    }
  }

  function relationLabel(index: number): string {
    return RELATIONS[index]?.label ?? "related";
  }

  async function writeLink(entity: Entity, relationIndex: number) {
    const job = openEntity;
    if (!job) return;
    const relation = RELATIONS[relationIndex]?.id ?? "related";
    const link = drawn(relation, job.entityId, entity.entityId);
    try {
      await createLink(link.fromId, link.toId, link.relation);
      const name = entity.kind === "ticket" ? keyOf(entity.entityId) : entity.title;
      push({ text: `Linked ${name} to ${job.label} as ${relationLabel(relationIndex)}` });
      close();
    } catch (cause) {
      sheet = null;
      session.error = ipcErrorMessage(cause);
    }
  }

  /** A fixed member for a kind sets hold; a manual rule for a ticket. */
  async function putInSet(set: LinkSetRow, entity: Entity) {
    const member = memberKindOf(entity.kind);
    const draft = {
      id: set.id,
      name: set.name,
      description: set.description,
      members: set.members.map((row) => ({ kind: row.kind, entity_id: row.entity_id })),
      patterns: set.patterns.map((row) => ({ kind: row.kind, query: row.query })),
      rules: set.rules.map((row) => ({ kind: row.kind, value: row.value })),
    };
    if (member) draft.members.push({ kind: member, entity_id: entity.entityId });
    else draft.rules.push({ kind: "manual", value: entity.entityId });
    try {
      await saveLinkSet(draft);
      push({ text: member ? `Added ${entity.title} to ${set.name}` : `${set.name} now shows on ${keyOf(entity.entityId)}` });
      close();
    } catch (cause) {
      sheet = null;
      session.error = ipcErrorMessage(cause);
    }
  }

  async function copy(text: string, said: string) {
    try {
      await navigator.clipboard.writeText(text);
      push({ text: said });
    } catch {
      push({ text: "The clipboard refused.", tone: "err" });
    }
    close();
  }

  /** The sheet's rows for `row`, in the order they are drawn. Empty: no sheet. */
  function itemsFor(row: LauncherRow): SheetItem[] {
    if (row.kind === "list") {
      if (!row.list.saved) return [];
      return [
        { id: "rename", label: "Rename", detail: "The name becomes a field on the board", run: () => startRename(row.list.id) },
        { id: "delete", label: "Delete", detail: "The saved query is gone; there is no undo", run: () => { sheet = null; void session.deleteList(row.list.id); } },
      ];
    }
    if (row.kind === "hit" && row.hit.kind === "view") {
      const id = keyOf(row.hit.entity_id);
      return [
        { id: "open", label: "Open", detail: "The Tickets page on this view", run: () => activate(row) },
        { id: "copy", label: "Copy address", detail: `#/tickets/${id}`, run: () => void copy(`#/tickets/${id}`, "Copied the view's address") },
      ];
    }
    if (row.kind === "hit" && row.hit.kind === "set") {
      const id = keyOf(row.hit.entity_id);
      const items: SheetItem[] = [{ id: "open", label: "Open", detail: "The set, its members and its rules", run: () => activate(row) }];
      const job = openEntity;
      const set = sets.find((candidate) => candidate.id === id);
      if (job && set) {
        items.push({
          id: "show",
          label: `Show on ${job.label}`,
          detail: "A rule for this one ticket, by hand",
          run: () => putInSet(set, { entityId: job.entityId, title: job.label, kind: "ticket" }),
        });
      }
      return items;
    }
    const entity = entityOf(row);
    if (!entity) return [];
    const items: SheetItem[] = [];
    const job = openEntity;
    if (job && entity.entityId !== job.entityId) {
      items.push({
        id: "link",
        label: `Link to ${job.label}`,
        detail: "",
        link: true,
        run: () => writeLink(entity, sheet?.relation ?? 0),
      });
    }
    if (sets.length > 0) {
      const member = memberKindOf(entity.kind);
      if (member || entity.kind === "ticket") {
        items.push({
          id: "set",
          label: member ? "Add to a set…" : `Show a set on ${keyOf(entity.entityId)}…`,
          detail: member ? "A fixed member, shown on every ticket the set applies to" : "The set's members appear on this ticket",
          run: () => {
            if (sheet) sheet = { ...sheet, level: "sets", selected: 0 };
          },
        });
      }
    }
    if (ontimer && canBeTarget({ entityId: entity.entityId })) {
      const start = ontimer;
      items.push({ id: "timer", label: "Start timer", detail: "Stops the running one first", run: () => { start(entity.entityId, entity.title); close(); } });
    }
    if (entity.kind === "ticket") {
      items.push({
        id: "work",
        label: "Work on this",
        detail: `Makes ${keyOf(entity.entityId)} the job and opens its page`,
        run: () => { close(); onnavigate(jobHash(entity.entityId)); },
      });
      items.push({ id: "copy", label: "Copy key", detail: keyOf(entity.entityId), run: () => void copy(keyOf(entity.entityId), `Copied ${keyOf(entity.entityId)}`) });
    } else {
      items.push({ id: "copy", label: "Copy title", detail: entity.title, run: () => void copy(entity.title, "Copied the title") });
    }
    return items;
  }

  /** The second level: one row per set. */
  const setItems = $derived.by((): SheetItem[] => {
    if (!sheet) return [];
    const entity = entityOf(sheet.row);
    if (!entity) return [];
    return sets.map((set) => ({
      id: `set:${set.id}`,
      label: set.name,
      detail: set.description,
      run: () => putInSet(set, entity),
    }));
  });

  /** The first level, derived live: the chosen job and the sets can arrive after the sheet opened. */
  const rootItems = $derived(sheet ? itemsFor(sheet.row) : []);
  const shown = $derived(sheet ? (sheet.level === "sets" ? setItems : rootItems) : []);

  function openSheet() {
    const row = session.current;
    if (!row) return;
    if (itemsFor(row).length === 0) return;
    sheet = { rowId: row.id, row, title: nameOf(row), selected: 0, relation: 0, level: "root" };
  }

  // The sheet belongs to one row: moving off it, or typing, closes it.
  $effect(() => {
    if (sheet && session.current?.id !== sheet.rowId) sheet = null;
  });

  // ---- ids, scrolling, the board's extras, the `>` palette ---------------

  const uid = $props.id();
  const optionId = (index: number) => `${uid}-opt-${index}`;
  const sheetOptionId = (index: number) => `${uid}-sheet-${index}`;
  const resultsId = `${uid}-results`;
  const sheetId = `${uid}-sheet`;

  const activeOption = $derived.by(() => {
    const row = session.current;
    if (!row || session.error) return undefined;
    if (row.kind === "list" && row.list.id === renaming) return undefined;
    return optionId(session.selected);
  });
  const controlled = $derived(sheet ? { list: sheetId, option: sheetOptionId(sheet.selected) } : { list: resultsId, option: activeOption });

  $effect(() => {
    void session.rows;
    if (activeOption) document.getElementById(activeOption)?.scrollIntoView?.({ block: "nearest" });
  });
  $effect(() => {
    if (sheet) document.getElementById(sheetOptionId(sheet.selected))?.scrollIntoView?.({ block: "nearest" });
  });

  $effect.pre(() => {
    session.extras = { inbox: inbox ?? null, today: today ?? null };
  });

  $effect(() => {
    const typed = session.response?.interpreted.text ?? "";
    session.actions = filterActions([...navigationActions(), ...actions], typed);
  });

  $effect(() => {
    if (open) {
      void session.loadHome();
      void loadLocal();
      void tick().then(() => box?.focus({ select: true }));
    } else {
      sheet = null;
      session.dispose();
    }
  });

  // PROTOTYPE: the screenshot affordances.
  const params = new URLSearchParams(location.search);
  let autoSheet = params.has("launcher-sheet");
  $effect(() => {
    if (autoSheet && open && session.mode === "results" && session.response && !sheet) {
      autoSheet = false;
      session.selected = Number(params.get("launcher-sheet") || 0);
      openSheet();
    }
  });

  onMount(() => {
    function onkeydown(event: KeyboardEvent) {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        if (!open) {
          open = true;
        } else if (sheet) {
          sheet = null;
        } else {
          openSheet();
        }
      }
    }
    window.addEventListener("keydown", onkeydown);
    const preset = params.get("launcher-open");
    if (preset !== null) {
      open = true;
      void tick().then(() => session.set(preset));
    }
    return () => {
      window.removeEventListener("keydown", onkeydown);
      session.dispose();
    };
  });

  function close() {
    open = false;
    sheet = null;
    session.dispose();
    onclose();
  }

  function addressOf(row: LauncherRow): string | null {
    switch (row.kind) {
      case "hit":
        // PROTOTYPE: a view and a set are addresses the search does not know yet.
        if (row.hit.kind === "view") return `#/tickets/${encodeURIComponent(keyOf(row.hit.entity_id))}`;
        if (row.hit.kind === "set") return `#/sets/${encodeURIComponent(keyOf(row.hit.entity_id))}`;
        if (row.group.kind === "docs-search") {
          // PROTOTYPE: the Docs page does not read this yet; the hand-off is the implementer's.
          sessionStorage.setItem("knobas.prototype.docs-search", keyOf(row.hit.entity_id));
          return "#/docs";
        }
        return entityAddress(row.hit.kind, row.hit.entity_id, row.hit.title);
      case "recent":
        return entityAddress(row.row.kind, row.row.entity_id, row.row.title);
      case "action":
        return row.action.hash;
      case "inbox":
        return opener(row.item)?.href ?? "#/inbox";
      case "today":
        return "#/time";
      case "list":
      case "syntax":
        return null;
    }
  }

  function startRename(id: string) {
    sheet = null;
    renaming = id;
  }

  function finishRename(id: string, label: string | null) {
    renaming = null;
    if (label !== null && label.trim() !== "") void session.renameList(id, label);
    box?.focus();
  }

  function activate(row: LauncherRow) {
    if (row.kind === "list") {
      if (row.list.needs_attention) return;
      session.set(`list:${row.list.id}`);
      box?.focus();
      return;
    }
    if (row.kind === "syntax") {
      session.set(row.entry.insert);
      box?.focus();
      return;
    }
    const hash = addressOf(row);
    if (!hash) return;
    close();
    onnavigate(hash);
  }

  function sheetKey(event: KeyboardEvent, current: Sheet) {
    const items = shown;
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        current.selected = Math.min(items.length - 1, current.selected + 1);
        return;
      case "ArrowUp":
        event.preventDefault();
        current.selected = Math.max(0, current.selected - 1);
        return;
      case "ArrowRight":
      case "ArrowLeft":
        if (items[current.selected]?.link) {
          event.preventDefault();
          const step = event.key === "ArrowRight" ? 1 : -1;
          current.relation = (current.relation + step + RELATIONS.length) % RELATIONS.length;
        }
        return;
      case "Enter":
        event.preventDefault();
        void items[current.selected]?.run();
        return;
      case "Tab":
        event.preventDefault();
        sheet = null;
        return;
      case "Escape":
        event.preventDefault();
        event.stopPropagation();
        if (current.level === "sets") sheet = { ...current, level: "root", selected: Math.max(0, rootItems.findIndex((item) => item.id === "set")) };
        else sheet = null;
        return;
      default:
    }
  }

  function onkeydown(event: KeyboardEvent) {
    if (sheet) {
      sheetKey(event, sheet);
      return;
    }
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        session.move(1);
        return;
      case "ArrowUp":
        event.preventDefault();
        session.move(-1);
        return;
      case "Enter": {
        event.preventDefault();
        const row = session.current;
        if (row) activate(row);
        return;
      }
      case "Tab":
        event.preventDefault();
        openSheet();
        return;
      case "Escape":
        event.preventDefault();
        event.stopPropagation();
        if (session.raw !== "") session.set("");
        else close();
        return;
      default:
    }
  }

  const placeholder = "Search everything, or type > for actions, # tickets, doc: pages, @ people, / source, ? help";
  const footnote = $derived(`mirror · ${counted(session.home?.pending_writes ?? 0, "pending write", "pending writes")}`);
  const interpreted = $derived(session.response?.interpreted ?? null);
  /** What the footer says the second ⌘K would open, or nothing when the row has no sheet. */
  const sheetHint = $derived.by(() => {
    const row = session.current;
    if (!row || sheet) return null;
    return itemsFor(row).length > 0 ? nameOf(row) : null;
  });
  const pinned = $derived(sheet ? entityOf(sheet.row) : null);
  const pinnedHit = $derived.by(() => {
    if (!sheet) return null;
    if (sheet.row.kind === "hit") return sheet.row.hit;
    if (sheet.row.kind === "recent") return { ...sheet.row.row, snippet: [] };
    return null;
  });
</script>

{#if open}
  <TfDialog title="Search and actions" className="tf-launch" onclose={close}>
    {#snippet body()}
      <QueryBox
        bind:this={box}
        value={session.raw}
        {placeholder}
        pending={session.pending}
        {footnote}
        controls={controlled.list}
        active={controlled.option}
        oninput={(value) => { sheet = null; session.type(value); }}
        {onkeydown}
        onclose={close}
      />

      {#if sheet}
        <!-- The row the sheet acts on, lifted under the box; the box keeps its words, dimmed. -->
        <div class="pinned">
          {#if pinnedHit}
            <Row
              entityId={pinnedHit.entity_id}
              kind={pinnedHit.kind}
              sourceId={pinnedHit.source_id}
              title={pinnedHit.title}
              syncedAt={pinnedHit.synced_at}
              path={pinnedHit.path}
              state={pinnedHit.state}
              sources={health}
              {now}
              id={`${uid}-pinned`}
              selected={false}
              onopen={() => {}}
              onhover={() => {}}
            />
          {:else}
            <p class="lead">{sheet.title}</p>
          {/if}
        </div>
        <div id={sheetId} class="sheet" role="listbox" tabindex="-1" aria-label="Actions on {sheet.title}">
          {#if sheet.level === "sets"}
            <div class="tf-ln-h"><span class="tf-ln-lab">{pinned && memberKindOf(pinned.kind) ? "Add to" : "Show on this ticket"}</span></div>
          {/if}
          {#each shown as item, index (item.id)}
            {#if index > 0 && sheet.level === "root" && (item.id === "timer" || item.id === "copy") && shown[index - 1]?.id !== "timer"}
              <div class="cut"></div>
            {/if}
            {#if item.link}
              <!-- Not a button: the relation chips inside it are buttons of their own. -->
              <div
                id={sheetOptionId(index)}
                class="tf-ln-opt linkrow"
                class:tf-ln-on={index === sheet.selected}
                role="option"
                tabindex="-1"
                aria-selected={index === sheet.selected}
                onmouseenter={() => sheet && (sheet.selected = index)}
              >
                <button type="button" class="tf-ln-nm linkbtn" onclick={() => void item.run()}>Link to <span class="job">{openEntity?.label}</span></button>
                <span class="rel" role="radiogroup" aria-label="Relation">
                  {#each RELATIONS as relation, r (relation.id)}
                    <button
                      type="button"
                      class:pick={r === sheet.relation}
                      role="radio"
                      aria-checked={r === sheet.relation}
                      tabindex="-1"
                      onclick={() => { if (sheet) sheet.relation = r; }}
                    >{relation.label}</button>
                  {/each}
                </span>
              </div>
            {:else}
              <button
                id={sheetOptionId(index)}
                class="tf-ln-opt"
                class:tf-ln-on={index === sheet.selected}
                role="option"
                aria-selected={index === sheet.selected}
                onclick={() => void item.run()}
                onmouseenter={() => sheet && (sheet.selected = index)}
              >
                <span class="tf-ln-nm">{item.label}</span>
                <span class="tf-ln-s">{item.detail}</span>
              </button>
            {/if}
          {/each}
          {#if shown.length === 0}
            <p class="tf-ln-none">Nothing to do here.</p>
          {/if}
        </div>
      {:else}
        {#if interpreted}
          <Chips {interpreted} sources={health} sourcesRead={sourcesPhase === "ready"} />
        {/if}

        <div id={resultsId} class="tf-ln-list" role="listbox" tabindex="-1" aria-label="Results">
          {#if session.error}
            <p class="tf-ln-none tf-ln-err">{session.error}</p>
            {#if !session.home}
              <button class="tf-ln-opt" onclick={() => void session.loadHome()}>
                <span class="tf-ln-nm">Retry</span>
                <span class="tf-ln-s">Read the board again.</span>
              </button>
            {/if}
          {:else if session.mode === "board"}
            {#if session.home}
              <Board
                home={session.home}
                inboxCount={inbox?.count ?? 0}
                sources={health}
                {runs}
                reading={sourcesRead}
                rows={session.rows}
                selected={session.selected}
                {renaming}
                {optionId}
                {now}
                onopen={activate}
                onhover={(index) => (session.selected = index)}
                onstartrename={startRename}
                onrename={finishRename}
                ondelete={(id) => void session.deleteList(id)}
              />
            {:else}
              <p class="tf-ln-none">Reading the mirror…</p>
            {/if}
          {:else if session.mode === "help"}
            <Help rows={session.rows} selected={session.selected} {optionId} onopen={activate} onhover={(index) => (session.selected = index)} />
          {:else if session.mode === "actions"}
            <div class="tf-ln-h"><span class="tf-ln-lab">Go to</span></div>
            {#each session.rows as row, i (row.id)}
              {#if row.kind === "action"}
                <button
                  id={optionId(i)}
                  class="tf-ln-opt"
                  class:tf-ln-on={i === session.selected}
                  role="option"
                  aria-selected={i === session.selected}
                  onclick={() => activate(row)}
                  onmouseenter={() => (session.selected = i)}
                >
                  <span class="tf-ln-nm">{row.action.label}</span>
                  <span class="tf-ln-s">{row.action.detail}</span>
                </button>
              {/if}
            {/each}
            {#if session.rows.length === 0}
              <p class="tf-ln-none">No action matches.</p>
            {/if}
          {:else if session.response}
            <Results
              response={session.response}
              rows={session.rows}
              selected={session.selected}
              {optionId}
              sources={health}
              {now}
              onopen={activate}
              onhover={(index) => (session.selected = index)}
            />
          {:else}
            <p class="tf-ln-none">Searching…</p>
          {/if}
        </div>
      {/if}
    {/snippet}

    {#snippet footer()}
      {#if sheet}
        <span><kbd>↑↓</kbd> move</span>
        {#if shown[sheet.selected]?.link}<span><kbd>←→</kbd> relation</span>{/if}
        <span><kbd>↵</kbd> {shown[sheet.selected]?.link ? "link" : "do it"}</span>
        <span><kbd>Esc</kbd> {sheet.level === "sets" ? "back" : "back to results"}</span>
        <span class="tf-spacer"></span>
        <span class="tf-ln-took">{sheet.title} · {counted(shown.length, "action", "actions")}</span>
      {:else}
        <span><kbd>↑↓</kbd> move</span>
        <span><kbd>↵</kbd> open</span>
        {#if sheetHint}
          <span><kbd>⌘K</kbd> actions on {sheetHint}</span>
        {/if}
        <span><kbd>Esc</kbd> back one step</span>
        {#if session.canSave}
          <button class="tf-btn tf-sm tf-ln-save" onclick={() => void session.saveCurrent(session.raw.trim())} disabled={session.savedId !== null}>
            {session.savedId === null ? "Save as list" : "Saved as list"}
          </button>
        {/if}
        <span class="tf-spacer"></span>
        {#if session.response}
          <span class="tf-ln-took">{session.response.total} matches · {session.response.took_ms} ms</span>
        {/if}
      {/if}
    {/snippet}
  </TfDialog>
{/if}

<style>
  /* PROTOTYPE rules. The shipped ones are `tf/css/launcher.css`; these would join them. */
  .pinned { flex: none; border-bottom: 1px solid var(--rule); }
  .lead { padding: 10px 18px; font-size: var(--t-sm); color: var(--ink); }
  .sheet { flex: 1; min-height: 0; overflow: auto; padding: 4px 0; }
  .cut { height: 1px; background: var(--rule); margin: 4px 0; }
  .job {
    display: inline-block; background: var(--card); color: var(--card-ink);
    font-family: var(--mono); font-variant-ligatures: none; font-size: var(--t-xs); font-weight: 600;
    padding: 0 6px; border-radius: 3px;
  }
  .rel { display: flex; flex-wrap: wrap; gap: 4px; align-items: center; }
  .linkrow { cursor: default; }
  .linkbtn { text-align: left; padding: 0; cursor: pointer; }
  .rel button {
    font-size: var(--t-xs); padding: 1px 8px; border: 1px solid var(--rule); border-radius: 3px; color: var(--ink-2);
    cursor: pointer;
  }
  .rel button:hover { border-color: var(--ink-2); color: var(--ink); }
  .rel .pick, .rel .pick:hover { background: var(--ink); color: var(--sheet); border-color: var(--ink); }
</style>
