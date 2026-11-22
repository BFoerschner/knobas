// Generated from the Rust types by ts-rs; do not edit. Regenerate with `cargo test -p knobas-app --lib ipc_mirror` (crates/knobas-app/src/ipc_mirror.rs).

/**
 * A site that signs in with its source's own credential (ux30).
 */
export type OwnSignIn = { 
/**
 * The credential's username: the source's `username` setting.
 */
user: string, 
/**
 * Not turned off. A saved sign-in still wins over it.
 */
on: boolean, };

/**
 * What `site_do` does.
 */
export type SiteAct = "back" | "forward" | "reload" | "stop" | "focus" | "focus-shell";

/**
 * What a closing, reloading or re-pointing command answers.
 */
export type SiteAsked = { 
/**
 * The act happened.
 */
done: boolean, 
/**
 * Why it did not: the page holds changes it has not saved, and the
 * person has to say.
 */
asks: SiteAsks | null, };

/**
 * The one question a command can answer with.
 */
export type SiteAsks = "unsaved";

/**
 * A view closed (`site:closed`).
 */
export type SiteClosed = { source: string, why: SiteClosedWhy, };

export type SiteClosedWhy = "asked" | "room" | "idle" | "forgot" | "declined";

/**
 * A download's progress (`site:download`).
 */
export type SiteDownload = { source: string, id: number, name: string, state: SiteDownloadState, };

export type SiteDownloadState = "started" | "saved" | "failed";

/**
 * Why nothing loaded.
 */
export type SiteFailure = { "kind": "timeout" } | { "kind": "certificate" } | { "kind": "not_found" } | { "kind": "refused" } | { "kind": "other", "detail": string };

/**
 * The distance of a site's rectangle from the main window's four edges, in
 * logical pixels.
 */
export type SiteInsets = { top: number, right: number, bottom: number, left: number, };

/**
 * A claimed key (`site:key`).
 */
export type SiteKey = { source: string, 
/**
 * A name, never a key code: `search`, `settings`, `timer`, `reload`,
 * `back`, `forward`, `zoom-in`, `zoom-out`, `zoom-reset`, `close`,
 * `bar`, `esc-armed`, `esc-leave`, `quit`.
 */
key: string, };

/**
 * Which side holds the keyboard for a site.
 */
export type SiteKeyboard = "site" | "shell";

/**
 * Where a site is, as its rectangle draws it. A closed site has no state.
 */
export type SitePhase = "opening" | "asks" | "live" | "failed" | "stopped";

/**
 * Where a navigation the site may not show went (`site:routed`).
 */
export type SiteRouted = { source: string, to: SiteRoutedTo, 
/**
 * The host, or the scheme for a refused address.
 */
host: string, 
/**
 * The address, for the page to hand to `openExternal` or `openWeb`.
 * Empty for a refused address.
 */
url: string, 
/**
 * For [`SiteRoutedTo::Site`]: the source whose site takes it.
 */
other: string | null, };

export type SiteRoutedTo = "browser" | "site" | "refused";

/**
 * One source's site, as Settings and `openWeb` read it.
 */
export type SiteSetting = { source: string, 
/**
 * *Open its pages inside knobas*.
 */
open_inside: boolean, 
/**
 * The source's base URL, which `openWeb` matches addresses against.
 */
base: string, 
/**
 * `own(S)`, as scheme, host and port (`https://jira.example`); empty when the base URL is not
 * an http(s) address.
 */
own: string, 
/**
 * The origins the person allowed.
 */
allowed: Array<string>, 
/**
 * Origins *Open in ⟨source⟩* named that went to the browser, offered
 * with *Allow* until knobas quits.
 */
suggested: Array<string>, 
/**
 * When a page on the source's own address first loaded after the last
 * *Forget…*, RFC 3339.
 */
kept_since: string | null, 
/**
 * Sites open inside knobas on this system (decisions §23).
 */
offered: boolean, 
/**
 * The sign-in outlives knobas here (macOS 14 and later, Windows).
 */
persists: boolean, 
/**
 * The username of the saved web sign-in (ux29), when one is saved. The
 * password never leaves the keychain item.
 */
sign_in_user: string | null, 
/**
 * The site signs in with the source's own credential (ux30): its
 * adapter says so, and the source signs in by username and password.
 * `None` for every other site.
 */
own_sign_in: OwnSignIn | null, 
/**
 * The site loads nothing from other websites but the allowed ones: a
 * password manager's (ux30). Settings says so beside the addresses.
 */
confined: boolean, };

/**
 * What the bar says about the sign-in (`SiteStatus::sign_in`).
 */
export type SiteSignIn = "filling" | "not_taken" | "second_factor" | "no_form" | "ambiguous" | "provider";

/**
 * One live site, as the page draws it (`site:status`). Every field is text
 * or a flag; nothing is HTML.
 */
export type SiteStatus = { source: string, phase: SitePhase, 
/**
 * The committed top-level address.
 */
url: string | null, 
/**
 * The page's title, cleaned, at most 200 characters.
 */
title: string, 
/**
 * The address is on the source's own origin.
 */
own: boolean, loading: boolean, can_back: boolean, can_forward: boolean, 
/**
 * The view is on screen.
 */
shown: boolean, keyboard: SiteKeyboard, 
/**
 * The origin the rectangle asks about, while [`SitePhase::Asks`].
 */
asks_origin: string | null, failure: SiteFailure | null, esc_again_ms: number, 
/**
 * *Fill sign-in* is drawn: a web sign-in is saved, the page is on the
 * source's own origin, and no fill is under way (ux29).
 */
fill: boolean, 
/**
 * What the bar says about the saved sign-in.
 */
sign_in?: SiteSignIn, };
