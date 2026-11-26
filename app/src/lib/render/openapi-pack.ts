/**
 * A spec's index as the worker sends it (the ux30 end gate): its operation rows as columns, not one object each. A
 * 5,000-operation spec's rows cost the page's thread 8 ms at 4x to receive as objects, each with its own keys and its
 * own tags array; as six columns of plain values they cost a fraction, and the rows are made again here, the same.
 * Small, and free of the model, so the page's side of the port reads it without loading `openapi.ts`.
 */
import type { Opened, OpRow, SpecIndex } from "./openapi";

/** The operation rows, a column each; `tags` joined by {@link TAG_SEP}, `null` for none (`""` is one empty name). */
export interface OpColumns {
  method: string[];
  path: string[];
  summary: string[];
  operationId: (string | null)[];
  tags: (string | null)[];
  deprecated: boolean[];
}

/** What separates an operation's tags in its column: a tag's name is a YAML or JSON string, never a NUL. */
const TAG_SEP = "\u0000";

/** An open's answer as it crosses to the page: an index's rows as columns. */
export type PackedOpened = Exclude<Opened, { ok: true }> | { ok: true; index: Omit<SpecIndex, "ops">; cols: OpColumns };

/** Packs an open's answer for the page. Answers that are not an index pass as they are. */
export function packOpened(opened: Opened): PackedOpened | Opened {
  if (!opened.ok) return opened;
  const { ops, ...rest } = opened.index;
  const cols: OpColumns = { method: [], path: [], summary: [], operationId: [], tags: [], deprecated: [] };
  for (const op of ops) {
    cols.method.push(op.method);
    cols.path.push(op.path);
    cols.summary.push(op.summary);
    cols.operationId.push(op.operationId);
    cols.tags.push(op.tags.length === 0 ? null : op.tags.join(TAG_SEP));
    cols.deprecated.push(op.deprecated);
  }
  return { ok: true, index: rest, cols };
}

/** The open's answer as the model made it, from what {@link packOpened} sent. Anything else passes as it is. */
export function unpackOpened(answer: unknown): unknown {
  if (answer === null || typeof answer !== "object" || !("cols" in answer)) return answer;
  const { index, cols } = answer as { index: Omit<SpecIndex, "ops">; cols: OpColumns };
  const ops: OpRow[] = Array.from({ length: cols.method.length });
  for (let i = 0; i < ops.length; i++) {
    const method = cols.method[i]!;
    const path = cols.path[i]!;
    const tags = cols.tags[i] ?? null;
    ops[i] = {
      i,
      method,
      path,
      summary: cols.summary[i]!,
      operationId: cols.operationId[i]!,
      tags: tags === null ? [] : tags.split(TAG_SEP),
      deprecated: cols.deprecated[i]!,
      anchor: `${method} ${path}`,
    };
  }
  return { ok: true, index: { ...index, ops } } satisfies Opened;
}
