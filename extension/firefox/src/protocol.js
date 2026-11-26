// The Native Messaging contract with knobas (ADR-0061). The Rust side holds
// the same values in crates/knobas-app/src/link_handoff/protocol.rs; the
// extension tests read that file and fail when the two drift apart.

/** The host name knobas registers its Native Messaging manifest under. */
export const HOST_NAME = "dev.knobas.desktop";

/** The add-on ID the host manifest allows; `manifest.json` carries it as the gecko ID. */
export const EXTENSION_ID = "link-handoff@knobas.dev";

/** The protocol this extension speaks; `ping` and `get_rules` answer with theirs. */
export const PROTOCOL_VERSION = 1;

/**
 * How long a `handoff` may take before the page loads in Firefox. The host
 * answers `knobas_not_running` after 2 s, and knobas acts on a request only
 * within 1.5 s of the host sending it, so past this nothing opens in knobas
 * and loading the page cannot open it twice. That holds because the host
 * gets to the request at once: native.js never queues it behind another one
 * on the same port.
 */
export const HANDOFF_TIMEOUT_MS = 2500;

/** How long a rules refresh may hold up a navigation before the cached rules are used. */
export const RULES_TIMEOUT_MS = 1000;

/** How old the cached rules may be before the next navigation fetches them again. */
export const RULES_TTL_MS = 10_000;
