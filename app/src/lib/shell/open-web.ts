/**
 * Where a web address opens: in its source's site inside knobas, or in the browser (ADR-0050, decisions §12).
 *
 * Every *Open in ⟨source⟩* and every link in synced text ends here. With *Open its pages inside knobas* on for a
 * source, every address of that source opens in its site, from wherever it is pressed; with it off, nothing changes
 * from before: the browser, through `openExternal` (http and https only). A middle press, or ⌘ or Ctrl held, always
 * asks for the browser.
 *
 * The decision reads the site store the shell filled once the database answered (`tf/site/site.svelte.ts`), so a
 * press is answered in its own task with no read. A test in `house-rules.test.ts` keeps every other module from
 * calling `openExternal` or `openInBrowser` itself.
 */
import { ipcErrorMessage } from "../ipc";
import { siteOpen, siteSuggest, type SiteSetting } from "../ipc/site";
import { siteHash, siteStore, type SiteStore } from "../tf/site/site.svelte";
import { siteInsetsGuess } from "../tf/site/insets";
import { acknowledged, openingWords } from "./ack.svelte";
import { openExternal } from "./open-external";
import { push } from "./toasts.svelte";

/** How a press asked for an address. */
export interface WebHow {
  /** The source of the entity the press was on: an *Open in ⟨source⟩* knows it. */
  source?: string | null;
  /** A press that asks for the other place: a middle press, or ⌘ or Ctrl held. Always the browser. */
  elsewhere?: boolean;
  /** What the press names, for the chip: *Opening PAY-231 in Jira*. */
  item?: string | null;
}

/** Where an address opens. */
export type WebPlace = { to: "site"; source: string } | { to: "browser" };

interface Base {
  origin: string;
  path: string;
}

function baseOf(url: string): Base | null {
  try {
    const parsed = new URL(url);
    if (parsed.protocol !== "http:" && parsed.protocol !== "https:") return null;
    return { origin: parsed.origin, path: parsed.pathname.replace(/\/+$/, "") };
  } catch {
    return null;
  }
}

/** Whether `url` (origin and path) lies under `base`, at a segment boundary. */
function holds(base: Base, origin: string, path: string): boolean {
  if (base.origin !== origin) return false;
  if (base.path === "") return true;
  return path === base.path || path.startsWith(`${base.path}/`);
}

/** The sources whose sites are on, as their settings. */
function onSites(store: Pick<SiteStore, "settings">): SiteSetting[] {
  return [...store.settings.values()].filter((one) => one.open_inside && one.offered);
}

/**
 * The source whose base is the longest prefix of `url` at a segment boundary, among the sources whose sites are on,
 * or `null` (decisions §6, `owner`).
 */
export function owner(url: string, store: Pick<SiteStore, "settings"> = siteStore): string | null {
  const at = baseOf(url);
  if (at === null) return null;
  let best: { source: string; length: number } | null = null;
  for (const one of onSites(store)) {
    const base = baseOf(one.base);
    if (base !== null && holds(base, at.origin, at.path) && (best === null || base.path.length > best.length)) {
      best = { source: one.source, length: base.path.length };
    }
  }
  return best?.source ?? null;
}

/** Whether `setting`'s site may show `url`: its own origin or one the person allowed. */
function allows(setting: SiteSetting, url: string): boolean {
  const at = baseOf(url);
  return at !== null && (at.origin === setting.own || setting.allowed.includes(at.origin));
}

/** Where `url` opens, decided from what the window holds, with no read. */
export function webPlace(url: string, how: WebHow = {}, store: Pick<SiteStore, "settings"> = siteStore): WebPlace {
  if (how.elsewhere === true || baseOf(url) === null) return { to: "browser" };
  const named = how.source == null ? undefined : store.settings.get(how.source);
  if (named !== undefined && named.open_inside && named.offered && allows(named, url)) {
    // The source the press was on, unless another source's base holds the address more closely. Two sources on the
    // same base (a vault and its test account on one server) tie, and the press's own wins: its site signs in with
    // its own credential (ux30).
    const closer = owner(url, store);
    if (closer === null || closer === named.source) return { to: "site", source: named.source };
    const closerBase = baseOf(store.settings.get(closer)?.base ?? "");
    const namedBase = baseOf(named.base);
    const at = baseOf(url);
    const tie =
      closerBase !== null && namedBase !== null && at !== null && holds(namedBase, at.origin, at.path) && namedBase.path.length >= closerBase.path.length;
    return { to: "site", source: tie ? named.source : closer };
  }
  const found = owner(url, store);
  return found === null ? { to: "browser" } : { to: "site", source: found };
}

/** What a page that moves the address and opens a site needs: replaceable in a test. */
export interface WebPorts {
  browse: (url: string) => void;
  openSite: (source: string, url: string, item: string | null) => Promise<void>;
  suggest: (source: string, origin: string) => void;
}

/** Go to `source`'s site page and point its view at `url` (decisions §14, *Opening*). */
export async function openSiteAt(source: string, url: string, item: string | null = null, store: SiteStore = siteStore): Promise<void> {
  const hash = siteHash(source);
  if (location.hash !== hash) location.hash = hash;
  const asked = await siteOpen(source, url, siteInsetsGuess(), false);
  if (!asked.done && asked.asks === "unsaved") store.ask(source, { kind: "address", url, name: item });
}

/** Hand `url` to the browser: the chip where the press was, and a toast when it refuses. */
function browse(url: string): void {
  acknowledged(openingWords("the browser"), () => openExternal(url), false).catch((cause: unknown) => {
    push({ text: `Could not open the browser: ${ipcErrorMessage(cause)}`, tone: "err" });
  });
}

const BRIDGE: WebPorts = {
  browse,
  openSite: (source, url, item) => openSiteAt(source, url, item),
  suggest: (source, origin) => void siteSuggest(source, origin).catch(() => {}),
};

/**
 * Open `url` where it belongs: its source's site, or the browser. The chip where the press was says which; a refusal
 * says why in a toast. An address that is not http or https goes to `openExternal`, which refuses it with its message:
 * it never reaches a site.
 */
export function openWeb(url: string, how: WebHow = {}, ports: WebPorts = BRIDGE, store: SiteStore = siteStore): void {
  const place = webPlace(url, how, store);
  if (place.to === "browser") {
    // An *Open in ⟨source⟩* whose source opens inside knobas named an address its site may not show: Settings offers it.
    const named = how.source == null ? undefined : store.settings.get(how.source);
    const at = baseOf(url);
    if (named?.open_inside === true && how.elsewhere !== true && at !== null && !allows(named, url)) ports.suggest(named.source, at.origin);
    ports.browse(url);
    return;
  }
  // The site's view will cover the page: the chip stands in the top bar's band.
  acknowledged(openingWords(store.name(place.source), how.item ?? null), () => ports.openSite(place.source, url, how.item ?? null), true).catch(
    (cause: unknown) => {
      push({ text: `Could not open ${store.name(place.source)}: ${ipcErrorMessage(cause)}`, tone: "err" });
    },
  );
}

/** {@link openWeb} as a port's `openExternal`, for a component that awaits its opening: it resolves once started. */
export function openWebPort(how: WebHow = {}): (url: string) => Promise<void> {
  return (url) => {
    openWeb(url, how);
    return Promise.resolve();
  };
}

/**
 * Open `url` where it belongs, for a component that draws its own chip and toast around a port's `openExternal`: the
 * site's page, or `openExternal` (http and https only, which refuses anything else with its message).
 */
export function routeWeb(url: string, how: WebHow = {}, store: SiteStore = siteStore): Promise<void> {
  const place = webPlace(url, how, store);
  if (place.to === "site") return openSiteAt(place.source, url, how.item ?? null, store);
  return openExternal(url); // browser on purpose: the one place a port's opening reaches the browser
}

/** The words of the chip a press on `url` shows, for a component that draws its own: *Opening in ⟨source⟩*. */
export function webPlaceName(url: string, how: WebHow = {}, store: SiteStore = siteStore): string {
  const place = webPlace(url, how, store);
  return place.to === "site" ? store.name(place.source) : "the browser";
}

/** Whether a press asks for the other place: a middle press, or ⌘ or Ctrl held. */
export function pressElsewhere(event: Pick<MouseEvent, "button" | "metaKey" | "ctrlKey">): boolean {
  return event.button === 1 || event.metaKey || event.ctrlKey;
}
