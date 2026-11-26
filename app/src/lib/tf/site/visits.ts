/**
 * After a visit (ADR-0050, decisions §15): the mirrored items whose pages a site showed are the ones the person stood
 * on. While the site is shown, each address it stands at is asked of the mirror (`resolve_url`, one read of the local
 * mirror, never a request to a source), the answer kept per address for the session; when the site's page stops being
 * shown, or the site closes, the last {@link REFRESH_LAST} of them are read again with the one-item refresh, one after
 * another, and the list is forgotten. knobas cannot know whether anything changed there; refreshing what was looked at
 * is cheap, and right every time something did.
 */
import { resolveUrl, type UrlMatch } from "../../ipc/entity";
import { refreshEntity } from "../../ipc/sources";

/** At most this many items are refreshed after a visit: they go out as a person's reads. */
export const REFRESH_LAST = 5;

export interface VisitPorts {
  resolve: (url: string) => Promise<UrlMatch | null>;
  refresh: (entityId: string) => Promise<unknown>;
}

export interface Visits {
  /** What the mirror said `url` names: a match, `null` for nothing, `undefined` while unasked or pending. */
  answer(url: string): UrlMatch | null | undefined;
  /** The site of `source` stands at `url` while shown: ask the mirror, once per address, and note what it names. */
  see(source: string, url: string): Promise<UrlMatch | null>;
  /**
   * The site of `source` stopped being shown: refresh the last items it stood on, in the order visited, and `first`, the
   * page `Esc` twice landed on, before them (Fable's ruling, 2026-11-25), so the page drawn now is the one read first.
   */
  leave(source: string, first?: string | null): Promise<void>;
  /** Tests only. */
  reset(): void;
}

export function createVisits(ports: VisitPorts, changed: () => void = () => {}): Visits {
  const answers = new Map<string, UrlMatch | null>();
  const pending = new Map<string, Promise<UrlMatch | null>>();
  const visited = new Map<string, string[]>();

  const note = (source: string, entityId: string) => {
    const list = (visited.get(source) ?? []).filter((id) => id !== entityId);
    list.push(entityId);
    visited.set(source, list);
  };

  return {
    answer: (url) => answers.get(url),
    async see(source, url) {
      const known = answers.get(url);
      if (known !== undefined) {
        if (known !== null) note(source, known.entity_id);
        return known;
      }
      let asking = pending.get(url);
      if (asking === undefined) {
        // A read that throws or rejects names nothing.
        asking = Promise.resolve()
          .then(() => ports.resolve(url))
          .catch(() => null);
        pending.set(url, asking);
      }
      const match = await asking;
      pending.delete(url);
      answers.set(url, match);
      changed();
      if (match !== null) note(source, match.entity_id);
      return match;
    },
    async leave(source, first = null) {
      const list = visited.get(source) ?? [];
      visited.delete(source);
      const last = list.slice(-REFRESH_LAST);
      const ordered = first !== null && last.includes(first) ? [first, ...last.filter((id) => id !== first)] : last;
      for (const id of ordered) {
        try {
          await Promise.resolve().then(() => ports.refresh(id));
        } catch {
          // A refresh that fails does not stop the next; the next run brings it.
        }
      }
    },
    reset() {
      answers.clear();
      pending.clear();
      visited.clear();
    },
  };
}

/** The window's one record, over the bridge. */
export const siteVisits = createVisits({ resolve: (url) => resolveUrl(url), refresh: (id) => refreshEntity(id) });
