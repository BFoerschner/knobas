<!--
  The right-click menu (2026-11-16, `docs/decisions/2026-11-16-right-click-menus.md`): one for the whole window, drawn
  by knobas in `TfMenu` at the pointer, and the web view's own menu nowhere but in a field a person types in (where its
  spelling, emoji and paste are the point) and, in a development build, on Shift+right-click (so *Inspect* stays). A
  right-click on nothing of its own (the page, a heading of no thing, a nav entry) draws nothing.

  What it is on is `thing.ts`' answer, what it offers is `items.ts`', and nothing is read before it opens: the reads it
  starts as it opens (the item's head, the repository's checkout, its page on the forge) only change a label or what a
  pick finds, never how many rows there are. The context menu key and Shift+F10 open it on the focused thing, under it.

  `↑` `↓` `Home` `End` move, `→` or Enter opens *Open in… ▸* (as does resting the pointer on it), `←` and `Esc` go back
  a level, a press anywhere else, a scroll, a move to another address and the window losing focus close it, and focus
  goes back to what had it.
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import type { CheckoutView, EntityDetail } from "../../ipc/entity";
  import { openInBrowser } from "../../shell/external-links";
  import { toastQueued } from "../../shell/queued-toast";
  import { push } from "../../shell/toasts.svelte";
  import { keyOf } from "../../time/format";
  import { entityAddress } from "../address";
  import { drawn } from "../../detail/relations";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import { apps as sharedApps, type Apps } from "../openin/apps.svelte";
  import { argvText, joinInside } from "../openin/openin";
  import TfMenu from "../TfMenu.svelte";
  import { folderOf, menuFor, type Act, type CodeAt, type Entry, type Item, type MenuFacts, type Text } from "./items";
  import { realMenuPorts, type MenuPorts } from "./ports";
  import { isEditable, selectionAt, thingAt, type Found } from "./thing";

  let {
    onsearch,
    onstarttimer,
    job = null,
    canTime = false,
    apps = sharedApps,
    ports = realMenuPorts,
    layers = sharedLayers,
    dev = import.meta.env.DEV,
    target = typeof window === "undefined" ? undefined : window,
  }: {
    /** Open the launcher with these words in its box. */
    onsearch: (query: string) => void;
    /** *Start timer* on a ticket. */
    onstarttimer?: (entityId: string, title: string) => void;
    /** The chosen job, *Link to ⟨job⟩*'s other end. */
    job?: { entityId: string; label: string } | null;
    canTime?: boolean;
    apps?: Apps;
    ports?: MenuPorts;
    layers?: Layers;
    /** A development build: Shift+right-click is the web view's, for *Inspect*. */
    dev?: boolean;
    /** The window it listens on. */
    target?: Window | undefined;
  } = $props();

  /** What is open: the thing, the words, where, and what it was found as. */
  interface Shown {
    found: Found | null;
    selection: string | null;
    at: { x: number; y: number };
    /** What had focus, which focus goes back to. */
    back: HTMLElement | null;
    /** The selection when the press went down, put back for *Copy*. */
    ranges: Range[];
    /** Opened by a key: its first row takes focus, as a pointer's opening doesn't. */
    keyed: boolean;
  }

  let shown = $state<Shown | null>(null);
  let open = $state(false);
  let facts = $state<MenuFacts>({});
  let sub = $state<{ item: Item; opener: HTMLElement } | null>(null);
  let subOpen = $state(false);
  let menuEl = $state<HTMLElement | null>(null);
  let subEl = $state<HTMLElement | null>(null);

  /** The reads started as the menu opened; a pick waits on the one it needs, which has usually answered by then. */
  let reads: { head: Promise<EntityDetail | null>; checkout: Promise<CheckoutView | null>; forge: Promise<string | null> } | null = null;

  /** The sources, read once: their names and base URLs. */
  let sources = $state.raw<{ id: string; name: string; base: string }[]>([]);

  const entries = $derived(
    shown === null
      ? []
      : menuFor(
          shown.found?.thing ?? null,
          shown.selection,
          {
            job,
            canTime,
            apps: apps.settings,
            sourceName: (id) => sources.find((one) => one.id === id)?.name ?? null,
            mirrored: (url) => sources.some((one) => one.base !== "" && url.startsWith(one.base)),
            own: shown.found?.own ?? false,
          },
          facts,
        ),
  );

  onMount(() => {
    void apps.ensure();
    ports.sources().then(
      (read) => (sources = read.map((one) => ({ id: one.id, name: one.display_name, base: one.base_url.replace(/\/+$/, "") + "/" }))),
      () => {},
    );
  });

  /** The selection as it stood when the right press went down: WebKit selects the word under the pointer first. */
  let before: { ranges: Range[]; at: number } = { ranges: [], at: 0 };
  /** When a key last opened the menu: the platform's `contextmenu` for the same key press only stops its menu. */
  let keyed = 0;

  function rangesOf(selection: Selection | null): Range[] {
    if (selection === null) return [];
    return Array.from({ length: selection.rangeCount }, (_, index) => selection.getRangeAt(index).cloneRange());
  }

  function restore(ranges: readonly Range[]) {
    const selection = getSelection();
    if (selection === null) return;
    selection.removeAllRanges();
    for (const range of ranges) selection.addRange(range);
  }

  /** The menu's own float, and its submenu's. */
  const floatOf = (inner: HTMLElement | null) => inner?.closest(".tf-menu") ?? null;

  function ownMenu(node: Node | null): boolean {
    return node !== null && ((floatOf(menuEl)?.contains(node) ?? false) || (floatOf(subEl)?.contains(node) ?? false));
  }

  /** Open on `node` at `at`, or draw nothing when it is on nothing of its own. Answers whether anything opened. */
  function show(node: Element, at: { x: number; y: number }, selection: string | null, ranges: Range[], keyed = false): boolean {
    const found = thingAt(node);
    if (found === null && selection === null) return false;
    const active = document.activeElement;
    shown = { found, selection, at, back: active instanceof HTMLElement && active !== document.body ? active : null, ranges, keyed };
    facts = {};
    sub = null;
    subOpen = false;
    open = true;
    read(found);
    return true;
  }

  /** Start the reads the menu's items may want. None is waited for here. */
  function read(found: Found | null) {
    const thing = found?.thing;
    const head = (id: string) => ports.head(id).then((value) => value, () => null);
    const checkout = (id: string) => ports.checkout(id).then((value) => value, () => null);
    const none = Promise.resolve(null);
    if (!thing) {
      reads = null;
      return;
    }
    let entity: string | null = null;
    let code: string | null = null;
    switch (thing.kind) {
      case "file":
      case "folder":
      case "line":
        code = thing.repoId;
        break;
      case "repo":
      case "branch":
        entity = thing.id;
        code = thing.id;
        break;
      case "commit":
        entity = thing.id;
        code = thing.repoId;
        break;
      case "branch-name":
      case "test":
      case "web":
      case "inbox":
        break;
      default:
        entity = thing.id;
    }
    const headRead = entity !== null ? head(entity) : code !== null ? head(code) : none;
    const checkoutRead = code !== null ? checkout(code) : none;
    const forgeRead =
      thing.kind === "file" || thing.kind === "folder" || thing.kind === "line"
        ? headRead.then((detail) => {
            const at = detail?.default_branch ?? null;
            if (at === null) return null;
            const line = thing.kind === "line" ? thing.line : null;
            return ports.objectPage(thing.repoId, { kind: "file", path: thing.path, line, at }).then((url) => url, () => null);
          })
        : none;
    const mine = (reads = { head: headRead, checkout: checkoutRead, forge: forgeRead });
    // Each answer is the facts of this opening only.
    void headRead.then((detail) => {
      if (reads === mine) facts = { ...facts, head: detail };
    });
    if (code !== null) {
      void checkoutRead.then((view) => {
        if (reads === mine) facts = { ...facts, checkout: view };
      });
    }
    void forgeRead.then((url) => {
      if (reads === mine) forge = url;
    });
  }

  /** The forge page, once read: kept beside the facts, which draw nothing from it. */
  let forge: string | null | undefined = undefined;

  onMount(() => {
    const win = target;
    if (win === undefined) return;
    const down = (event: PointerEvent) => {
      if (event.button !== 2) return;
      before = { ranges: rangesOf(getSelection()), at: event.timeStamp };
    };
    const onmenu = (event: MouseEvent) => {
      if (event.defaultPrevented) return;
      const node = event.target instanceof Element ? event.target : event.target instanceof Node ? event.target.parentElement : null;
      if (node === null) return;
      // The menu's own rows: a right-click there is a click on nothing.
      if (ownMenu(node)) {
        event.preventDefault();
        return;
      }
      if (Date.now() - keyed < 600) {
        event.preventDefault();
        return;
      }
      if (dev && event.shiftKey) return;
      if (isEditable(node)) return;
      event.preventDefault();
      // What was selected when the press went down: WebKit's own word selection under the pointer is put back.
      const ranges = before.ranges;
      const selection = getSelection();
      const was = ranges.length > 0 && ranges.some((range) => !range.collapsed);
      const words = was ? selectionAt(node, selectionOf(ranges)) : null;
      if (!was && selection !== null && !selection.isCollapsed) restore(ranges);
      if (words !== null) restore(ranges);
      before = { ranges: [], at: 0 };
      if (open) close(false);
      show(node, { x: event.clientX, y: event.clientY }, words, words === null ? [] : ranges);
    };
    const onkey = (event: KeyboardEvent) => {
      const asked = event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey && !event.metaKey && !event.ctrlKey && !event.altKey);
      if (!asked || event.defaultPrevented) return;
      const node = document.activeElement;
      if (!(node instanceof Element) || isEditable(node) || ownMenu(node)) return;
      event.preventDefault();
      keyed = Date.now();
      const selection = getSelection();
      const words = selectionAt(node, selection);
      const box = node === document.body ? null : node.getBoundingClientRect();
      const at = box === null ? { x: 16, y: 16 } : { x: Math.round(box.left + Math.min(16, box.width / 2)), y: Math.round(box.bottom) };
      show(node, at, words, rangesOf(words === null ? null : selection), true);
    };
    win.addEventListener("pointerdown", down, true);
    win.addEventListener("contextmenu", onmenu);
    win.addEventListener("keydown", onkey);
    return () => {
      win.removeEventListener("pointerdown", down, true);
      win.removeEventListener("contextmenu", onmenu);
      win.removeEventListener("keydown", onkey);
    };
  });

  /** A stand-in `Selection` over saved ranges, for `selectionAt`. */
  function selectionOf(ranges: readonly Range[]): Selection {
    return {
      isCollapsed: ranges.every((range) => range.collapsed),
      rangeCount: ranges.length,
      getRangeAt: (index: number) => ranges[index]!,
      toString: () => ranges.map((range) => range.toString()).join(""),
    } as unknown as Selection;
  }

  // The row the menu is on keeps its hover look while it is open (`[data-ctx-on]`, layers.css).
  $effect(() => {
    const node = open ? shown?.found?.node : null;
    if (!node) return;
    node.setAttribute("data-ctx-on", "");
    return () => node.removeAttribute("data-ctx-on");
  });

  // Closed by `TfMenu` (a press elsewhere, a scroll, Esc): the submenu goes with it, and so do the reads' answers.
  $effect(() => {
    if (open) return;
    untrack(() => {
      subOpen = false;
      sub = null;
      reads = null;
      forge = undefined;
    });
  });

  function close(refocus = true) {
    const back = shown?.back ?? null;
    subOpen = false;
    open = false;
    if (refocus && back?.isConnected) back.focus({ preventScroll: true });
  }

  function openSub(item: Item, opener: HTMLElement, focus: boolean) {
    if (item.disabled) return;
    if (sub?.item.id === item.id && subOpen) {
      if (focus) queueMicrotask(() => subEl?.querySelector<HTMLElement>('[role="menuitem"]:not([aria-disabled="true"])')?.focus());
      return;
    }
    sub = { item, opener };
    subOpen = true;
    subFocus = focus;
  }
  let subFocus = $state(true);

  function pick(item: Item, opener: HTMLElement | null) {
    if (item.disabled) return;
    if (item.sub) {
      if (opener) openSub(item, opener, true);
      return;
    }
    const act = item.act;
    const was = shown;
    close();
    if (act && was) carry(act, was);
  }

  /** What `text` is now, or `null` while its read hasn't answered. */
  function now(text: Text): string | null | undefined {
    if (typeof text === "string") return text;
    switch (text.read) {
      case "web":
        return facts.head === undefined ? undefined : (facts.head?.web_url ?? null);
      case "title":
        return facts.head === undefined ? undefined : (facts.head?.row.title ?? null);
      case "key-title": {
        if (facts.head === undefined) return undefined;
        const thing = shown?.found?.thing;
        const key = thing && "id" in thing ? keyOf(thing.id) : "";
        return facts.head === null ? null : `${key} ${facts.head.row.title}`;
      }
      case "full-path":
        if (facts.checkout === undefined) return undefined;
        return facts.checkout?.path ? (text.path === "" ? facts.checkout.path : joinInside(facts.checkout.path, text.path)) : null;
      case "forge":
        return forge;
    }
  }

  /** `text`, waiting for its read. */
  async function later(text: Text, from: NonNullable<typeof reads>): Promise<string | null> {
    if (typeof text === "string") return text;
    switch (text.read) {
      case "web":
        return (await from.head)?.web_url ?? null;
      case "title":
        return (await from.head)?.row.title ?? null;
      case "key-title": {
        const detail = await from.head;
        return detail === null ? null : `${keyOf(detail.row.entity_id)} ${detail.row.title}`;
      }
      case "full-path": {
        const view = await from.checkout;
        return view?.path ? (text.path === "" ? view.path : joinInside(view.path, text.path)) : null;
      }
      case "forge":
        return from.forge;
    }
  }

  const NO_PAGE = "This has no page on the web that knobas knows of.";

  function failed(what: string) {
    return (cause: unknown) => push({ text: `${what}: ${ipcErrorMessage(cause)}`, tone: "err" });
  }

  /** Start an application and say what started, as `apps.run` does. */
  function started(name: string, start: () => Promise<string[]>) {
    start().then(
      (argv) => push({ text: `Started ${name}: ${argvText(argv)}` }),
      (cause: unknown) => push({ text: `Could not start ${name}: ${ipcErrorMessage(cause)}`, tone: "err" }),
    );
  }

  /** The one place an item is carried out. */
  function carry(act: Act, was: Shown) {
    const from = reads ?? { head: Promise.resolve(null), checkout: Promise.resolve(null), forge: Promise.resolve(null) };
    switch (act.do) {
      case "go":
        return ports.go(act.hash);
      case "copy": {
        const value = now(act.text);
        if (value === null) return void push({ text: act.text !== null && typeof act.text === "object" && act.text.read === "full-path" ? "Not cloned on this machine." : NO_PAGE, tone: "err" });
        // Known: written inside the press. Still reading: the write starts inside it with the text to come.
        const wrote = value !== undefined ? ports.copyText(value) : ports.copyLater(later(act.text, from).then((text) => text ?? Promise.reject(new Error(NO_PAGE))));
        return void wrote.then(() => push({ text: act.said }), failed("Could not copy"));
      }
      case "copy-selection":
        restore(was.ranges);
        if (ports.copySelection()) push({ text: "Copied" });
        else push({ text: "Could not copy the selection.", tone: "err" });
        return;
      case "search":
        return onsearch(act.text);
      case "browser": {
        const value = now(act.url);
        if (value !== undefined) return value === null ? void push({ text: NO_PAGE, tone: "err" }) : openInBrowser(value, ports.openExternal);
        return void later(act.url, from).then((url) => (url === null ? push({ text: NO_PAGE, tone: "err" }) : openInBrowser(url, ports.openExternal)));
      }
      case "in-knobas":
        return void ports.resolveUrl(act.url).then(
          (match) => {
            if (match !== null) return ports.go(entityAddress(match.kind, match.entity_id));
            push({ text: "knobas holds nothing at that address yet.", action: { label: "Open in browser", run: () => openInBrowser(act.url, ports.openExternal) } });
          },
          failed("Could not read that address"),
        );
      case "timer": {
        const title = facts.head?.row.title;
        if (title !== undefined) return onstarttimer?.(act.entityId, title);
        return void from.head.then((detail) => onstarttimer?.(act.entityId, detail?.row.title ?? keyOf(act.entityId)));
      }
      case "link": {
        if (job === null) return;
        const row = drawn("related", job.entityId, act.entityId);
        const name = keyOf(act.entityId);
        return void ports.createLink(row.fromId, row.toId, row.relation).then(() => push({ text: `Linked ${name} to ${job!.label} as related` }), failed("Could not link"));
      }
      case "write":
        return void ports.submitWrite(act.payload).then(
          (write) => toastQueued(act.said, act.sourceId === null ? null : { id: act.sourceId, display_name: "" }, { write, ...(act.where ? { where: { label: act.where.label, run: () => ports.go(act.where!.hash) } } : {}) }),
          failed("Could not queue it"),
        );
      case "ack":
        if (act.own) return act.own();
        return void ports.ackAlert(act.monitorId).then(
          () => push({ text: "Acknowledged. The alert stays open until the monitor recovers.", action: { label: "Undo", run: () => void ports.unackAlert(act.monitorId).catch(failed("Could not undo")) } }),
          failed("Could not acknowledge"),
        );
      case "inbox":
        return void (act.op === "ack" ? ports.inboxAck(act.key) : ports.inboxDone(act.key)).then(
          () => push({ text: act.op === "ack" ? "Acked. The alert stays open until the monitor recovers." : `Done: ${act.title}` }),
          failed(act.op === "ack" ? "Could not acknowledge" : "Could not mark it done"),
        );
      case "app":
        return runApp(act.entityId, act.app, act.name, act.at);
      case "clone": {
        const title = keyOf(act.repoId);
        return void ports.clone({ entityId: act.repoId, title }).then(() => push({ text: `Cloning ${title}` }), failed("Could not clone"));
      }
    }
  }

  /**
   * An application on what the menu was opened on. A file opens with the application's file command; one with none
   * opens the file's folder as it opens a folder (Fable's ruling 5), the repository's top for a file at the top.
   */
  function runApp(entityId: string, appId: string, name: string, at: CodeAt) {
    const app = apps.settings?.applications.find((one) => one.id === appId) ?? null;
    switch (at.kind) {
      case "repository":
        return void apps.run(entityId, name, { application: appId });
      case "folder":
        return started(name, () => ports.openFolder(entityId, appId, at.path));
      case "commit":
        return void apps.runObject(entityId, name, appId, { kind: "commit", sha: at.sha });
      case "file": {
        if (app?.file_command) return void apps.run(entityId, name, { application: appId, file: at.path, line: at.line });
        const folder = folderOf(at.path);
        if (folder === "") return void apps.run(entityId, name, { application: appId });
        return started(name, () => ports.openFolder(entityId, appId, folder));
      }
    }
  }

  function isItem(entry: Entry): entry is Item {
    return !("rule" in entry);
  }

  /** A pointer resting on a row: *Open in… ▸* opens at once, any other row closes it. */
  function rest(item: Item, node: HTMLElement) {
    if (item.sub) openSub(item, node, false);
    else if (subOpen) subOpen = false;
  }

  function onrowkey(event: KeyboardEvent, item: Item) {
    if (item.sub && event.key === "ArrowRight") {
      event.preventDefault();
      event.stopPropagation();
      openSub(item, event.currentTarget as HTMLElement, true);
    }
  }
</script>

{#snippet rows(list: Entry[], top: boolean)}
  {#each list as entry, index (isItem(entry) ? entry.id : `rule-${index}`)}
    {#if isItem(entry)}
      <button
        type="button"
        role="menuitem"
        class="tf-ctx-i"
        data-item={entry.id}
        aria-disabled={entry.disabled ? "true" : undefined}
        aria-haspopup={entry.sub ? "menu" : undefined}
        aria-expanded={entry.sub ? subOpen && sub?.item.id === entry.id : undefined}
        title={entry.disabled ?? undefined}
        onmousedown={(event) => event.preventDefault()}
        onpointerenter={(event) => top && rest(entry, event.currentTarget)}
        onkeydown={(event) => onrowkey(event, entry)}
        onclick={(event) => pick(entry, event.currentTarget)}
        ><span class="tf-ctx-l">{entry.label}</span>{#if entry.sub}<svg class="tf-ctx-sub" viewBox="0 0 16 16" aria-hidden="true"><path d="m6 4 4 4-4 4" /></svg>{:else if entry.hint}<span class="tf-ctx-h">{entry.hint}</span>{/if}</button
      >
    {:else}
      <hr />
    {/if}
  {/each}
{/snippet}

<TfMenu
  bind:open
  opener={shown?.back ?? null}
  at={shown?.at ?? null}
  label="Actions"
  className="tf-ctx-m"
  focusItem={shown?.keyed ?? false}
  keep={() => floatOf(subEl)}
  {layers}
>
  <span class="tf-ctx-m-inner" bind:this={menuEl}>{@render rows(entries, true)}</span>
</TfMenu>
{#if sub !== null}
  <TfMenu bind:open={subOpen} opener={sub.opener} beside label={sub.item.label} className="tf-ctx-m" focusItem={subFocus} keep={() => floatOf(menuEl)} {layers}>
    <span class="tf-ctx-m-inner" bind:this={subEl}>{@render rows(sub.item.sub ?? [], false)}</span>
  </TfMenu>
{/if}
