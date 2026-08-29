/**
 * Credential health, live — one fact, one home.
 *
 * Spec §3: *"Credential health: PAT expiry countdown, 401 detection →*
 * Re-enter password*"*. Three surfaces draw it — the top strip's monograms,
 * the sources view's rows, and the launcher's per-row provenance — and before
 * this module they drew it from two independent copies: `launcher_home.sources`
 * (fetched once per ⌘K opening) and a `sources` prop nothing supplied.
 *
 * That is the bug issue #27's review found and this module closes. A
 * per-opening fetch is stale by construction: the scheduler emits
 * `source:health` the moment a credential is rejected, and a launcher opened
 * before that keeps drawing "synced 4 min ago" over a source that has been
 * refusing knobas' password since. The subscription is the fix, and holding it
 * once at the shell rather than once per component is what keeps the three
 * surfaces from disagreeing.
 *
 * **Every `listen()` is cleaned up**, including the "unmounted before the
 * promise resolved" race: `listen` is itself an `invoke`, so it resolves a
 * tick or more after the caller may already be gone.
 */
import { listen as tauriListen } from "@tauri-apps/api/event";

import { EVENTS } from "../ipc";
import { credentialHealth as realCredentialHealth, type AuthState, type CredentialHealth } from "../ipc/sources";

/**
 * The states that mean *a human has to do something*.
 *
 * **`unknown` is deliberately not one of them.** It is migration 0002's
 * default — nothing has tested the credential yet — which is the state every
 * source is in until the scheduler's first run. Counting it would put a
 * complaint on every source of a fresh install, which is both wrong and the
 * loudest possible way to be wrong.
 *
 * **A total record over `AuthState`, not a `Set<AuthState>`** — #37's shape,
 * kept when this rule moved here from `launcher/format.ts`. A set can be
 * missing a member and still compile, so a state added on the Rust side falls
 * through to "not actionable" silently; that direction is safe and therefore
 * never noticed, which is the worse half. A `Record<AuthState, boolean>` has to
 * name every state: adding one to `AuthState` makes this literal incomplete and
 * fails `svelte-check`, which is the gate `just front` runs. Rust's
 * `sources_mirror::the_auth_state_union_matches_the_rust_enum` is the other
 * half of that chain — it pins the union to `AuthState::ALL`.
 */
const ACTIONABLE: Record<AuthState, boolean> = {
  ok: false,
  // Never tested, not broken — see above.
  unknown: false,
  unauthorized: true,
  unreachable: true,
  missing_secret: true,
};

/**
 * Whether this state is one a person has to resolve.
 *
 * The one spelling of the rule. Four places branch on it — `launcher/format.ts`
 * row provenance, `launcher/Board.svelte`'s source strip, `launcher/Chips.svelte`'s
 * chip dot and this module's own {@link Health.failing} — and four inline
 * copies is four chances for them to disagree about what a red dot means.
 *
 * **`undefined` is a state too, and it is answered here** (#72). A caller can
 * hold a source id the health store has no row for — a `/gitea` chip typed
 * before the first scheduler run — and "no reading yet" deserves exactly the
 * answer `unknown` gets. Taking `AuthState | undefined` is what keeps that
 * half of the rule from living at the call site: while the signature forbade
 * `undefined`, `Chips.svelte` had to guard for it itself, and a rule with a
 * clause outside its own function is a rule that can be spelled two ways.
 *
 * Two doors, then, and both close on "do not shout about it": `undefined` for
 * the reading that has not arrived, and `=== true` rather than a bare lookup
 * for the one that arrived in a shape this build has no name for. The second
 * is not the first made redundant — `AuthState` is a mirror of a Rust enum
 * across the IPC bridge, so a newer backend can hand over a variant that
 * satisfies no branch of the union the compiler checked.
 *
 * The widening costs something, and it is worth naming here rather than
 * discovering later: every *other* caller — `launcher/format.ts`,
 * `launcher/Board.svelte`, `shell/TopStrip.svelte`, `sources/SourceRow.svelte`
 * and {@link Health.failing} — passes a `CredentialHealth.state`, which is
 * required, so for them the parameter is now looser than the value. Were that
 * field ever to become optional, those five would quietly read `false` instead
 * of failing `svelte-check` — the same silent-safe direction the table above
 * is a `Record` and not a `Set` to avoid. The trade is taken deliberately: one
 * caller genuinely holds `undefined` (a chip naming a source the store has no
 * row for), and a rule that cannot answer for its own missing case is a rule
 * every caller has to finish.
 */
export function isActionable(state: AuthState | undefined): boolean {
  return state !== undefined && ACTIONABLE[state] === true;
}

/** The IPC this store needs, injectable so a test needs no Tauri bridge. */
export interface HealthPorts {
  credentialHealth: () => Promise<CredentialHealth[]>;
  listen: (
    event: string,
    handler: (event: { payload: CredentialHealth }) => void,
  ) => Promise<() => void>;
}

/** The live credential health of every source knobas knows about. */
export interface Health {
  /** Every source, by `source_id`, so the strip's order never jitters. */
  readonly all: CredentialHealth[];
  /** The ones a person has to act on. */
  readonly failing: CredentialHealth[];
  /** Whether any source is refusing knobas' credential — the strip's `401`. */
  readonly unauthorized: boolean;
  get(sourceId: string): CredentialHealth | null;
  /** Apply one reading. Exposed so a mutation elsewhere can seed it directly. */
  patch(next: CredentialHealth): void;
  /**
   * Take a whole authoritative *membership*, keeping the newer reading per row.
   *
   * **Two halves, and they answer to different authorities** — this used to
   * read "take a whole authoritative reading, forgetting anything not in it",
   * and half of that is no longer true (#148):
   *
   * - **Membership is the incoming set's, absolutely.** A source absent from
   *   `rows` leaves the store, whatever this store holds for it and however
   *   fresh that is. This is the only operation that can *remove* a source:
   *   {@link patch} only ever adds, so a view that re-lists after a delete and
   *   patches each surviving row leaves the deleted one drawing its monogram
   *   and its room tab for the rest of the session.
   * - **Per surviving row, the newer `checked_at` wins.** A reading this store
   *   already holds survives only when its `checked_at` is *strictly* newer
   *   than the incoming one's. Incoming wins ties, wins when both are null,
   *   and wins when the held reading has no `checked_at`; a reading with a
   *   timestamp beats one without. Ties go to the incoming set because a tie
   *   is two readings of one check, so neither is staler — and the set that
   *   also decides membership is the one to believe when nothing separates
   *   them. A genuine tie carrying *different* content would mean that is no
   *   longer true, and is worth reporting rather than papering over.
   *
   * **Why the second half does not weaken the first**, which is the argument
   * worth not making the next reader re-derive: a *deleted* source is gone
   * from the incoming set entirely rather than sitting in it with a stale
   * reading. Freshness never gets a say about it, so the deleted chip still
   * disappears — pinned by `SourcesView`'s "a deleted source leaves the shared
   * health store, not just the list" and by "replace forgets a source the
   * authoritative list no longer carries" below it.
   *
   * **Why newest-wins is safe to decide here.** Both timestamps are the
   * backend's own `checked_at` for the same source — the `source:health` event
   * carries it at check time, `list_sources` and `credential_health` carry the
   * same column at snapshot time — so this compares one clock against itself.
   * There is no skew to allow for.
   *
   * **What it costs.** `replace` can no longer rewind a row: a backend flow
   * that wanted to reset a source's reading *without* removing the row and
   * without emitting a newer event would be ignored here until the next check.
   * Nothing does that today — every reading in the app is the result of an
   * actual credential check, stamped with when that check happened, whether it
   * reaches the store through the scheduler's event, `list_sources`,
   * `credential_health` or `set_source_secret`'s answer. If such a flow ever
   * appears, this rule is the thing to re-open rather than the caller.
   *
   * The reason the rule lives here rather than in a caller: `SourcesView.load()`
   * is one door and the shell's boot-time {@link reseed} is the other, and both
   * end in this line. Fixing one caller leaves the other exposed (#148).
   */
  replace(rows: CredentialHealth[]): void;
  /**
   * Read the whole set from `credential_health`.
   *
   * The shell's to call, and **only once the database is ready** — the command
   * answers `not_ready` until the embedded Postgres is up. Also awaited by a
   * caller that has just changed the set of sources.
   */
  reseed(): Promise<void>;
  /**
   * Subscribe to `source:health`. Returns the teardown; calling it twice is
   * harmless.
   *
   * **Subscribing only.** The seed is {@link reseed}, and it is the shell's
   * because only the shell knows when the database can answer — see the note
   * on `reseed`.
   */
  start(): () => void;
}

/**
 * When a reading was taken, as an instant — `null` for one that carries no
 * usable answer.
 *
 * **Parsed, not compared as a string.** `checked_at` is "RFC 3339, or null if
 * it has never been checked" (`ipc/sources.ts`), and RFC 3339 admits an offset:
 * `2026-08-25T13:00:00+02:00` is an hour *earlier* than `2026-08-25T12:30:00Z`
 * and sorts after it lexically. Every stamp knobas has seen from the backend is
 * `Z`, which is exactly what makes a string comparison the kind of shortcut
 * that works until the day it does not.
 *
 * An unparseable stamp answers `null` — the same as never having been checked.
 * A reading whose timestamp cannot be read is not evidence of being newer, and
 * the safe direction for a value coming across the IPC bridge is to let the
 * authoritative set win.
 */
function checkedAt(row: CredentialHealth): number | null {
  if (!row.checked_at) return null;
  const at = Date.parse(row.checked_at);
  return Number.isNaN(at) ? null : at;
}

/**
 * Whether `held` is a *strictly* newer reading than `incoming`.
 *
 * The whole of {@link Health.replace}'s per-row rule, in one place so the two
 * doors into `replace` cannot disagree about it. *Strictly* newer, so every
 * draw goes to the incoming set — the reasoning is on `replace`. A timestamped
 * reading beats one with no timestamp, in either position.
 */
function isNewer(held: CredentialHealth, incoming: CredentialHealth): boolean {
  const heldAt = checkedAt(held);
  if (heldAt === null) return false;
  const incomingAt = checkedAt(incoming);
  return incomingAt === null || heldAt > incomingAt;
}

export function createHealth(ports?: HealthPorts): Health {
  const io: HealthPorts = ports ?? {
    credentialHealth: realCredentialHealth,
    listen: (event, handler) => tauriListen<CredentialHealth>(event, handler),
  };

  /**
   * A map rather than an array, and a whole-object reassignment rather than an
   * index write: the store is patched one source at a time from an event, and
   * a keyed map is what makes "patch one, leave the others" the shape of the
   * code instead of a search that has to find the right row.
   */
  const state = $state<{ by: Record<string, CredentialHealth> }>({ by: {} });

  let live = false;

  function apply(next: CredentialHealth) {
    state.by = { ...state.by, [next.source_id]: next };
  }

  return {
    get all() {
      return Object.values(state.by).sort((a, b) => a.source_id.localeCompare(b.source_id));
    },
    get failing() {
      return this.all.filter((row) => isActionable(row.state));
    },
    get unauthorized() {
      return this.all.some((row) => row.state === "unauthorized");
    },
    get(sourceId: string) {
      return state.by[sourceId] ?? null;
    },
    patch: apply,
    replace(rows: CredentialHealth[]) {
      const by: Record<string, CredentialHealth> = {};
      for (const row of rows) {
        const held = state.by[row.source_id];
        // Membership from `rows` either way — the row is written on every
        // branch — and only *which reading* it carries is in question.
        by[row.source_id] = held !== undefined && isNewer(held, row) ? held : row;
      }
      state.by = by;
    },
    async reseed() {
      try {
        const rows = await io.credentialHealth();
        if (!live) return;
        this.replace(rows);
      } catch {
        // The keychain being locked answers `internal`, and that is not a
        // reason to take the window down — the subscription still delivers.
        //
        // It is no longer where a `not_ready` goes to die: the shell calls
        // this only once the lifecycle says the database is up, so a rejection
        // here is a real fault rather than the ordinary boot race it used to
        // be mistaken for.
      }
    },
    start() {
      if (live) {
        // Already subscribed. Handing back a teardown that unwinds the *first*
        // subscription would strand it if the second caller stops first, so
        // this one is a no-op and the first `stop()` remains the real one.
        return () => {};
      }
      live = true;
      let off: (() => void) | undefined;

      // Subscribed at shell start, long before the seed the shell fires when
      // the database is ready: an event that lands in between is delivered
      // rather than lost.
      //
      // What follows used to be a hope about ordering — "the seed that follows
      // is the newer reading" — and it was wrong for the window that matters:
      // a `credential_health` read *already in flight* when the event lands
      // answers from before it, and `replace` wrote that older reading back
      // over the event. Since #148 it is a guarantee instead of a hope, and it
      // is `replace`'s newest-wins rule that makes it one: whichever of the
      // two readings the backend checked later is the one that stands, in
      // whichever order they arrive here.
      void io
        .listen(EVENTS.sourceHealth, (event) => {
          if (live) apply(event.payload);
        })
        .then((unlisten) => {
          if (live) off = unlisten;
          else unlisten();
        })
        .catch(() => {
          // A failed subscription is not a failed window; the seed still shows.
        });

      return () => {
        live = false;
        off?.();
        off = undefined;
      };
    },
  };
}

/**
 * The one the window uses.
 *
 * Module-level, like `toasts`, because three components in two trees read it
 * and the alternative — a context threaded from `App.svelte` through the top
 * strip, the sources view and the launcher — is ceremony around a map of at
 * most a handful of rows. Tests build their own with {@link createHealth}.
 */
export const health = createHealth();

/** A PAT expiry reading: what to say, and how alarmed to look saying it. */
export interface ExpiryNote {
  text: string;
  tone: "plain" | "amber" | "fail";
}

/**
 * Amber from two weeks out.
 *
 * Long enough that there is a working week in which to rotate the token
 * before the sync stops, short enough that the countdown is not permanently
 * amber on a 90-day PAT — a warning that is always on is a warning nobody
 * reads.
 */
const AMBER_DAYS = 14;

/**
 * The PAT expiry countdown — spec §3.
 *
 * `null` for a source whose credential has no expiry, or whose server will not
 * say: the great majority of sources, and a line saying "expires: unknown" on
 * every one of them is noise.
 *
 * An expired secret reads *expired*, never "-3 days". A negative duration is a
 * fact about arithmetic, not about the credential, and it is the reading a
 * person is least able to act on.
 */
export function expiryNote(iso: string | null | undefined, now: Date = new Date()): ExpiryNote | null {
  if (!iso) return null;
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return null;

  const ms = at.getTime() - now.getTime();
  if (ms <= 0) return { text: "PAT expired", tone: "fail" };

  // Rounded up: with 30 hours left, "in 1 day" is true and "in 1 day" rounded
  // down would read "in 1 day" as well — but with 6 hours left, rounding down
  // gives "in 0 days", which is either wrong or alarming for the wrong reason.
  const days = Math.ceil(ms / 86_400_000);
  return {
    text: `PAT expires in ${days} ${days === 1 ? "day" : "days"}`,
    tone: days <= AMBER_DAYS ? "amber" : "plain",
  };
}
