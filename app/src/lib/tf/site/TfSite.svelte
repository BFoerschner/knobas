<!--
  A source's own website inside knobas (ADR-0050, decisions §7, §14, §16): the bar, and under it the rectangle the
  native view covers.

  The view is Rust's, by source id, and outlives this page: the page only says "show yours here" and "hide yours". It
  is on screen while this page is the shown page, no knobas layer is open (a native view lies above every pixel the
  page draws, so a dialog, a menu or the launcher over it would be cut off), and the site has a page to show. The
  rectangle under it always draws what the site's state says, so hiding the view never uncovers a blank.

  The insets go to Rust when they change (zoom, the font size, the top bar wrapping), never per resize: Rust sets the
  bounds from the window's size on its own.
-->
<script lang="ts" module>
  import type { SiteInsets } from "../../ipc/site";

  /** The bridge calls the page makes; a test's stand-ins otherwise. */
  export interface SitePagePorts {
    show: (source: string, insets: SiteInsets, focus: boolean, esc: boolean) => void;
    hide: (source: string) => void;
    open: (source: string, insets: SiteInsets) => void;
    browse: (url: string) => void;
    answer: (source: string, origin: string, allow: boolean) => Promise<string | null>;
    measure: (el: Element) => SiteInsets;
  }
</script>

<script lang="ts">
  import { untrack } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import { siteAnswer, siteHide, siteOpen, siteShow } from "../../ipc/site";
  import { openInBrowser } from "../../shell/external-links"; // browser on purpose: the site's *Open in browser*
  import { push } from "../../shell/toasts.svelte";
  import type { EscTarget, EscTargetOf } from "../escape";
  import type { Layers } from "../layers.svelte";
  import { namePage } from "../page-name.svelte";
  import { pageRefresh } from "../page-refresh.svelte";
  import { reloadSite } from "./actions";
  import { measure, sameInsets } from "./insets";
  import { siteStore, type SiteLanding, type SiteStore } from "./site.svelte";
  import TfSiteBar from "./TfSiteBar.svelte";
  import TfSiteVeil from "./TfSiteVeil.svelte";
  import { siteVisits, type Visits } from "./visits";
  import { entityHead, type UrlMatch } from "../../ipc/entity";
  import { entityAddress } from "../address";
  import { landingName, kindName } from "./landing";

  /** An item's title from its row, through the answer cache. */
  const headTitleOf = (entityId: string): Promise<string | null> => entityHead(entityId).then((detail) => detail.row.title || null);

  const BRIDGE: SitePagePorts = {
    show: (source, insets, focus, esc) => void siteShow(source, insets, focus, esc).catch(() => {}),
    hide: (source) => void siteHide(source).catch(() => {}),
    open: (source, insets) =>
      void siteOpen(source, null, insets, false).catch((cause: unknown) => push({ text: `Could not open the site: ${ipcErrorMessage(cause)}`, tone: "err" })),
    browse: (url) => openInBrowser(url), // browser on purpose: the bar's *Open in browser*
    answer: (source, origin, allow) => siteAnswer(source, origin, allow),
    measure: (el) => measure(el),
  };

  let {
    source,
    visible,
    back,
    layers,
    ports = BRIDGE,
    store = siteStore,
    visits = siteVisits,
    head = headTitleOf,
  }: {
    source: string;
    visible: boolean;
    back: EscTarget | null;
    layers: Pick<Layers, "open">;
    ports?: SitePagePorts;
    store?: SiteStore;
    /** What the site's addresses name in the mirror, and the refresh after a visit (decisions §15). */
    visits?: Pick<Visits, "answer" | "see" | "leave">;
    /** An item's title from its row (`entity_head`), for the landing's name where nothing drawn names it. */
    head?: (entityId: string) => Promise<string | null>;
  } = $props();

  const status = $derived(store.status.get(source) ?? null);
  const setting = $derived(store.settings.get(source));
  const name = $derived(store.name(source));
  /** A knobas layer stands where the view would be: the view steps aside. */
  const covered = $derived(layers.open.length > 0);
  const wanted = $derived(visible && !covered && status !== null);

  // *Back to ⟨name⟩* on the next page names what was left here.
  namePage(() => status?.title || name);

  let room = $state<HTMLElement>();
  /** The insets last sent, and whether the view was last asked to be on screen. */
  let sent: SiteInsets | null = null;
  let shown: boolean | null = null;
  /** Whether Esc had somewhere to go when the view was last shown. */
  let escSent: boolean | null = null;
  /** The site held the keyboard when a layer came over it: it takes it back when the layer goes. */
  let keysBack = false;

  $effect(() => {
    const on = wanted;
    const el = room;
    if (el === undefined) return;
    untrack(() => {
      if (on) {
        const insets = ports.measure(el);
        // Shown by an address change it takes the keyboard; uncovered, it takes it back if it had it.
        const focus = shown === null || shown === false ? keysBack || !covered : false;
        keysBack = false;
        if (shown !== true || !sameInsets(sent, insets)) ports.show(source, insets, focus, back !== null);
        sent = insets;
        escSent = back !== null;
        shown = true;
      } else if (shown === true || (shown === null && status !== null)) {
        keysBack = covered && status?.keyboard === "site";
        ports.hide(source);
        shown = false;
      }
    });
  });

  // The rectangle moved within the window (zoom, a font size, the top bar wrapping): the new insets, once.
  $effect(() => {
    const el = room;
    if (el === undefined || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      if (shown !== true) return;
      const insets = ports.measure(el);
      if (sameInsets(sent, insets)) return;
      sent = insets;
      ports.show(source, insets, false, back !== null);
    });
    observer.observe(el);
    return () => observer.disconnect();
  });

  // Esc's target can come and go while shown: Rust is told, so the second Esc knows where to go.
  $effect(() => {
    const esc = back !== null;
    untrack(() => {
      if (shown !== true || sent === null || escSent === esc) return;
      escSent = esc;
      ports.show(source, sent, false, esc);
    });
  });

  /** The mirrored item the site's address names, for *Open in knobas*; `null` while unknown or for nothing. */
  let match = $state<UrlMatch | null>(null);
  /** Whether the mirror has not yet said what the address names. */
  let matching = $state(false);
  $effect(() => {
    const url = status?.url ?? null;
    if (!visible || url === null) {
      match = null;
      matching = false;
      return;
    }
    let dead = false;
    const known = untrack(() => visits.answer(url));
    match = known ?? null;
    matching = known === undefined;
    const read = untrack(() => visits.see(source, url));
    if (known === undefined) untrack(() => store.settling(source, read));
    void read.then((found) => {
      if (dead) return;
      match = found;
      matching = false;
      store.settling(source, null);
    });
    return () => {
      dead = true;
      untrack(() => store.settling(source, null));
    };
  });

  /** The item the site was opened at (`openSiteAt`), `null` for none or one the mirror does not hold. */
  let openedItem = $state<UrlMatch | null>(null);
  let openedKnown = $state(true);
  $effect(() => {
    const url = store.opened.get(source) ?? null;
    if (url === null) {
      openedItem = null;
      openedKnown = true;
      return;
    }
    let dead = false;
    const known = untrack(() => visits.answer(url));
    openedItem = known ?? null;
    openedKnown = known !== undefined;
    void untrack(() => visits.see(source, url)).then((found) => {
      if (dead) return;
      openedItem = found;
      openedKnown = true;
    });
    return () => {
      dead = true;
    };
  });

  /**
   * Where `Esc` twice lands (Björn and Fable's ruling, 2026-11-25; #2559): the page knobas holds for where the site
   * stands, else for the item it was opened at. He browsed in the site; leaving it lands on the page he is on, never on
   * what opened the site. The shell's `escTarget` steps back instead where it is the opener's own page.
   */
  const landing = $derived.by((): SiteLanding | null => {
    if (!visible || matching || !openedKnown) return null;
    const at = match ?? openedItem;
    if (at === null) return null;
    return { hash: entityAddress(at.kind, at.entity_id), name: landingName(at, headTitle), entityId: at.entity_id, kind: at.kind };
  });
  /** The landing's title as its row says, where nothing knobas drew names it yet. */
  let headTitle = $state<{ id: string; title: string } | null>(null);
  $effect(() => {
    const m = match;
    if (!visible || m === null || landingName(m, null) !== kindName(m.kind)) return;
    let dead = false;
    void untrack(() => head(m.entity_id)).then(
      (title) => {
        if (!dead && title !== null) headTitle = { id: m.entity_id, title };
      },
      () => {},
    );
    return () => {
      dead = true;
    };
  });
  /** The page the site stood on last as it was left, which its leaving refreshes first. */
  let landedLast: string | null = null;
  $effect(() => {
    const next = landing;
    // Left: the page it lands on is kept for the refresh below, which runs after this.
    if (next !== null) landedLast = next.entityId;
    else if (untrack(() => visible)) landedLast = null;
    untrack(() => store.land(source, next));
  });

  // Leaving the site's page, or its close: the items it stood on are read again, the page it lands on first.
  $effect(() => {
    if (visible && status !== null) return;
    const first = landedLast;
    landedLast = null;
    untrack(() => void visits.leave(source, first));
  });

  // ⌘R with the keyboard in knobas reloads the site, through the page's question (decisions §8).
  $effect(() => {
    if (!visible) return;
    return pageRefresh.add({ kind: "detail", press: () => void reloadSite(source) });
  });

  function open() {
    if (room === undefined) return;
    ports.open(source, ports.measure(room));
  }

  function browse(url: string | null) {
    if (url !== null && url !== "") ports.browse(url);
  }

  async function answer(allow: boolean) {
    const origin = status?.asks_origin;
    if (origin == null) return;
    try {
      const url = await ports.answer(source, origin, allow);
      if (!allow) browse(url);
      else store.holdOne({ ...(setting ?? fallbackSetting()), allowed: [...(setting?.allowed ?? []), origin] });
    } catch (cause) {
      push({ text: `Could not answer the site: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  function fallbackSetting() {
    return { source, open_inside: true, base: "", own: "", allowed: [], suggested: [], kept_since: null, offered: true, persists: true, sign_in_user: null, own_sign_in: null, confined: false };
  }

  const backOf = $derived(back as EscTargetOf | null);
</script>

<div class="tf-site" data-source={source}>
  <TfSiteBar {source} {name} {status} {setting} back={backOf} {store} {match} />
  <div class="tf-site-room" bind:this={room}>
    <TfSiteVeil
      {name}
      {status}
      {setting}
      {covered}
      onopen={open}
      onbrowse={() => browse(status?.url ?? setting?.base ?? null)}
      oncontinue={() => void answer(true)}
      ondecline={() => void answer(false)}
      onreload={() => void reloadSite(source)}
    />
  </div>
</div>
