/**
 * The action items in a protocol body, and the Confluence sources a publish
 * may aim at — the pure half of the protocol panel (issue #289).
 *
 * Separate from `ProtocolPanel.svelte` for the reason `standup.ts` is separate
 * from `StandupView.svelte`: both rules here have real content, and both are
 * wrong in ways a screenshot does not show.
 *
 * ## Why the action-item rule lives on this side as well as in Rust
 *
 * It is one rule with two jobs, not two rules. `knobas_app::protocol`'s
 * `action_items` is what a *backend* reader would need; what the panel needs is
 * to draw a **button per item**, which means it has to know where the items are
 * in the text the reader is looking at right now — before any save, and while
 * they are still typing. A round trip per keystroke to be told what is on
 * screen would be the wrong shape as well as slow.
 *
 * The two are kept honest by the pair of tests that assert the same fixture
 * body produces the same items on both sides, the way `toStorage` and
 * `storage::from_text` are kept in step.
 */

/** One action item a protocol names. */
export interface ActionItem {
  /** The words after the bullet and the checkbox, trimmed. */
  text: string;
  /** Whether the box is ticked. */
  done: boolean;
}

/** The heading whose bullets are action items, matched case-insensitively. */
const HEADING = "action items";

/**
 * The action items in a protocol body.
 *
 * Everything bulleted under the **Action items** heading, until the next
 * heading of any level. A `- [ ]` or `- [x]` prefix is a checkbox and is
 * stripped; a plain bullet is an action item with no box.
 *
 * A bullet with no words is not an item — the template ships one empty bullet
 * on purpose, and a *Create ticket* button on it would file a ticket called
 * nothing.
 *
 * Nothing else in the body is interpreted. This is a reader over one section,
 * not a markdown implementation.
 */
export function actionItems(bodyMd: string): ActionItem[] {
  let inside = false;
  const out: ActionItem[] = [];
  for (const line of bodyMd.split("\n")) {
    const trimmed = line.trim();
    if (trimmed.startsWith("#")) {
      inside = trimmed.replace(/^#+/, "").trim().toLowerCase() === HEADING;
      continue;
    }
    if (!inside) continue;
    const rest = bullet(trimmed);
    if (rest === null) continue;
    const { done, text } = checkbox(rest);
    if (text === "") continue;
    out.push({ text, done });
  }
  return out;
}

/** What follows a list bullet, or `null` for a line that is not one. */
function bullet(line: string): string | null {
  for (const marker of ["- ", "* ", "+ "]) {
    if (line.startsWith(marker)) return line.slice(marker.length).trim();
  }
  return ["-", "*", "+"].includes(line) ? "" : null;
}

/** A leading `[ ]` / `[x]`, read and removed. */
function checkbox(rest: string): { done: boolean; text: string } {
  for (const [marker, done] of [
    ["[ ]", false],
    ["[x]", true],
    ["[X]", true],
  ] as const) {
    if (rest.startsWith(marker)) return { done, text: rest.slice(marker.length).trim() };
  }
  return { done: false, text: rest };
}

/** One source a protocol could be published into. */
export interface PublishableSource {
  id: string;
  name: string;
}

/**
 * The configured sources a protocol can be published into, in listing order.
 *
 * A source qualifies by **what its adapter declares**, never by its name or
 * its id: `create_page` in the descriptor's `write_ops` is the same test
 * `submit_write` applies, so a source offered here is one the write queue will
 * accept. A disabled source is not offered — its items have left the mirror,
 * so its parent pages cannot be picked either.
 *
 * The empty answer is a real one and the panel says so: no Confluence
 * configured is a different sentence from "pick one".
 */
export function publishableSources(
  sources: readonly { id: string; display_name: string; adapter_kind: string; enabled: boolean }[],
  descriptors: readonly { adapter_kind: string; write_ops: string[] }[],
): PublishableSource[] {
  const canCreate = new Set(
    descriptors
      .filter((descriptor) => descriptor.write_ops.includes("create_page"))
      .map((descriptor) => descriptor.adapter_kind),
  );
  return sources
    .filter((source) => source.enabled && canCreate.has(source.adapter_kind))
    .map((source) => ({ id: source.id, name: source.display_name }));
}

/**
 * Which source a publish should aim at without asking, or `null` when the
 * reader has to choose.
 *
 * Story 68 in one function: **one** configured Confluence needs no question,
 * and *two* have no defensible default — a publish that picked one would put
 * the team's standup in the wrong instance, which is the one mistake the story
 * exists to prevent. Zero is also `null`, and the panel's sentence for that
 * case is different again.
 */
export function presumedSource(sources: readonly PublishableSource[]): string | null {
  return sources.length === 1 ? sources[0]!.id : null;
}
