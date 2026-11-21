/**
 * `?fake-ipc`: a source's own website inside knobas (ADR-0050, decisions §24). The site commands answered and
 * `site:status` emitted as the native side would, so `just dev`, the budgets, the press trips and `just moves` draw a
 * site's page. Where the native view would be, a grey block stands over the rectangle (a fixed element of the
 * document, as the native view is above the page), shown and hidden as the view would be.
 *
 * Tidewater Jira's and Tidewater Confluence's pages open inside knobas; the others open the browser.
 * `?fake-site=off` turns them all off. `?fake-site-dirty` makes every page hold changes, so a close, a reload and
 * another address ask. `?fake-long-paths` opens a site at a long address; `?fake-site-esc=<ms>` holds the first Esc
 * that long. A view commits its page 250 ms after it is
 * made, as a fast local site would.
 */
import type { SiteInsets, SiteSetting, SiteStatus } from "../../../ipc/site";
import { onFakeReset } from "../fake-seed";
import type { FakeBridge, Handlers } from "../fake-tauri";

let bridge: FakeBridge | null = null;
// The bridge is the installed one's, not a value a handler changes; a reset leaves no grey block behind.
onFakeReset(() => {
  bridge = null;
  if (typeof document !== "undefined") for (const el of document.querySelectorAll("[data-fake-site]")) el.remove();
});

/** Hold the installed bridge: the site's status is an event. */
export function attach(installed: FakeBridge): void {
  bridge = installed;
}

const BASES: Record<string, string> = {
  jira: "https://jira.tidewater.example",
  confluence: "https://wiki.tidewater.example",
  gitea: "https://git.tidewater.example",
  teamcity: "https://ci.tidewater.example",
  kuma: "https://status.tidewater.example",
  mock: "https://mock.tidewater.example",
  kestrel: "https://kestrel.tidewater.example",
};

const TITLES: Record<string, string> = {
  jira: "PAY-231 Payout CSV export – Tidewater Jira",
  confluence: "SEPA payout retry design – Payments – Tidewater Confluence",
};

interface FakeSite {
  status: SiteStatus;
  insets: SiteInsets | null;
  wanted: boolean;
  history: string[];
  at: number;
}

function blank(source: string, escMs = 500): SiteStatus {
  return {
    source,
    phase: "opening",
    url: null,
    title: "",
    own: false,
    loading: true,
    can_back: false,
    can_forward: false,
    shown: false,
    keyboard: "shell",
    asks_origin: null,
    failure: null,
    esc_again_ms: escMs,
    fill: false,
  };
}

/** The grey block where the native view would be. */
function block(source: string): HTMLElement | null {
  if (typeof document === "undefined") return null;
  let el = document.querySelector<HTMLElement>(`[data-fake-site="${source}"]`);
  if (el === null) {
    el = document.createElement("div");
    el.dataset["fakeSite"] = source;
    el.className = "tf-fake-site";
    el.textContent = "The site's own page, drawn by the site";
    el.setAttribute("aria-hidden", "true");
    document.body.appendChild(el);
  }
  return el;
}

function place(site: FakeSite): void {
  const el = block(site.status.source);
  if (el === null) return;
  const on = site.status.shown && site.insets !== null;
  el.hidden = !on;
  // The inline display below would beat `hidden`: off is said here too.
  el.style.display = on ? "grid" : "none";
  if (!on || site.insets === null) return;
  const level = Number.parseFloat(getComputedStyle(document.documentElement).zoom || "1") || 1;
  const px = (v: number) => `${v / level}px`;
  Object.assign(el.style, {
    position: "fixed",
    top: px(site.insets.top),
    left: px(site.insets.left),
    right: px(site.insets.right),
    bottom: px(site.insets.bottom),
    background: "#d5d9de",
    color: "#4b5663",
    placeItems: "center",
    font: "13px system-ui, sans-serif",
    zIndex: "30",
  });
}

/** This feature's commands. */
export function handlers(params: URLSearchParams = new URLSearchParams()): Handlers {
  const off = params.get("fake-site") === "off";
  const dirty = params.has("fake-site-dirty");
  // `?fake-site-esc=<ms>`: how long the second Esc counts, so a screenshot can catch the first one's cap down.
  const escMs = Number(params.get("fake-site-esc") ?? 500) || 500;
  // `?fake-long-paths`: a site's first page at a long address, for the bar's address in `long-paths.mjs`.
  const first = (base: string) =>
    params.has("fake-long-paths")
      ? `${base}/display/PAY/folders with spaces/a repository-name-that-is-long and a page whose title runs on past any bar?view=edit&pageId=88213`
      : `${base}/`;
  const settings = new Map<string, SiteSetting>(
    Object.entries(BASES).map(([source, base]) => [
      source,
      {
        source,
        open_inside: !off && (source === "jira" || source === "confluence"),
        base,
        own: base,
        allowed: [],
        suggested: [],
        kept_since: source === "confluence" ? "2026-11-12T09:30:00Z" : null,
        offered: true,
        persists: true,
        // Confluence has a saved web sign-in, so the bar's *Fill sign-in* and Settings' row are drawn (ux29).
        sign_in_user: source === "confluence" ? "knobas" : null,
      },
    ]),
  );
  const sites = new Map<string, FakeSite>();
  let kept = 2;

  const emit = (site: FakeSite) => {
    site.status = { ...site.status, shown: site.wanted && site.status.phase === "live" };
    place(site);
    bridge?.emit("site:status", { ...site.status });
  };

  const commit = (site: FakeSite, url: string) => {
    const own = url.startsWith(BASES[site.status.source] ?? "\u0000");
    site.status = {
      ...site.status,
      phase: "live",
      url,
      own,
      fill: own && settings.get(site.status.source)?.sign_in_user != null,
      title: TITLES[site.status.source] ?? new URL(url).host,
      loading: false,
      can_back: site.history.length > 1,
    };
    emit(site);
  };

  const load = (site: FakeSite, url: string) => {
    site.history.push(url);
    site.at = site.history.length - 1;
    site.status = { ...site.status, loading: true };
    emit(site);
    setTimeout(() => commit(site, url), 250);
  };

  const asked = (source: string, force: boolean) => !force && dirty && sites.get(source)?.status.phase === "live";

  return {
    site_settings: () => [...settings.values()],
    set_site_open_inside: (args) => {
      const id = String(args["id"]);
      const one = { ...settings.get(id)!, open_inside: args["on"] === true };
      settings.set(id, one);
      return one;
    },
    site_allow: (args) => {
      const id = String(args["id"]);
      const origin = String(args["origin"]);
      const was = settings.get(id)!;
      const one = { ...was, allowed: [...was.allowed.filter((o) => o !== origin), origin], suggested: was.suggested.filter((o) => o !== origin) };
      settings.set(id, one);
      return one;
    },
    site_disallow: (args) => {
      const id = String(args["id"]);
      const was = settings.get(id)!;
      const one = { ...was, allowed: was.allowed.filter((o) => o !== String(args["origin"])) };
      settings.set(id, one);
      return one;
    },
    site_suggest: (args) => {
      const id = String(args["id"]);
      const was = settings.get(id);
      if (was && !was.suggested.includes(String(args["origin"]))) settings.set(id, { ...was, suggested: [...was.suggested, String(args["origin"])] });
      return null;
    },
    site_forget: (args) => {
      const id = String(args["id"]);
      if (asked(id, args["force"] === true)) return { done: false, asks: "unsaved" };
      sites.delete(id);
      block(id)?.remove();
      bridge?.emit("site:closed", { source: id, why: "forgot" });
      const was = settings.get(id);
      if (was) settings.set(id, { ...was, kept_since: null });
      return { done: true, asks: null };
    },
    site_open: (args) => {
      const source = String(args["source"]);
      const url = (args["url"] as string | null) ?? null;
      const base = BASES[source];
      if (base === undefined || settings.get(source)?.open_inside !== true) throw { code: "invalid", message: "this source's pages do not open inside knobas" };
      const existing = sites.get(source);
      if (existing !== undefined) {
        if (url === null) return { done: true, asks: null };
        if (asked(source, args["force"] === true)) return { done: false, asks: "unsaved" };
        load(existing, url);
        return { done: true, asks: null };
      }
      const site: FakeSite = { status: blank(source, escMs), insets: (args["insets"] as SiteInsets | undefined) ?? null, wanted: false, history: [], at: 0 };
      sites.set(source, site);
      emit(site);
      load(site, url ?? first(base));
      return { done: true, asks: null };
    },
    site_show: (args) => {
      const site = sites.get(String(args["source"]));
      if (site === undefined) return null;
      site.insets = args["insets"] as SiteInsets;
      site.wanted = true;
      if (args["focus"] === true) site.status = { ...site.status, keyboard: "site" };
      emit(site);
      return null;
    },
    site_hide: (args) => {
      const site = sites.get(String(args["source"]));
      if (site === undefined) return null;
      site.wanted = false;
      site.status = { ...site.status, keyboard: "shell" };
      emit(site);
      return null;
    },
    site_do: (args) => {
      const source = String(args["source"]);
      const site = sites.get(source);
      if (site === undefined) throw { code: "not_found", message: "the site is not open" };
      const act = String(args["act"]);
      if (act === "reload") {
        if (asked(source, args["force"] === true)) return { done: false, asks: "unsaved" };
        site.status = { ...site.status, loading: true };
        emit(site);
        setTimeout(() => commit(site, site.status.url ?? `${BASES[source]}/`), 250);
      } else if (act === "back" && site.at > 0) {
        site.at -= 1;
        commit(site, site.history[site.at]!);
        site.status = { ...site.status, can_back: site.at > 0, can_forward: true };
        emit(site);
      } else if (act === "forward" && site.at < site.history.length - 1) {
        site.at += 1;
        commit(site, site.history[site.at]!);
        site.status = { ...site.status, can_back: true, can_forward: site.at < site.history.length - 1 };
        emit(site);
      } else if (act === "focus" || act === "focus-shell") {
        site.status = { ...site.status, keyboard: act === "focus" ? "site" : "shell" };
        emit(site);
      }
      return { done: true, asks: null };
    },
    site_answer: (args) => {
      const site = sites.get(String(args["source"]));
      if (site === undefined) throw { code: "not_found", message: "the site is not open" };
      site.status = { ...site.status, phase: "live", asks_origin: null };
      emit(site);
      return null;
    },
    site_close: (args) => {
      const source = String(args["source"]);
      if (!sites.has(source)) throw { code: "not_found", message: "the site is not open" };
      if (asked(source, args["force"] === true)) return { done: false, asks: "unsaved" };
      sites.delete(source);
      block(source)?.remove();
      bridge?.emit("site:closed", { source, why: "asked" });
      return { done: true, asks: null };
    },
    sites: () => [...sites.values()].map((site) => site.status),
    sites_unsaved: () => (dirty ? [...sites.values()].map((site) => site.status) : []),
    site_quit: () => null,
    site_zoom: () => null,
    // The window's own look (ADR-0055): the browser has no bare window to colour.
    window_look_set: () => null,
    site_reveal_download: () => null,
    sites_kept: () => kept,
    set_sites_kept: (args) => {
      kept = Math.min(6, Math.max(1, Math.round(Number(args["count"]))));
      return kept;
    },
    site_sign_in_save: (args) => {
      const id = String(args["id"]);
      const username = String(args["username"] ?? "");
      if (username.trim() === "" || String(args["password"] ?? "") === "") throw { code: "invalid", message: "a sign-in needs a username and a password" };
      const one = { ...settings.get(id)!, sign_in_user: username };
      settings.set(id, one);
      return one;
    },
    site_sign_in_remove: (args) => {
      const id = String(args["id"]);
      const one = { ...settings.get(id)!, sign_in_user: null };
      settings.set(id, one);
      return one;
    },
    // `?fake-site-not-taken`: the site does not take the saved sign-in, for the bar's note.
    site_fill: (args) => {
      const site = sites.get(String(args["source"]));
      if (site === undefined) throw { code: "not_found", message: "the site is not open" };
      site.status = { ...site.status, fill: false, sign_in: "filling" };
      emit(site);
      setTimeout(() => {
        const { sign_in: _filling, ...rest } = site.status;
        site.status = params.has("fake-site-not-taken") ? { ...rest, fill: true, sign_in: "not_taken" } : { ...rest, fill: true };
        emit(site);
      }, 300);
      return null;
    },
  };
}
