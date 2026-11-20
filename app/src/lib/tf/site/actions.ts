/**
 * What a site's bar and the shell's keys do to a site (decisions §8 to §10), one function each, so a key means the same
 * with focus in the site or in knobas, and the bar's button is the key.
 *
 * Closing, reloading and pointing the site elsewhere ask the page first (Rust's probe); when it objects, the bar asks
 * in its row ({@link SiteQuestion}) and the keyboard comes to knobas, onto the answer that keeps the page.
 */
import { siteClose, siteDo, siteOpen, type SiteAsked } from "../../ipc/site";
import { measureShown } from "./shown";
import { siteStore, type SiteQuestion, type SiteStore } from "./site.svelte";

/** The bridge calls the actions make; a test's stand-ins otherwise. */
export interface SiteActionPorts {
  close: (source: string, force: boolean) => Promise<SiteAsked>;
  act: (source: string, act: "back" | "forward" | "reload" | "stop" | "focus" | "focus-shell", force: boolean) => Promise<SiteAsked>;
  open: (source: string, url: string, force: boolean) => Promise<SiteAsked>;
}

export const BRIDGE_ACTIONS: SiteActionPorts = {
  close: (source, force) => siteClose(source, force),
  act: (source, act, force) => siteDo(source, act, force),
  open: (source, url, force) => siteOpen(source, url, measureShown(), force),
};

function asked(store: SiteStore, source: string, answer: SiteAsked, question: SiteQuestion, ports: SiteActionPorts): void {
  if (answer.done || answer.asks !== "unsaved") return;
  store.ask(source, question);
  // The keyboard to knobas, onto the bar's first answer (the bar focuses it as it draws).
  void ports.act(source, "focus-shell", false).catch(() => {});
}

/** The bar's ×, ⌘W: close, asking the page first. */
export async function closeSite(source: string, ports: SiteActionPorts = BRIDGE_ACTIONS, store: SiteStore = siteStore): Promise<void> {
  if (store.status.get(source) === undefined) return;
  asked(store, source, await ports.close(source, false), { kind: "close" }, ports);
}

/** ⌘R, the bar's reload: reload, asking the page first. */
export async function reloadSite(source: string, ports: SiteActionPorts = BRIDGE_ACTIONS, store: SiteStore = siteStore): Promise<void> {
  const status = store.status.get(source);
  if (status === undefined) return;
  if (status.loading && status.phase === "live") {
    await ports.act(source, "stop", false);
    return;
  }
  asked(store, source, await ports.act(source, "reload", false), { kind: "reload" }, ports);
}

/** ⌘[ in a site: back in the site; `false` with no page to go back to, so knobas goes back instead. */
export function siteBack(source: string, ports: SiteActionPorts = BRIDGE_ACTIONS, store: SiteStore = siteStore): boolean {
  if (store.status.get(source)?.can_back !== true) return false;
  void ports.act(source, "back", false);
  return true;
}

/** ⌘] in a site: forward in the site; `false` with no page to go forward to. */
export function siteForward(source: string, ports: SiteActionPorts = BRIDGE_ACTIONS, store: SiteStore = siteStore): boolean {
  if (store.status.get(source)?.can_forward !== true) return false;
  void ports.act(source, "forward", false);
  return true;
}

/** The answer that throws the page away: *Close anyway*, *Reload anyway*, *Open here anyway*. */
export async function answerAnyway(source: string, ports: SiteActionPorts = BRIDGE_ACTIONS, store: SiteStore = siteStore): Promise<void> {
  const question = store.asking.get(source);
  if (question === undefined) return;
  store.ask(source, null);
  if (question.kind === "close") await ports.close(source, true);
  else if (question.kind === "reload") await ports.act(source, "reload", true);
  else await ports.open(source, question.url, true);
}

/** The answer that keeps the page: the keyboard goes back to the site. */
export function answerKeep(source: string, ports: SiteActionPorts = BRIDGE_ACTIONS, store: SiteStore = siteStore): void {
  if (store.asking.get(source) === undefined) return;
  store.ask(source, null);
  void ports.act(source, "focus", false).catch(() => {});
}
