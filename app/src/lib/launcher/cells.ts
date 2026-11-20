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
 *
 * A pull request's title starts with its number, read off its id, so `#142`
 * finds a row that says it found it (F-654): *payout-service*, then *#142
 * Idempotent retry window*. Not in the key cell, whose 118 px would then cut
 * the repository's name.
 */
import type { Excerpt, FilesFound, Segment } from "../ipc";
import { keyOf } from "../time/format";

export interface Cells {
  /** The key cell's text. */
  cell: string;
  /** Whether the cell is an identifier, drawn in the mono face. */
  mono: boolean;
  /** The title cell's text. */
  title: string;
}

/**
 * What a row is made of: the fields a search hit and a mirror row share,
 * `cell` the engine's key cell (#2452), absent where it resolved none.
 */
export interface CellSource {
  kind: string;
  entity_id: string;
  title: string;
  path: string | null;
  cell?: string | null | undefined;
}

const parts = (path: string, sep: string) =>
  path
    .split(sep)
    .map((part) => part.trim())
    .filter(Boolean);

/** The row's cells; `singular` is its kind's word, *Pull request*, drawn lower-case where nothing better is known. */
export function cellsOf(row: CellSource, singular: string): Cells {
  const cells = placeOf(row, singular);
  const number = row.kind === "pr" ? /#\d+$/.exec(row.entity_id)?.[0] : undefined;
  return number === undefined ? cells : { ...cells, title: `${number} ${cells.title}` };
}

function placeOf(row: CellSource, singular: string): Cells {
  const { kind, entity_id: entityId, title, path, cell } = row;
  const word: Cells = { cell: singular.toLowerCase(), mono: false, title };
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

/**
 * **The evidence line** (the everything-search decisions, 2026-11-20, D10): where the words are, for a row its name does
 * not explain. Drawn under the title only where the hit has an excerpt and its title no mark: a row found by its name
 * says so with its marks, and one found by its key or a stem the excerpts miss has neither and stays one line.
 */
export interface Why {
  /** Where it matched: *Description*, *Comment by Jonas Becker*, a section, *Log line 212*; `null` where the passage says it itself. */
  place: string | null;
  /** The passage, cut so its first mark sits in the line's first third. */
  segments: Segment[];
  /** Whether it is drawn in the mono face: a log's line. */
  code: boolean;
  /** What closes a file's line (*4 files*), `null` for everything else. */
  count: string | null;
  /** The excerpt it quotes, which says where the row opens. */
  excerpt: Excerpt;
}

/** What `whyOf` reads of a hit. */
export interface WhySource {
  /** Absent from an answer written before #2431. */
  excerpts?: readonly Excerpt[] | undefined;
  marks?: readonly number[] | undefined;
  /** A repository's files that hold the words (D10), where its files found it. */
  files?: FilesFound | undefined;
}

/** The longest unmarked run a passage may open with before it is cut. */
const LEAD_MOST = 18;
/** What a cut lead keeps of its end. */
const LEAD_KEPT = 16;

const wordChar = (c: string | undefined) => c !== undefined && /[\p{L}\p{N}_]/u.test(c);

/**
 * A passage's leading unmarked run of more than {@link LEAD_MOST} characters, cut to `…` and its last
 * {@link LEAD_KEPT}, from a word's start where one is in them.
 */
export function cutLead(segments: readonly Segment[]): Segment[] {
  const first = segments[0];
  if (first === undefined || first.hit) return [...segments];
  const chars = Array.from(first.text);
  if (chars.length <= LEAD_MOST) return [...segments];
  let from = chars.length - LEAD_KEPT;
  for (let i = from; i < chars.length; i++) {
    if (wordChar(chars[i]) && !wordChar(chars[i - 1])) {
      from = i;
      break;
    }
  }
  return [{ text: `…${chars.slice(from).join("")}`, hit: false }, ...segments.slice(1)];
}

/** *4 files*, *1 file name*. */
const counted = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/**
 * A repository its files found (D10): the file's name and line, then the line, both mono, then how many files hold the
 * words by their text; or, where its name alone found it, the path with the letters marked, then how many files' names.
 */
function fileWhy(excerpt: Excerpt, file: NonNullable<Excerpt["file"]>, files: FilesFound | undefined): Why {
  const name = file.path.slice(file.path.lastIndexOf("/") + 1);
  if (file.line != null) {
    const by = files?.by_text ?? 0;
    // The line's indent is the pane's to share with the lines around it; the row starts at its first letter.
    const [first, ...rest] = excerpt.segments;
    const segments = first && !first.hit ? [{ text: first.text.trimStart(), hit: false }, ...rest].filter((seg) => seg.text !== "") : [...excerpt.segments];
    return { place: `${name}:${file.line}`, segments: cutLead(segments), code: true, count: by > 0 ? counted(by, "file", "files") : null, excerpt };
  }
  const by = files?.by_name ?? 0;
  return { place: null, segments: excerpt.segments.map((seg) => ({ ...seg })), code: true, count: by > 0 ? counted(by, "file name", "file names") : null, excerpt };
}

/** A run log's line, which the line names by its number. */
const LOG_LINE = /^Log line \d+$/;

/**
 * The row's evidence line, or `null` where its name explains it or nothing is quoted. `cell` is the row's key cell: a
 * row whose quoted words all stand in it (`PAY-231` typed, its body saying it too) was found by its key, which is its own
 * reason (D9's table, row 18).
 */
export function whyOf(hit: WhySource, cell: string | null = null): Why | null {
  const excerpt = hit.excerpts?.[0];
  if (excerpt === undefined || (hit.marks?.length ?? 0) > 0) return null;
  const file = excerpt.file;
  if (file) return fileWhy(excerpt, file, hit.files);
  const key = cell?.toLowerCase() ?? "";
  const words = excerpt.segments.filter((seg) => seg.hit).map((seg) => seg.text.trim().toLowerCase());
  if (key !== "" && words.length > 0 && words.every((word) => key.includes(word))) return null;
  const place = excerpt.place ?? null;
  const code = place !== null && LOG_LINE.test(place);
  return {
    // A heading's own words say where they are.
    place: place === "Heading" ? null : place,
    segments: cutLead(excerpt.segments),
    code,
    count: null,
    excerpt,
  };
}
