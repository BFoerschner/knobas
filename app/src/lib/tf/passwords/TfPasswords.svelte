<!--
  Passwords (ADR-0056): a password manager's entries, from the clickdummy at `mockups/passwords-2/`.

  **One read holds the page** (`passwords_held`): every source, folder and a compact row per entry. A folder press and
  a filter keystroke ask nothing; the tree, the panels, a path and the filter's hits are computed here (`model.ts`).
  A return to the page draws from what was read last (`held.svelte.ts`) and reads again behind.

  - **The tree** is one flat list of the rows on show (All entries, then each open fold's folders), one `Windowed`,
    with the Docs rail's look. A press opens the folder and its fold; the chevron folds alone; a first press never
    closes. ↑ ↓ walk it, → opens a fold and steps in, ← closes it and steps out. One `Tab` stop.
  - **The list**: a folder lists everything under it, its own entries first, then a panel per folder below that holds
    entries; one further down with more than six rows draws five and *Show all*. At the top, *Used recently* first. One
    flat list of lines (a panel's head, a row, *Show all*), one `Windowed`.
  - **The entry** beside it, the list and the entry one block capped at 1,438 px and centred (`TfPwEntry`).
  - **Keys:** `/` filter; ↑ ↓ or `j` `k` move in the list; `c` copy the password, `s` show it, `u` the username, `o`
    the one-time code, `a` the address; `Esc` clears the filter, else goes up a folder. `r` does nothing here: a sync
    is something somebody else can read in a log, so it is a button a person aims at.
  - **Leaving the page** hides everything shown. It does not clear the clipboard: the person copied to paste elsewhere.

  The page names no source and no vendor. With more than one password source, each one's folders sit under a top
  named after it: the one place a source's name appears.
-->
<script lang="ts">
  import type { Snippet } from "svelte";
  import { untrack } from "svelte";
  import type { Held, PwEntry, PwSource } from "../../ipc/passwords";
  import { Windowed } from "../../shell/windowed.svelte";
  import { inTextField } from "../../shell/keys";
  import { push } from "../../shell/toasts.svelte";
  import { stampLabel, clockLabel } from "../../time/format";
  import TfPwEntry from "./TfPwEntry.svelte";
  import TfPwWrite from "./TfPwWrite.svelte";
  import type { Layers } from "../layers.svelte";
  import { folds, heldNow, readHeld } from "./held.svelte";
  import {
    CUT,
    CUT_OVER,
    CANNOT_READ,
    HAS_CODE,
    NEEDS_APPROVAL,
    bare,
    chainOf,
    commentFolder,
    expiry,
    holding,
    marks,
    model,
    panels,
    plural,
    sourceNode,
    sourceWord,
    treeRows,
    type Entry,
    type Node,
  } from "./model";
  import { realPasswordPorts, type PasswordPorts } from "./ports";

  let {
    ready,
    rest,
    visible = true,
    timedTicket = null,
    layers,
    sync,
    syncRevision = 0,
    ports = realPasswordPorts,
  }: {
    ready: boolean;
    rest: string[];
    visible?: boolean;
    timedTicket?: string | null;
    layers?: Layers;
    sync?: Snippet;
    syncRevision?: number;
    ports?: PasswordPorts;
  } = $props();

  // ---- the read ----
  // Raw: the read is replaced whole, never changed in place. A deep proxy over 20,000 rows made every read of the
  // model a reactive lookup: the first open took 1.6 s at 6x (the page probe, 2026-11-22).
  let held = $state.raw<Held | null>(heldNow());
  let failed = $state<string | null>(null);
  const m = $derived(held ? model(held) : null);

  async function read(): Promise<void> {
    try {
      held = await readHeld(ports.held);
      failed = null;
    } catch (cause) {
      failed = String((cause as { message?: string })?.message ?? cause);
    }
  }

  // Read when the database answers, again when the role's runs end (`syncRevision`), and when shown again.
  $effect(() => {
    void syncRevision;
    if (!ready || !visible) return;
    untrack(() => void read());
  });

  // Leaving the page forgets what it showed; the clipboard is the person's.
  $effect(() => {
    if (visible) return;
    void ports.hide().catch(() => {});
  });

  // ---- where the reader is ----
  /**
   * The folder a press is going to, drawn in the press's own task rather than after the address comes back through
   * the router (four frames at 4x: the page probe, 2026-11-22). The address catching up clears it.
   */
  let going = $state<string | null>(null);
  $effect.pre(() => {
    void rest;
    going = null;
  });
  const where = $derived.by(() => {
    if (going !== null) return { folder: going as string | null, entry: null as string | null };
    const [kind, id] = rest;
    if (kind === "in" && id) return { folder: id, entry: null as string | null };
    if (kind === "source" && id) return { folder: sourceNode(id), entry: null };
    if (kind === "entry" && id) return { folder: null as string | null, entry: id };
    return { folder: "", entry: null };
  });
  /** The entry the reader picked in this folder, when the address names none. */
  let picked = $state<string | null>(null);
  const openId = $derived(where.entry ?? picked);
  const openEntry = $derived(m && openId ? (m.entries.get(openId) ?? null) : null);
  const node = $derived.by((): Node | null => {
    if (!m) return null;
    if (where.folder !== null) return m.nodes.get(where.folder) ?? m.root;
    const e = openEntry;
    return e && e.folder !== null ? (m.nodes.get(e.folder) ?? m.root) : m.root;
  });

  let query = $state("");
  let filter = $state<HTMLInputElement | null>(null);
  const shownPanels = $derived(m && node ? panels(m, node, query) : []);

  // ---- the tree ----
  /** Folds the reader opened, kept for the session. */
  const opened = folds;
  const treeOpen = $derived.by(() => {
    const all = new Set(opened);
    if (node) for (let at: Node | null = node.parent; at !== null; at = at.parent) all.add(at.id);
    return all;
  });
  const rows = $derived(m ? treeRows(m, treeOpen) : []);
  const holds = $derived(m && openEntry ? holding(m, openEntry) : new Set<string>());
  const treeWin = new Windowed({ count: () => rows.length, key: (i) => rows[i]!.node.id || "~", estimate: 26, from: 40, scrolls: true });

  function pressNode(event: MouseEvent, target: Node): void {
    const chevron = (event.target as HTMLElement | null)?.closest("[data-twist]") !== null;
    if (chevron) {
      if (opened.has(target.id)) opened.delete(target.id);
      else opened.add(target.id);
      return;
    }
    if (target === node && target.kids.length > 0) {
      if (opened.has(target.id)) opened.delete(target.id);
      else opened.add(target.id);
    } else if (target.kids.length > 0) opened.add(target.id);
    go(target);
  }

  function hashOf(target: Node): string {
    if (target.id === "") return "#/passwords";
    if (target.top) return `#/passwords/source/${encodeURIComponent(target.id.slice(4))}`;
    return `#/passwords/in/${encodeURIComponent(target.id)}`;
  }

  function go(target: Node): void {
    picked = null;
    query = "";
    going = target.id;
    location.hash = hashOf(target);
  }

  function treeKey(event: KeyboardEvent): void {
    const buttons = [...(event.currentTarget as HTMLElement).querySelectorAll<HTMLButtonElement>("[role=treeitem]")];
    const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (at === -1) return;
    const row = rows.find((r) => (r.node.id || "~") === buttons[at]!.dataset.id);
    const step = (to: number) => {
      event.preventDefault();
      buttons[Math.max(0, Math.min(buttons.length - 1, to))]?.focus();
    };
    if (event.key === "ArrowDown") step(at + 1);
    else if (event.key === "ArrowUp") step(at - 1);
    else if (event.key === "ArrowRight" && row) {
      event.preventDefault();
      if (row.node.kids.length > 0 && !row.open) opened.add(row.node.id);
      else if (row.open) step(at + 1);
    } else if (event.key === "ArrowLeft" && row) {
      event.preventDefault();
      if (row.open && row.node.id !== "") opened.delete(row.node.id);
      else if (row.node.parent) buttons.find((b) => b.dataset.id === (row.node.parent!.id || "~"))?.focus();
    }
  }

  // ---- the list: one flat list of lines ----
  type Line =
    | { kind: "head"; key: string; panel: number; title: string; pre: string; href: string | null; count: number; comment: boolean; first: boolean }
    | { kind: "row"; key: string; e: Entry; path: boolean; last: boolean }
    | { kind: "more"; key: string; node: Node; count: number }
    | { kind: "none"; key: string; text: string };

  /**
   * The folders whose panel the open entry sits beyond the cut of, so that panel shows whole: a string, so `lines`
   * is built again only when it changes, never on every move of the selection (at 20,000 entries a move rebuilt
   * every panel's lines: the page probe, 2026-11-22).
   */
  const lifted = $derived.by((): string => {
    const e = openId && m ? m.entries.get(openId) : undefined;
    if (!e || !m) return "";
    const out: string[] = [];
    for (const home of [e.folder, ...e.also]) {
      const at = home !== null ? m.nodes.get(home) : undefined;
      if (at && at.own.length > CUT_OVER && at.own.indexOf(e) >= CUT) out.push(at.id);
    }
    return out.join("\n");
  });

  const lines = $derived.by((): Line[] => {
    if (!m || !node) return [];
    const liftedIds = new Set(lifted.split("\n"));
    const out: Line[] = [];
    shownPanels.forEach((p, i) => {
      if (p.kind === "hits") {
        if (p.rows.length === 0) {
          out.push({ kind: "none", key: "none", text: `Nothing matches "${query}". The filter looks at titles, usernames, addresses and folder names.` });
          return;
        }
        out.push({ kind: "head", key: "h:hits", panel: i, title: `${plural(p.rows.length, "entry", "entries")} in every folder`, pre: "", href: null, count: p.rows.length, comment: false, first: true });
        p.rows.forEach((e, n) => out.push({ kind: "row", key: `r:hits:${e.id}`, e, path: true, last: n === p.rows.length - 1 }));
        return;
      }
      if (p.kind === "recent") {
        out.push({ kind: "head", key: "h:recent", panel: i, title: "Used recently", pre: "", href: null, count: p.rows.length, comment: false, first: i === 0 });
        p.rows.forEach((e, n) => out.push({ kind: "row", key: `r:recent:${e.id}`, e, path: true, last: n === p.rows.length - 1 }));
        return;
      }
      const at = p.node!;
      const rel = chainOf(m, at).slice(chainOf(m, node).length);
      const title = rel.length > 0 ? rel[rel.length - 1]!.name : node.kids.length > 0 ? `Directly in ${node.name}` : node.name;
      const pre = rel.length > 1 ? `${rel.slice(0, -1).map((r) => r.name).join(" / ")} / ` : "";
      const cut = rel.length > 0 && p.rows.length > CUT_OVER && !liftedIds.has(at.id) ? CUT : p.rows.length;
      out.push({ kind: "head", key: `h:${at.id}`, panel: i, title, pre, href: rel.length > 0 ? hashOf(at) : null, count: p.rows.length, comment: commentFolder(at) !== null, first: i === 0 });
      p.rows.slice(0, cut).forEach((e, n) => out.push({ kind: "row", key: `r:${at.id}:${e.id}`, e, path: false, last: n === cut - 1 && cut === p.rows.length }));
      if (cut < p.rows.length) out.push({ kind: "more", key: `m:${at.id}`, node: at, count: p.rows.length });
    });
    if (out.length === 0) out.push({ kind: "none", key: "none", text: `Nothing in ${node.name} yet.` });
    return out;
  });
  const listWin = new Windowed({
    count: () => lines.length,
    key: (i) => lines[i]!.key,
    estimate: 52,
    estimateOf: (key) => (key.startsWith("h:") ? 50 : key.startsWith("m:") ? 36 : 52),
    from: 60,
  });
  const listRows = $derived(lines.flatMap((l) => (l.kind === "row" ? [l.e] : [])));

  // ---- the entry ----
  const records = new Map<string, PwEntry>();
  let record = $state<PwEntry | null>(null);
  let entryPane = $state<TfPwEntry | null>(null);

  async function fetchEntry(id: string): Promise<PwEntry | null> {
    const kept = records.get(id);
    if (kept) return kept;
    const read = await ports.entry(id);
    if (read) records.set(id, read);
    return read;
  }

  /** Ask an entry ahead, when a row takes the pointer, so a press finds it in memory. */
  function ahead(id: string): void {
    if (!records.has(id)) void fetchEntry(id).catch(() => {});
  }

  // A new read of the page drops the records it had: an entry may have changed.
  $effect(() => {
    void held;
    records.clear();
  });

  $effect(() => {
    const id = openId;
    if (!id) {
      record = null;
      return;
    }
    const kept = records.get(id);
    if (kept) {
      record = kept;
      return;
    }
    let live = true;
    void fetchEntry(id).then((read) => {
      if (live) record = read;
    });
    return () => {
      live = false;
    };
  });

  // The first entry listed opens when the address names none.
  $effect(() => {
    if (where.entry !== null || !m) return;
    const listed = listRows;
    if (picked !== null && listed.some((e) => e.id === picked)) return;
    picked = listed[0]?.id ?? null;
  });

  function pick(e: Entry): void {
    picked = e.id;
    if (where.entry !== null) location.hash = node ? hashOf(node) : "#/passwords";
  }

  function move(step: number): void {
    const at = listRows.findIndex((e) => e.id === openId);
    const next = listRows[Math.max(0, Math.min(listRows.length - 1, at + step))];
    if (!next) return;
    pick(next);
    const index = lines.findIndex((l) => l.kind === "row" && l.e.id === next.id);
    if (index >= 0) listWin.reveal(index);
  }

  const source = $derived.by((): PwSource | null => {
    if (!held || !openEntry) return null;
    return held.sources[openEntry.src] ?? null;
  });
  const word = $derived(held && openEntry ? sourceWord(held, openEntry.src) : "the password manager");
  let retrying = $state(false);

  async function retry(): Promise<void> {
    retrying = true;
    try {
      await read();
    } finally {
      retrying = false;
    }
  }

  // ---- keys ----
  function onkey(event: KeyboardEvent): void {
    if (!visible || event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey) return;
    if (event.key === "Escape") {
      if (query !== "") {
        query = "";
        event.preventDefault();
        return;
      }
      if (inTextField(event.target)) return;
      if (node && node.parent) {
        event.preventDefault();
        go(node.parent);
      }
      return;
    }
    if (inTextField(event.target)) return;
    const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
    const act: Record<string, () => void> = {
      "/": () => filter?.focus(),
      ArrowDown: () => move(1),
      ArrowUp: () => move(-1),
      j: () => move(1),
      k: () => move(-1),
      c: () => entryPane?.copyPassword(),
      s: () => entryPane?.togglePassword(),
      u: () => void entryPane?.copyUsername(),
      o: () => entryPane?.copyCode(),
      a: () => void entryPane?.address(),
      e: () => {
        if (record && source && record.can_modify && source.offers.writes.includes("edit_entry") && source.down_at === null) writing = "edit";
      },
      n: () => {
        if (newSource) writing = "new";
      },
    };
    // ↑ ↓ in the tree are the tree's.
    if ((key === "ArrowDown" || key === "ArrowUp") && (event.target as HTMLElement | null)?.closest?.(".tf-pw-tree")) return;
    const run = act[key];
    if (!run) return;
    event.preventDefault();
    run();
  }

  // ---- words ----
  const multi = $derived((held?.sources.length ?? 0) > 1);
  const headLine = $derived.by(() => {
    if (!held || !m || !node) return "";
    const own = node.top ? [held.sources[node.src]!] : node.id === "" ? held.sources : [held.sources[node.src]].filter((s): s is PwSource => !!s);
    const lines = own.map((s) => {
      const name = multi && node.id === "" ? `${s.name} ` : "";
      if (s.never_synced) return `${name || "It "}is not synced yet.`;
      const when = s.synced_at ? (s.by_hand ? `by hand at ${clockLabel(s.synced_at, new Date(), true)}` : stampLabel(s.synced_at, new Date(), true)) : "";
      if (s.down_at) return `${name || s.name + " "}did not answer at ${clockLabel(s.down_at, new Date(), true)}; its entries are as synced ${when}.`;
      return `${name ? `${name}synced` : "Synced"} ${when}.`;
    });
    const folders = countFolders(node);
    const count = query
      ? `${plural(shownPanels[0]?.rows.length ?? 0, "entry matches", "entries match")} "${query}".`
      : `${plural(node.total, "entry", "entries")}${folders ? ` in ${plural(folders, "folder")}` : ""}.`;
    const rule = node.id !== "" && commentFolder(node) ? " A usage comment is required for every read here." : "";
    return `${count} ${lines.join(" ")}${rule}`;
  });

  function countFolders(at: Node): number {
    let n = 0;
    const walk = (x: Node) => {
      for (const kid of x.kids) {
        if (!kid.top) n++;
        walk(kid);
      }
    };
    walk(at);
    return n;
  }

  /** A tree row's depth, as `--d`, through the CSSOM: CSP refuses a style attribute. */
  function depth(n: number) {
    return (el: HTMLElement) => {
      el.style.setProperty("--d", String(n));
    };
  }

  // ---- writes (ADR-0056) ----
  let writing = $state<"change" | "new" | "edit" | null>(null);
  /** The source a new entry goes to: the folder's, else the first that performs creates. */
  const newSource = $derived.by((): PwSource | null => {
    if (!held) return null;
    const at = node && node.src >= 0 ? held.sources[node.src] : null;
    const performs = (s: PwSource) => s.offers.writes.includes("create_entry") && s.down_at === null && !s.never_synced;
    if (at) return performs(at) ? at : null;
    return held.sources.find(performs) ?? null;
  });

  async function written(entityId: string, words: string): Promise<void> {
    records.clear();
    push({ text: `${words}.` });
    await read();
    if (where.entry === null) picked = entityId;
    else location.hash = `#/passwords/entry/${encodeURIComponent(entityId)}`;
    records.delete(entityId);
    const fresh = await fetchEntry(entityId).catch(() => null);
    if (fresh && openId === entityId) record = fresh;
  }

  function copied(what: string): void {

    push({ text: `Copied the ${what}.` });
  }
</script>

<svelte:window onkeydown={onkey} />

{#if writing !== null && m}
  {@const writeSource = writing === "new" ? newSource : source}
  {#if writeSource}
    <TfPwWrite
      kind={writing}
      source={writeSource}
      entry={writing === "new" ? null : record}
      folder={node}
      {m}
      {timedTicket}
      {layers}
      send={ports.write}
      copySecret={ports.copySecret}
      onclose={() => (writing = null)}
      ondone={(id, words) => void written(id, words)}
    />
  {/if}
{/if}

{#snippet mark()}
  <svg class="tf-pw-mark" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" role="img" aria-label="Usage comment required"
    ><title>A usage comment is required for every read here</title><path d="M2.5 3.5h11v7h-6l-3 2.5v-2.5h-2z" /></svg
  >
{/snippet}

{#snippet row(e: Entry, path: boolean, last: boolean)}
  {@const x = held?.sources[e.src]?.offers.expiry ? expiry(e.expires) : null}
  <button
    type="button"
    class="tf-pw-row"
    class:tf-pw-last={last}
    aria-current={e.id === openId ? "true" : undefined}
    onclick={() => pick(e)}
    onpointerenter={() => ahead(e.id)}
  >
    <span class="tf-pw-t">{#each marks(e.title, query) as part, i (i)}{#if part.hit}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}</span>
    <span class="tf-pw-s"
      >{#if e.user}<span class="tf-mono">{#each marks(e.user, query) as part, i (i)}{#if part.hit}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}</span>{/if}{#if path && m}<span
          >{chainOf(m, e.folder !== null ? (m.nodes.get(e.folder) ?? null) : null).map((n) => n.name).join(" / ")}</span
        >{:else if e.address}<span class="tf-mono">{#each marks(bare(e.address), query) as part, i (i)}{#if part.hit}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}</span>{/if}{#if !e.user && !e.address && !path}&nbsp;{/if}</span
    >
    <span class="tf-pw-e"
      >{#if x && x.tone}<span class="tf-chip tf-{x.tone}">{x.text}</span>{/if}{#if e.flags & NEEDS_APPROVAL}<span class="tf-lab">approval</span>{/if}{#if e.flags & HAS_CODE}<span class="tf-lab">code</span>{/if}{#if e.flags & CANNOT_READ}<span class="tf-lab">no read</span>{/if}{#if e.changed}<span class="tf-num">{e.changed.slice(0, 10)}</span>{/if}</span
    >
  </button>
{/snippet}

<div class="tf-pw-page">
  {#if !held || !m || !node}
    <div class="tf-pw-none">
      {#if failed}<p class="tf-stg-errt">{failed}</p>{/if}
    </div>
  {:else if held.sources.length === 0}
    <div class="tf-pw-none">
      <div class="tf-empty">
        <h1>No password source yet</h1>
        <p>
          Add your password manager in Settings › Sources, the way Jira or Gitea is added. Its entries are listed here and found by the filter. A password stays where it is:
          knobas reads one only when you show or copy it.
        </p>
        <a class="tf-btn tf-primary" href="#/settings/sources">Add a password source</a>
      </div>
    </div>
  {:else if held.sources.length === 1 && held.sources[0]!.never_synced}
    {@const only = held.sources[0]!}
    <div class="tf-pw-none">
      <div class="tf-empty">
        <h1>{only.name} is not synced yet</h1>
        <p>{only.by_hand ? "It syncs only when you press Sync now. " : ""}A sync lists its entries and their folders. It reads no passwords.</p>
        {@render sync?.()}
      </div>
    </div>
  {:else}
    <div class="tf-pw-body">
      <aside class="tf-rd-tree tf-pw-tree">
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <div class="tf-dl-body" role="tree" aria-label="Folders" tabindex="-1" onkeydown={treeKey} {@attach treeWin.list}>
          {#if treeWin.before > 0}<div class="tf-wgap" aria-hidden="true" {@attach treeWin.room("before")}></div>{/if}
          {#each treeWin.slice(rows) as r (r.node.id || "~")}
            {@const here = r.node === node && query === ""}
            {@const src = r.node.src >= 0 ? held.sources[r.node.src] : null}
            <button
              type="button"
              role="treeitem"
              class="tf-pw-node"
              class:tf-pw-top={r.node.top}
              class:tf-holds={!r.open && r.node.kids.length > 0 && holds.has(r.node.id) && !here}
              {@attach depth(Math.max(0, r.node.depth))}
              aria-level={Math.max(0, r.node.depth) + 1}
              aria-expanded={r.node.kids.length > 0 && r.node.id !== "" ? r.open : undefined}
              aria-selected={here}
              tabindex={here ? 0 : -1}
              data-id={r.node.id || "~"}
              onclick={(event) => pressNode(event, r.node)}
              {@attach treeWin.row(r.node.id || "~")}
              ><span class="tf-pw-tw" data-twist={r.node.kids.length > 0 && r.node.id !== "" ? "" : undefined}
                >{r.node.kids.length > 0 && r.node.id !== "" ? (r.open ? "▾" : "▸") : ""}</span
              ><span class="tf-pw-nm">{r.node.name}</span>{#if r.node.comment}{@render mark()}{/if}{#if r.node.top && src?.down_at}<span
                  class="tf-dot tf-err"
                  role="img"
                  aria-label="Not reachable"
                  title="Did not answer at {clockLabel(src.down_at, new Date(), true)}"
                ></span>{/if}{#if r.node.top && src?.never_synced}<span class="tf-pw-n">not synced</span>{:else}<span class="tf-pw-n">{r.node.total}</span>{/if}</button
            >
          {/each}
          {#if treeWin.after > 0}<div class="tf-wgap" aria-hidden="true" {@attach treeWin.room("after")}></div>{/if}
        </div>
      </aside>
      <section class="tf-pw-work">
        <div class="tf-pw-bench">
          <header class="tf-fhead tf-pw-head">
            <div>
              <h1>{node.id === "" ? "Passwords" : node.name}</h1>
              <p class="tf-sub">{headLine}</p>
            </div>
            <div class="tf-acts">
              {#if newSource}<button type="button" class="tf-btn" aria-label="New entry" aria-keyshortcuts="N" onclick={() => (writing = "new")}>New entry<kbd>N</kbd></button>{/if}
              {@render sync?.()}
            </div>
          </header>
          <div class="tf-pw-cols">
            <div class="tf-pw-listcol">
              <label class="tf-fsearch tf-pw-filter"
                ><svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"
                  ><circle cx="7" cy="7" r="5" /><path d="M11 11l3.5 3.5" /></svg
                ><input
                  type="text"
                  placeholder="Filter every folder"
                  aria-label="Filter the entries of every folder"
                  autocomplete="off"
                  spellcheck="false"
                  bind:value={query}
                  bind:this={filter}
                  onkeydown={(event) => {
                    if (event.key === "Escape" && query !== "") {
                      event.preventDefault();
                      event.stopPropagation();
                      query = "";
                    } else if (event.key === "ArrowDown") {
                      event.preventDefault();
                      move(1);
                    } else if (event.key === "ArrowUp") {
                      event.preventDefault();
                      move(-1);
                    }
                  }}
                /><kbd>/</kbd></label
              >
              <div class="tf-pw-rows" {@attach listWin.list}>
                {#each listWin.slice(lines) as line (line.key)}
                  {#if line.kind === "head"}
                    <header class="tf-pw-lhead" class:tf-pw-first={line.first} {@attach listWin.row(line.key)}>
                      <h3>
                        {#if line.href}<a href={line.href}>{#if line.pre}<span class="tf-pw-pre">{line.pre}</span>{/if}{line.title}</a>{:else}{line.title}{/if}
                      </h3>
                      {#if line.comment}{@render mark()}{/if}
                      <span class="tf-count">{line.count}</span>
                    </header>
                  {:else if line.kind === "row"}
                    <div class="tf-pw-lrow" class:tf-pw-last={line.last} {@attach listWin.row(line.key)}>{@render row(line.e, line.path, line.last)}</div>
                  {:else if line.kind === "more"}
                    <div class="tf-pw-lrow tf-pw-last" {@attach listWin.row(line.key)}>
                      <button type="button" class="tf-pw-more" onclick={() => go(line.node)}>Show all {line.count} in {line.node.name}</button>
                    </div>
                  {:else}
                    <div class="tf-panel" {@attach listWin.row(line.key)}><div class="tf-empty tf-pw-blank"><p>{line.text}</p></div></div>
                  {/if}
                {/each}
              </div>
              <div class="tf-pw-keys" aria-label="Keys">
                <span><kbd>↑</kbd> <kbd>↓</kbd> move</span><span><kbd>/</kbd> filter</span><span><kbd>C</kbd> copy the password</span><span><kbd>U</kbd> username</span><span
                  ><kbd>O</kbd> one-time code</span
                ><span><kbd>Esc</kbd> up a folder</span>
              </div>
            </div>
            <div class="tf-pw-entrycol">
              {#if record && source && openEntry}
                {#key record.id}
                  <TfPwEntry bind:this={entryPane} entry={record} {source} {m} {word} {timedTicket} {visible} {ports} onretry={() => void retry()} {retrying} {copied} onchange={() => (writing = "change")} onedit={() => (writing = "edit")} {layers} />
                {/key}
              {:else}
                <div class="tf-panel"><div class="tf-empty tf-pw-blank"><p>{query ? "No entry matches the filter." : "No entry is open."}</p></div></div>
              {/if}
            </div>
          </div>
        </div>
      </section>
    </div>
  {/if}
</div>
