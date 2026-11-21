/**
 * The sources whose adapter declares `create_page` (the ux23 Windows pass, 2026-11-20, Fable's ruling): what decides
 * whether the Docs rail draws its foot, *New page…*. The foot belongs to the rail, not to the open page, so it stands on
 * the home as on a page and the rail's scroll box never changes height on a press
 * (`docs/decisions/2026-11-20-the-docs-rail-keeps-its-foot.md`).
 *
 * Read once a session (`list_sources` and `list_adapters`, the test `publishableSources` applies) and kept, so a later
 * mount of Docs knows it before its tree draws. A failed read is forgotten, so the next mount asks again.
 */
import { listAdapters, listSources } from "../../ipc/sources";
import { publishableSources } from "../../standup/protocol";

let asked: Promise<readonly string[]> | null = null;
let known: ReadonlySet<string> | null = null;

/** The ids of the enabled sources that can create pages, read once a session. */
export function pageMakers(read: () => Promise<readonly string[]> = readPageMakers): Promise<readonly string[]> {
  asked ??= read().then(
    (ids) => {
      known = new Set(ids);
      return ids;
    },
    (cause: unknown) => {
      asked = null;
      throw cause;
    },
  );
  return asked;
}

/** What the session's read answered, or `null` before it has. */
export function knownPageMakers(): ReadonlySet<string> | null {
  return known;
}

async function readPageMakers(): Promise<readonly string[]> {
  const [sources, descriptors] = await Promise.all([listSources(), listAdapters()]);
  return publishableSources(sources, descriptors).map((source) => source.id);
}

/** Tests only: forget the session's read. */
export function forgetPageMakers(): void {
  asked = null;
  known = null;
}
