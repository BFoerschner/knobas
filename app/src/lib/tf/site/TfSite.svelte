<!--
  A source's own website inside knobas (ADR-0048, decisions §7, §14, §16): the bar, and under it the rectangle the
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
  import { siteStore, type SiteStore } from "./site.svelte";
  import TfSiteBar from "./TfSiteBar.svelte";
  import TfSiteVeil from "./TfSiteVeil.svelte";
  import { siteVisits, type Visits } from "./visits";
  import type { UrlMatch } from "../../ipc/entity";

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
  }: {
    source: string;
    visible: boolean;
    back: EscTarget | null;
    layers: Pick<Layers, "open">;
    ports?: SitePagePorts;
    store?: SiteStore;
    /** What the site's addresses name in the mirror, and the refresh after a visit (decisions §15). */
    visits?: Pick<Visits, "answer" | "see" | "leave">;
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
  $effect(() => {
    const url = status?.url ?? null;
    if (!visible || url === null) {
      match = null;
      return;
    }
    let dead = false;
    match = untrack(() => visits.answer(url)) ?? null;
    void untrack(() => visits.see(source, url)).then((found) => {
      if (!dead) match = found;
    });
    return () => {
      dead = true;
    };
  });
  // Leaving the site's page, or its close: the items it stood on are read again.
  $effect(() => {
    if (visible && status !== null) return;
    untrack(() => void visits.leave(source));
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
    return { source, open_inside: true, base: "", own: "", allowed: [], suggested: [], kept_since: null, offered: true, persists: true };
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
