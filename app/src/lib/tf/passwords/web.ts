/**
 * *Open web interface* (ux30, Fable's rulings while Björn was busy, 2026-11-22): whether the Passwords page draws it
 * for what is open, and its tooltip. The address is the Rust side's (`password_web_link`), read at press time; here are
 * only the words, decided from what the window holds.
 *
 * The label never names the source or its vendor (Björn's decision 2): it is always "Open web interface". The press
 * opens the source's site inside knobas where *Open its pages inside knobas* is on (on by default for a password
 * source), else the browser; Shift W or a ⌘/Ctrl or middle press always the browser.
 */
import type { PwSource, WebPages } from "../../ipc/passwords";
import type { SiteSetting } from "../../ipc/site";

/** What the press is on: an entry, a folder, or the source itself (its top, or the page's top with one source). */
export type WebOn = "entry" | "folder" | "home";

/** The button's words, or `null` where it is not drawn. */
export interface WebWords {
  title: string;
}

/** Which page a press opens, as `password_web_link` falls back: its own, else its folder's, else the front page. */
export function webOpens(on: WebOn, pages: WebPages, hasFolder: boolean): "entry" | "entry_folder" | "folder" | "home" {
  if (on === "entry") {
    if (pages.entry) return "entry";
    return pages.folder && hasFolder ? "entry_folder" : "home";
  }
  if (on === "folder" && pages.folder) return "folder";
  return "home";
}

const OPENS: Record<ReturnType<typeof webOpens>, string> = {
  entry: "Opens this entry in the password manager's web interface",
  entry_folder: "Opens this entry's folder in the password manager's web interface",
  folder: "Opens this folder in the password manager's web interface",
  home: "Opens the password manager's web interface",
};

/** Whether a press opens the site inside knobas rather than the browser. */
export function opensInside(setting: SiteSetting | undefined): boolean {
  return setting !== undefined && setting.open_inside && setting.offered;
}

/** The username a sign-in will be filled with in the site, or `null` when the person signs in. */
export function fillsAs(setting: SiteSetting | undefined): string | null {
  if (setting === undefined) return null;
  if (setting.sign_in_user != null) return setting.sign_in_user;
  return setting.own_sign_in?.on === true ? setting.own_sign_in.user : null;
}

/**
 * The button for `source` on `on`, or `null` where none is drawn: a source whose base URL is no web address
 * (`own` empty) has none.
 */
export function webWords(source: PwSource, on: WebOn, hasFolder: boolean, setting: SiteSetting | undefined): WebWords | null {
  if (setting === undefined || setting.own === "") return null;
  const inside = opensInside(setting);
  const as = inside ? fillsAs(setting) : null;
  const ending = inside ? (as !== null ? ", inside knobas and signed in." : ", inside knobas.") : ", in your browser.";
  let title = OPENS[webOpens(on, source.web, hasFolder)] + ending;
  if (as !== null && source.offers.audit_log) title += ` A new sign-in is recorded there under ${as}.`;
  if (inside) title += " Shift W opens it in your browser instead.";
  return { title };
}
