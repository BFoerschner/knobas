/**
 * A source's own website inside knobas (ADR-0050) — `crates/knobas-app/src/commands/site.rs`.
 *
 * The types are generated from the Rust (`./generated/site.ts`): a field is explained in its Rust doc comment and
 * nowhere else. The `invoke` functions stay hand-written, pinned to the Rust by a test in that module which
 * `include_str!`s this file.
 */
import { invoke } from "./invoke";

import type { SiteAct, SiteAsked, SiteInsets, SiteSetting, SiteStatus } from "./generated/site";

export type {
  SiteAct,
  SiteAsked,
  SiteAsks,
  SiteClosed,
  SiteClosedWhy,
  SiteDownload,
  SiteDownloadState,
  SiteFailure,
  SiteInsets,
  SiteKey,
  SiteKeyboard,
  SitePhase,
  SiteRouted,
  SiteRoutedTo,
  SiteSetting,
  SiteSignIn,
  SiteStatus,
} from "./generated/site";

/** Every source's site setting; also brings the native rules in line with what is stored. */
export function siteSettings(): Promise<SiteSetting[]> {
  return invoke<SiteSetting[]>("site_settings");
}

/** *Open its pages inside knobas* for one source. */
export function setSiteOpenInside(id: string, on: boolean): Promise<SiteSetting> {
  return invoke<SiteSetting>("set_site_open_inside", { id, on });
}

/** Add an origin (`http(s)://host[:port]`) to the ones a site may show. */
export function siteAllow(id: string, origin: string): Promise<SiteSetting> {
  return invoke<SiteSetting>("site_allow", { id, origin });
}

/** Remove an allowed origin. */
export function siteDisallow(id: string, origin: string): Promise<SiteSetting> {
  return invoke<SiteSetting>("site_disallow", { id, origin });
}

/** An *Open in ⟨source⟩* named an origin outside the site's and went to the browser: offered in Settings. */
export function siteSuggest(id: string, origin: string): Promise<void> {
  return invoke<void>("site_suggest", { id, origin });
}

/** Close the site, asking first unless `force`, and remove its session. */
export function siteForget(id: string, force: boolean): Promise<SiteAsked> {
  return invoke<SiteAsked>("site_forget", { id, force });
}

/** Make the site's view, or point it at `url`, asking the page first unless `force`. */
export function siteOpen(source: string, url: string | null, insets: SiteInsets, force: boolean): Promise<SiteAsked> {
  return invoke<SiteAsked>("site_open", { source, url, insets, force });
}

/** Show the site's view at `insets`, when it has a page. */
export function siteShow(source: string, insets: SiteInsets, focus: boolean, esc: boolean): Promise<void> {
  return invoke<void>("site_show", { source, insets, focus, esc });
}

/** Hide the site's view; its page keeps running. */
export function siteHide(source: string): Promise<void> {
  return invoke<void>("site_hide", { source });
}

/** Back, forward, reload, stop, or the keyboard to the site or to knobas. */
export function siteDo(source: string, act: SiteAct, force: boolean): Promise<SiteAsked> {
  return invoke<SiteAsked>("site_do", { source, act, force });
}

/** The answer to "⟨source⟩ wants to continue at another address": declined, the stopped address for the browser. */
export function siteAnswer(source: string, origin: string, allow: boolean): Promise<string | null> {
  return invoke<string | null>("site_answer", { source, origin, allow });
}

/** Close the site's view, asking the page first unless `force`. */
export function siteClose(source: string, force: boolean): Promise<SiteAsked> {
  return invoke<SiteAsked>("site_close", { source, force });
}

/** The live sites. */
export function sites(): Promise<SiteStatus[]> {
  return invoke<SiteStatus[]>("sites");
}

/** The live sites whose pages hold changes. */
export function sitesUnsaved(): Promise<SiteStatus[]> {
  return invoke<SiteStatus[]>("sites_unsaved");
}

/** *Quit anyway*. */
export function siteQuit(): Promise<void> {
  return invoke<void>("site_quit");
}

/** The window's zoom level, for every live site and each new one. */
export function siteZoom(level: number): Promise<void> {
  return invoke<void>("site_zoom", { level });
}

/** Show a download knobas recorded in the file manager. */
export function siteRevealDownload(id: number): Promise<void> {
  return invoke<void>("site_reveal_download", { id });
}

/** *Sites kept open*. */
export function sitesKept(): Promise<number> {
  return invoke<number>("sites_kept");
}

/** Store *Sites kept open* (1 to 6). */
export function setSitesKept(count: number): Promise<number> {
  return invoke<number>("set_sites_kept", { count });
}

/** Save or change the web sign-in typed into the site's own form (ux29). Nothing reads the password back. */
export function siteSignInSave(id: string, username: string, password: string): Promise<SiteSetting> {
  return invoke<SiteSetting>("site_sign_in_save", { id, username, password });
}

/** Remove the site's saved web sign-in. */
export function siteSignInRemove(id: string): Promise<SiteSetting> {
  return invoke<SiteSetting>("site_sign_in_remove", { id });
}

/** *Fill sign-in*: the saved sign-in into the site's page now, if it is on the source's own origin. */
export function siteFill(source: string): Promise<void> {
  return invoke<void>("site_fill", { source });
}
