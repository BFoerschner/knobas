/**
 * Settings › Updates' states by name (ADR-0052): what the tests and the `?fake-ipc` fixture (`?fake-update=<state>`) draw.
 * Data only; its addresses are reserved names that never resolve.
 */
import type { UpdateState } from "../../ipc/update";

const AT = (minutesAgo: number) => new Date(Date.now() - minutesAgo * 60_000).toISOString();
const MB = 1_000_000;

/** The pieces every state is made of. */
function base(): UpdateState {
  return {
    current: "3.2.9-ux27",
    current_is_pre_release: true,
    current_schema: 101,
    set_up: true,
    check: true,
    channel: "pre_releases",
    standing: { standing: "in_place", kind: "nsis" },
    checks_itself: true,
    checked_at: AT(12),
    next_check_at: new Date(Date.now() + 348 * 60_000).toISOString(),
    check_failed: null,
    newest: { version: "3.2.9-ux27", pre_release: true, published_at: AT(60 * 26), page_url: "https://releases.example/knobas/v3.2.9-ux27" },
    offer: null,
    phase: { phase: "idle" },
    marked: false,
    last: { from: "3.2.9-ux26", to: "3.2.9-ux27", at: AT(60 * 26), installed: true, backup: "knobas-20261120-110148.knobas" },
    just_updated: false,
  };
}

/**
 * The build on offer. Made when a state is asked for, like {@link base}, never when this file loads: a test pins its
 * clock after its imports run, so a time taken at load would be the wall clock's (light CI, 22 Nov 00:01).
 */
function offer() {
  return {
    version: "3.2.9-ux28",
    pre_release: true,
    published_at: AT(240),
    bytes: 89 * MB,
    changes_schema: true,
    page_url: "https://releases.example/knobas/v3.2.9-ux28",
  };
}

/** One named state. */
export function updateSample(name: string | null): UpdateState {
  const up = base();
  const OFFER = offer();
  const offered: UpdateState = {
    ...up,
    newest: { version: OFFER.version, pre_release: true, published_at: OFFER.published_at, page_url: OFFER.page_url },
    offer: OFFER,
    marked: true,
  };
  switch (name) {
    case "checks-off":
      return { ...up, check: false, next_check_at: null };
    case "checking":
      return { ...up, phase: { phase: "checking" } };
    case "available":
      return offered;
    case "downloading":
      return { ...offered, phase: { phase: "downloading", received: 41 * MB, total: 89 * MB } };
    case "ready":
      return { ...offered, phase: { phase: "ready" } };
    case "installing":
      return { ...offered, phase: { phase: "installing", step: "backing_up" } };
    case "signature":
      return { ...offered, phase: { phase: "failed", stage: "signature", message: "the signature does not match the file", received: null } };
    case "download-failed":
      return { ...offered, phase: { phase: "failed", stage: "download", message: "no bytes for 30s", received: 41 * MB } };
    case "not-installed":
      return { ...offered, phase: { phase: "ready" }, last: { from: "3.2.9-ux27", to: "3.2.9-ux28", at: AT(30), installed: false, backup: "knobas-20261121-143012.knobas" } };
    case "check-failed":
      return {
        ...up,
        channel: "releases",
        current: "3.2.9",
        current_is_pre_release: false,
        newest: { version: "3.2.9", pre_release: false, published_at: AT(60 * 24 * 3), page_url: "https://releases.example/knobas/v3.2.9" },
        check_failed: { kind: "offline", message: "error sending request", at: AT(2) },
      };
    case "package":
      return {
        ...offered,
        current: "3.2.8",
        current_is_pre_release: false,
        channel: "releases",
        standing: { standing: "by_hand", why: "package", kind: "deb" },
        newest: { version: "3.2.9", pre_release: false, published_at: AT(60 * 24 * 3), page_url: "https://releases.example/knobas/v3.2.9" },
        offer: { ...OFFER, version: "3.2.9", pre_release: false, bytes: 63 * MB, page_url: "https://releases.example/knobas/v3.2.9" },
        last: null,
      };
    case "pre-on-releases":
      return {
        ...up,
        current: "3.2.10-ux3",
        channel: "releases",
        newest: { version: "3.2.9", pre_release: false, published_at: AT(60 * 24 * 3), page_url: "https://releases.example/knobas/v3.2.9" },
        last: null,
      };
    case "development":
      return { ...up, standing: { standing: "by_hand", why: "development", kind: null }, checks_itself: false, next_check_at: null, checked_at: null, newest: null, last: null };
    case "just-updated":
      return {
        ...up,
        current: "3.2.9-ux28",
        newest: { version: OFFER.version, pre_release: true, published_at: OFFER.published_at, page_url: OFFER.page_url },
        last: { from: "3.2.9-ux27", to: "3.2.9-ux28", at: AT(1), installed: true, backup: "knobas-20261121-140112.knobas" },
        just_updated: true,
      };
    case "not-set-up":
      return { ...up, set_up: false, checks_itself: false, next_check_at: null, checked_at: null, newest: null, last: null };
    default:
      return up;
  }
}

