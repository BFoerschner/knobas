/**
 * A launcher row's key cell (#2421, spec #2419): something a person reads.
 *
 * A ticket's key, in the mono face; otherwise where the thing sits: a pull
 * request's repository, a repository's owner, a page's space, a pipeline's
 * project, a monitor's type, a view's group; the kind's word when nothing
 * better is known. A repository's own name moves to the title, so the owner
 * is not said twice.
 *
 * The engine resolves it from the kind's declarations and sends it as the
 * row's `cell` (#2452), the same text *Titles* matches (#2451); a row that
 * carries one draws it, whatever its kind, save a ticket (its key) and a
 * repository (its owner, out of its title). A row without one is read off
 * its `path`, the one line `knobas_core::path_line_read!` joins (` › `
 * between ancestors), as a view's is. A kind whose path misses falls back to
 * its word, as ADR-0007's miss fails toward a row that says less, not
 * something wrong.
 */
import { keyOf } from "../time/format";

export interface Cells {
  /** The key cell's text. */
  cell: string;
  /** Whether the cell is an identifier, drawn in the mono face. */
  mono: boolean;
  /** The title cell's text. */
  title: string;
}

/** What a row is made of: the fields a search hit and a mirror row share. */
export interface CellSource {
  kind: string;
  entityId: string;
  title: string;
  path: string | null;
  /** The engine's key cell (#2452), absent where it resolved none. */
  cell?: string | undefined;
  /** The kind's word, singular: *Pull request*. Drawn lower-case. */
  kindWord: string;
}

const parts = (path: string, sep: string) =>
  path
    .split(sep)
    .map((part) => part.trim())
    .filter(Boolean);

export function cellsOf({ kind, entityId, title, path, cell, kindWord }: CellSource): Cells {
  const word: Cells = { cell: kindWord.toLowerCase(), mono: false, title };
  const sent = cell?.trim();
  if (sent && kind !== "ticket" && kind !== "repo") return { cell: sent, mono: false, title };
  const at = (pick: (all: string[]) => string | undefined, sep: string): Cells => {
    const found = path ? pick(parts(path, sep)) : undefined;
    return found ? { cell: found, mono: false, title } : word;
  };
  switch (kind) {
    case "ticket":
      return { cell: keyOf(entityId), mono: true, title };
    // The repository is the parent's last segment: `tidewater/payout-service`.
    case "pr":
    case "branch":
    case "commit":
      return at((all) => all.at(-1), "/");
    case "repo": {
      const slash = title.indexOf("/");
      return slash > 0 ? { cell: title.slice(0, slash), mono: false, title: title.slice(slash + 1) } : word;
    }
    // The nearest project.
    case "pipeline":
    case "build_config":
      return at((all) => all.at(-1), "›");
    // The page's space: the top of its ancestor chain.
    case "page":
      return at((all) => all[0], "›");
    // A view's path line is its group, then its conditions.
    case "view":
      return at((all) => all[0], "·");
    case "monitor":
      return at((all) => all[0], "›");
    default:
      return word;
  }
}
