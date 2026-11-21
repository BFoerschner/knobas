/**
 * The sites' store (ADR-0050, decisions §12, §14, §16): what the shell knows about each source's site, held so a press
 * is answered in its own task with no read.
 *
 * - **settings**: each source's *Open its pages inside knobas*, base, own and allowed origins (`site_settings`), filled
 *   once the database answers and again after each write in Settings;
 * - **status**: each live site's state, from `site:status`, dropped on `site:closed`;
 * - **names**: each source's display name, from the source list the status bar reads;
 * - **asking**: the question the bar asks in its row (decisions §10), per source;
 * - **said**: the bar's line about what happened to a link, for 6 seconds (decisions §16);
 * - **armed**: until when the first Esc's cap is drawn down.
 *
 * Every text here came from a remote system or a person, and the page draws it as text.
 */
import type { SiteSetting, SiteStatus } from "../../ipc/site";

/** What the bar asks, per source, when the page holds changes it has not saved. */
export type SiteQuestion =
  | { kind: "close" }
  | { kind: "reload" }
  /** Another address, from *Open in ⟨source⟩* elsewhere: `name` is what the press named, when it named something. */
  | { kind: "address"; url: string; name: string | null };

/** A line the bar says where the address is, for {@link SAID_MS}. */
export interface SiteSaid {
  text: string;
  /** Code-face text after the sentence: a file's name, a host. */
  mono?: string;
  action?: { label: string; run: () => void };
  /** When it goes, on the `performance.now()` clock. */
  until: number;
}

/** How long a said line stands. */
export const SAID_MS = 6000;

export interface SiteStore {
  readonly settings: ReadonlyMap<string, SiteSetting>;
  readonly status: ReadonlyMap<string, SiteStatus>;
  readonly names: ReadonlyMap<string, string>;
  readonly asking: ReadonlyMap<string, SiteQuestion>;
  readonly said: ReadonlyMap<string, SiteSaid>;
  /** Per source, until when the first Esc's cap is down, on the `performance.now()` clock. */
  readonly armed: ReadonlyMap<string, number>;
  /** Whether the settings have been read. */
  readonly loaded: boolean;
  hold(settings: readonly SiteSetting[]): void;
  holdOne(setting: SiteSetting): void;
  nameAll(names: readonly { id: string; display_name: string }[]): void;
  /** A source's name, its id while none is known. */
  name(source: string): string;
  update(status: SiteStatus): void;
  closed(source: string): void;
  ask(source: string, question: SiteQuestion | null): void;
  say(source: string, said: Omit<SiteSaid, "until"> | null, now?: number): void;
  arm(source: string, ms: number, now?: number): void;
  disarm(source: string): void;
  /** Tests only. */
  reset(): void;
}

export function createSiteStore(): SiteStore {
  let settings = $state.raw(new Map<string, SiteSetting>());
  let status = $state.raw(new Map<string, SiteStatus>());
  let names = $state.raw(new Map<string, string>());
  let asking = $state.raw(new Map<string, SiteQuestion>());
  let said = $state.raw(new Map<string, SiteSaid>());
  let armed = $state.raw(new Map<string, number>());
  let loaded = $state(false);
  const timers = new Map<string, ReturnType<typeof setTimeout>>();
  const armTimers = new Map<string, ReturnType<typeof setTimeout>>();

  const set = <V>(map: ReadonlyMap<string, V>, key: string, value: V | null): Map<string, V> => {
    const next = new Map(map);
    if (value === null) next.delete(key);
    else next.set(key, value);
    return next;
  };

  return {
    get settings() {
      return settings;
    },
    get status() {
      return status;
    },
    get names() {
      return names;
    },
    get asking() {
      return asking;
    },
    get said() {
      return said;
    },
    get armed() {
      return armed;
    },
    get loaded() {
      return loaded;
    },
    hold(list) {
      settings = new Map(list.map((one) => [one.source, one]));
      loaded = true;
    },
    holdOne(setting) {
      settings = set(settings, setting.source, setting);
    },
    nameAll(list) {
      names = new Map(list.map((one) => [one.id, one.display_name]));
    },
    name(source) {
      return names.get(source) ?? source;
    },
    update(next) {
      const was = status.get(next.source);
      if (was !== undefined && JSON.stringify(was) === JSON.stringify(next)) return;
      status = set(status, next.source, next);
    },
    closed(source) {
      status = set(status, source, null);
      asking = set(asking, source, null);
      armed = set(armed, source, null);
    },
    ask(source, question) {
      asking = set(asking, source, question);
    },
    say(source, line, now = performance.now()) {
      clearTimeout(timers.get(source));
      timers.delete(source);
      if (line === null) {
        said = set(said, source, null);
        return;
      }
      said = set(said, source, { ...line, until: now + SAID_MS });
      timers.set(
        source,
        setTimeout(() => {
          timers.delete(source);
          said = set(said, source, null);
        }, SAID_MS),
      );
    },
    arm(source, ms, now = performance.now()) {
      clearTimeout(armTimers.get(source));
      armed = set(armed, source, now + ms);
      armTimers.set(
        source,
        setTimeout(() => {
          armTimers.delete(source);
          armed = set(armed, source, null);
        }, ms),
      );
    },
    disarm(source) {
      clearTimeout(armTimers.get(source));
      armTimers.delete(source);
      armed = set(armed, source, null);
    },
    reset() {
      for (const timer of [...timers.values(), ...armTimers.values()]) clearTimeout(timer);
      timers.clear();
      armTimers.clear();
      settings = new Map();
      status = new Map();
      names = new Map();
      asking = new Map();
      said = new Map();
      armed = new Map();
      loaded = false;
    },
  };
}

/** The window's one store. */
export const siteStore = createSiteStore();

/** The address of a source's site page. */
export function siteHash(source: string): string {
  return `#/site/${encodeURIComponent(source)}`;
}
