/**
 * PROTOTYPE: the key column of a row (rounds 9 and 10): something a person
 * reads. A ticket's key; otherwise where the thing sits (a pull request's
 * repository, a repository's owner, a page's space, a pipeline's project, a
 * monitor's type); an inbox line's time; and the kind's word when nothing
 * better is known. A repository's own name moves to the title, so the owner
 * is not said twice.
 */
import { keyOf } from "../../time/format";

export interface Cells {
  cell: string;
  /** Whether the cell is an identifier, drawn in the mono face. */
  mono: boolean;
  title: string;
}

const last = (path: string, sep: RegExp) => path.split(sep).map((part) => part.trim()).filter(Boolean).at(-1) ?? path;
const first = (path: string, sep: RegExp) => path.split(sep).map((part) => part.trim()).filter(Boolean)[0] ?? path;

export function cellOf(kind: string, entityId: string, title: string, path: string | null, kindWord: string): Cells {
  const word = kindWord.toLowerCase();
  switch (kind) {
    case "ticket":
      return { cell: keyOf(entityId), mono: true, title };
    case "pr":
    case "branch":
      return { cell: path ? last(path, /\//) : word, mono: false, title };
    case "repo": {
      const slash = title.indexOf("/");
      return slash > 0 ? { cell: title.slice(0, slash), mono: false, title: title.slice(slash + 1) } : { cell: word, mono: false, title };
    }
    case "monitor":
      return { cell: path ?? word, mono: false, title };
    case "pipeline":
      return { cell: path ? last(path, /›/) : word, mono: false, title };
    case "page":
      return { cell: path ? first(path, /›/) : word, mono: false, title };
    case "view":
      return { cell: path ? first(path, /·/) : word, mono: false, title };
    case "inbox":
      return { cell: keyOf(entityId).replace(/ \(\d+\)$/, ""), mono: true, title };
    default:
      return { cell: word, mono: false, title };
  }
}
