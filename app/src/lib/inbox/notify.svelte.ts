/**
 * Desktop notifications for inbox items (issue #290, spec #272
 * "Notifications").
 *
 * One store, because two surfaces read the same fact: the settings section
 * draws a checkbox per kind, and the listener below decides whether an item
 * may interrupt somebody. Two independent reads of one setting is two chances
 * for the switch on screen to disagree with the switch that fires — the
 * argument `health.svelte.ts` and `inbox.svelte.ts` both record for their own
 * stores.
 *
 * ## The three gates, and the order they are in
 *
 * An item notifies only if it is **new**, its **kind is on**, and the window
 * is **unfocused**. The order matters in one place: an item is marked seen
 * whatever the focus, so something that arrived while the reader was looking
 * at it does not fire a notification the moment they switch away. Spec story
 * 74 is "I am never told what is already on screen", and an item that waited
 * for a blur would be exactly that.
 *
 * **Nothing here reads the OS permission.** Asking for it is
 * {@link Notifications.choose}'s, on the click that switches a kind on — so a
 * silence in this path is always attributable to the kind or to the focus, and
 * never to a permission the notifier failed to check.
 *
 * ## Why {@link Notifications.saw} does *not* wait for the setting
 *
 * It looks as though it should: `seen` is a one-way memory, so a stream
 * offered while the stored kinds are still in flight — or while a failed read
 * waits behind the section's *Retry* — marks items the reader can then never
 * be told about. A `loaded` gate was written, and then taken out again,
 * because it changes nothing anybody can observe: while the setting is
 * unknown no kind is on, so those items are silent either way, and the only
 * question is *which* stream gets spent as the backlog below. Gating simply
 * moves that to the next one, and the next one contains the same items plus
 * anything that has since arrived — so if it differs at all, it differs by
 * swallowing more. Written down because the guard reads as an obvious
 * omission; it is a decoration, and its mutant proved it by surviving.
 *
 * ## Why the first read is primed and not announced
 *
 * The first stream the inbox answers with is the **backlog** — everything that
 * needs the reader now, including whatever arrived while knobas was shut. A
 * notification per line of it is a burst about nothing new, so the first
 * observation records the keys and says nothing. What "first" means is the
 * shell's: `App.svelte` feeds this only once `inbox.answered` is true, because
 * the empty stream a store holds before its first read would otherwise prime
 * against nothing and the backlog would arrive as news.
 *
 * ## What is remembered, and for how long
 *
 * Every key ever seen this session, and it is never pruned. An item leaves the
 * stream when it is answered or resolved at the source, and its key is the
 * *demand* rather than the occurrence (`<category>:<subject>`) — so an item
 * that leaves and comes back is the same key, and re-announcing it is what
 * "once per item" forbids. The memory is a session's: a restart is where a
 * failed build that has failed again gets to speak up.
 *
 * ## The click, and the one thing this cannot witness
 *
 * A notification carries the item's address in `extra`, and
 * {@link Notifications.start} subscribes to the plugin's own action channel to
 * navigate there. **On desktop that channel is never fed today.**
 * `tauri-plugin-notification` 2.4.0 registers exactly three commands
 * (`is_permission_granted`, `request_permission`, `notify`); its desktop
 * `notify` hands the notification to `notify-rust` and returns, and
 * `register_listener` — which `onAction` invokes — exists on mobile only. So
 * the subscription is expected to reject on macOS, is caught, and costs
 * nothing; the navigation itself is real, tested, and waits for a plugin that
 * reports the click. This is written down rather than left out because the
 * alternative is a click path that quietly does not exist.
 */
import {
  isPermissionGranted as realIsPermissionGranted,
  onAction as realOnAction,
  requestPermission as realRequestPermission,
  sendNotification as realSendNotification,
} from "@tauri-apps/plugin-notification";

import { ipcErrorMessage } from "../ipc";
import {
  notificationKinds as realRead,
  setNotificationKinds as realWrite,
  type InboxCategory,
  type InboxEntry,
  type InboxItem,
} from "../ipc/entity";
import { DEFAULT_CTX, hashFor, router } from "../shell/router.svelte";

/**
 * The key in a notification's `extra` that carries where its click goes.
 *
 * Named once because it is the two ends of the click path: {@link
 * Notifications.saw} writes it and {@link Notifications.start} reads it back
 * out of whatever the OS hands over, and a literal spelled twice is a door
 * that silently stops opening.
 */
const ADDRESS = "address";

/**
 * What a notification carries, as much of it as this store fills in.
 *
 * `NotificationDraft` and not `Notification`: this is a browser module, and a
 * type sharing a name with the DOM global would have the two read as one.
 */
export interface NotificationDraft {
  title: string;
  body: string;
  extra: Record<string, unknown>;
}

/** The bridge this store needs, injectable so a test needs no Tauri. */
export interface NotifyPorts {
  notificationKinds: () => Promise<InboxCategory[]>;
  setNotificationKinds: (kinds: InboxCategory[]) => Promise<InboxCategory[]>;
  /** Whether the OS has already said yes. Falsy covers *not asked yet*. */
  isPermissionGranted: () => Promise<boolean | null>;
  /** Ask the OS. `"granted"` is the only answer that is a yes. */
  requestPermission: () => Promise<string>;
  send: (notification: NotificationDraft) => void;
  /** Subscribe to notification clicks; see the module note about desktop. */
  onAction: (handler: (notification: { extra?: Record<string, unknown> }) => void) => Promise<
    () => void
  >;
  /** Whether the window is focused — `timer.svelte.ts`'s port and its rule. */
  focused: () => boolean;
  navigate: (hash: string) => void;
}

/** What the OS last said, when it has been asked this session. */
export type PermissionOutcome = "granted" | "refused";

export interface Notifications {
  /** The kinds switched on, as the backend stores them. */
  readonly kinds: InboxCategory[];
  /** True once the stored setting has been read — *unknown* is not *off*. */
  readonly loaded: boolean;
  /** What the OS answered when it was last asked, or `null`. */
  readonly permission: PermissionOutcome | null;
  /** Set when the last read or write failed, so a surface can say so. */
  readonly error: string | null;
  /** True while a write is in flight, so a checkbox cannot be double-fired. */
  readonly busy: boolean;
  /** Read the stored kinds. The shell's, once the database can answer. */
  reseed(): Promise<void>;
  /**
   * Switch one kind on or off.
   *
   * Switching the **first** kind on asks the OS, and a later one asks only if
   * the permission has since gone. Nothing is stored if the answer is no: a
   * checkbox drawn on over a refused permission is a switch that promises
   * something nothing will deliver. Switching one **off** never asks — knobas
   * has no business prompting anybody on the way out.
   */
  choose(kind: InboxCategory, on: boolean): Promise<void>;
  /** Offer the inbox's current stream. See the module note for the gates. */
  saw(items: InboxEntry[]): void;
  /** Subscribe to notification clicks. Returns the teardown. */
  start(): () => void;
}

/**
 * Where a notification's click goes.
 *
 * The kind-agnostic `#/entity/<id>` alias, which is what `InboxView`'s own
 * *Open* uses and for the reason recorded there: an item's `kind` is the
 * mirror's word and the router's kind segment is the view's.
 *
 * A credential expiry has no entity — its subject is a source — so its address
 * is the inbox itself. That is honest rather than incomplete: the item is
 * there, with its *Open in browser* and its snooze, and inventing a detail
 * address for a source would land on a 404.
 *
 * The room in the route is `DEFAULT_CTX` and is not read: `hashFor` drops it
 * for a detail address, and `router.go` parses the result against whichever
 * room the reader is standing in, so Escape still returns them there.
 */
export function addressOf(item: InboxItem): string {
  if (!item.entity_id) return hashFor({ view: "inbox", ctx: null });
  return hashFor({
    view: "room",
    ctx: DEFAULT_CTX,
    detail: { kind: null, entityId: item.entity_id },
  });
}

export function createNotifications(ports?: Partial<NotifyPorts>): Notifications {
  const io: NotifyPorts = {
    notificationKinds: () => realRead(),
    setNotificationKinds: (kinds) => realWrite(kinds),
    isPermissionGranted: () => realIsPermissionGranted(),
    requestPermission: () => realRequestPermission(),
    send: (notification) => realSendNotification(notification),
    onAction: (handler) =>
      realOnAction((notification) =>
        handler(notification as { extra?: Record<string, unknown> }),
      ).then((listener) => () => void listener.unregister()),
    focused: () => document.hasFocus(),
    navigate: (hash) => router.go(hash),
    ...ports,
  };

  const state = $state<{
    kinds: InboxCategory[];
    loaded: boolean;
    permission: PermissionOutcome | null;
    error: string | null;
    busy: boolean;
  }>({ kinds: [], loaded: false, permission: null, error: null, busy: false });

  /** Every item key this session has already had its say about. Not a rune. */
  const seen = new Set<string>();
  /** Whether the backlog has been recorded. See the module note. */
  let primed = false;
  let live = false;

  async function reseed(): Promise<void> {
    try {
      state.kinds = await io.notificationKinds();
      state.loaded = true;
      state.error = null;
    } catch (cause) {
      // Not an empty list: *off* is a claim about what is stored, and a store
      // that could not ask has not earned it. `loaded` stays false, so the
      // section offers a retry rather than five switches reading off.
      state.error = ipcErrorMessage(cause);
    }
  }

  async function choose(kind: InboxCategory, on: boolean): Promise<void> {
    if (state.busy) return;
    state.busy = true;
    try {
      // **The first kind switched on always asks**, even where the OS says it
      // has already said yes. That is the criterion's own wording (spec #272,
      // story 72) and on desktop it is the difference between asking and never
      // asking at all: `tauri-plugin-notification`'s desktop implementation
      // answers `permission_state()` with `Granted` unconditionally, so a
      // guard that only asked when the answer was no would leave
      // `requestPermission` unreached on macOS -- and with it the line that
      // tells the reader what the OS said. Later kinds ask only if the
      // permission has since gone, which is what keeps knobas from prompting
      // on every click.
      if (on && (state.kinds.length === 0 || !(await io.isPermissionGranted()))) {
        const answered = (await io.requestPermission()) === "granted";
        state.permission = answered ? "granted" : "refused";
        if (!answered) {
          state.error = null;
          return;
        }
      }
      const next = on
        ? [...state.kinds, kind]
        : state.kinds.filter((candidate) => candidate !== kind);
      // What the backend says is stored, not what the click asked for — the
      // rule `BackupSection` and `PassiveSection` both follow on this screen.
      state.kinds = await io.setNotificationKinds(next);
      state.loaded = true;
      state.error = null;
    } catch (cause) {
      state.error = ipcErrorMessage(cause);
    } finally {
      state.busy = false;
    }
  }

  function saw(items: InboxEntry[]): void {
    const fresh: InboxEntry[] = [];
    for (const entry of items) {
      const key = entry.item.key;
      // Marked seen before any gate, and deliberately: the same item can
      // arrive twice in one stream and will arrive again in the next one, and
      // an item held back by the focus gate or by a kind being off has still
      // been *seen*.
      if (seen.has(key)) continue;
      seen.add(key);
      fresh.push(entry);
    }

    if (!primed) {
      primed = true;
      return;
    }
    if (io.focused()) return;

    for (const entry of fresh) {
      if (!state.kinds.includes(entry.item.category)) continue;
      io.send({
        title: entry.item.title,
        body: entry.item.reason,
        extra: { [ADDRESS]: addressOf(entry.item) },
      });
    }
  }

  return {
    get kinds() {
      return state.kinds;
    },
    get loaded() {
      return state.loaded;
    },
    get permission() {
      return state.permission;
    },
    get error() {
      return state.error;
    },
    get busy() {
      return state.busy;
    },
    reseed,
    choose,
    saw,
    start() {
      if (live) {
        // Already subscribed. Handing back a teardown that unwinds the *first*
        // subscription would strand it if the second caller stops first — the
        // rule `inbox.start()` records.
        return () => {};
      }
      live = true;
      let off: (() => void) | null = null;

      void io
        .onAction((notification) => {
          const address = notification.extra?.[ADDRESS];
          if (live && typeof address === "string") io.navigate(address);
        })
        .then((unlisten) => {
          if (live) off = unlisten;
          else unlisten();
        })
        .catch(() => {
          // Expected on desktop: the plugin has no click channel there (see
          // the module note). A failed subscription is not a failed window —
          // notifications still fire, they simply have no door behind them.
        });

      return () => {
        live = false;
        off?.();
        off = null;
      };
    },
  };
}

/**
 * The one the window uses.
 *
 * Module-level, like `inbox` and `health`, because two trees read it: the
 * settings section's toggles and the shell's listener. Tests build their own
 * with {@link createNotifications}.
 */
export const notifications = createNotifications();
