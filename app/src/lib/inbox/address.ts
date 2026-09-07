/**
 * Where an inbox item opens — one rule, read by both ways in (issue #447).
 *
 * The inbox row's *Open* and the desktop notification's click are the same
 * promise made twice: *this item, opened*. They were two functions until the
 * sixth category arrived, and the sixth category is the one they disagree
 * about — an alert opens the Tree (spec #427 story 61) where every other
 * category opens a room detail, so a rule written once in `InboxView` left the
 * notifier sending clicks to `#/entity/<asset id>`: a slide-over over an asset,
 * which is not where the reader was told they were going. A category whose two
 * doors lead to different rooms is exactly the failure the notify store's own
 * module note names for the *setting*, one level up.
 *
 * So it is one function, and neither surface computes an address of its own.
 */
import type { InboxItem } from "../ipc/entity";
import { DEFAULT_CTX, hashFor } from "../shell/router.svelte";

/**
 * The address behind an item, as a hash the router can be handed.
 *
 * Three answers, each of them a decision:
 *
 * **An alert opens the Tree at the affected asset** — `#/asset/<id>`, story
 * 61. Its `entity_id` is the asset the monitor watches (the monitor is the
 * item's *subject*, the other half of its key), and the Tree re-opens at that
 * asset's path with the monitor in the pane. A room detail over an asset would
 * be a slide-over with no estate around it.
 *
 * **A credential expiry has no entity, so its door is the inbox itself.** Its
 * subject is a source; inventing an `#/entity/<source id>` address would land
 * on a detail `get_entity` has never heard of. That is honest rather than
 * incomplete — the item is there, with its *Open in browser* and its snooze.
 *
 * **Everything else opens the kind-agnostic `#/entity/<id>` alias.** An item's
 * `kind` is the mirror's word and the router's kind segment is the view's;
 * resolving one from the other is `get_entity`'s job and it already does it.
 *
 * Built with `hashFor` rather than a template string in every branch: entity
 * keys carry `#` and `/` (`acme/payouts#144`), and unencoded the first
 * truncates the fragment at the browser level and the second reads as another
 * path segment.
 *
 * The room in the detail route is {@link DEFAULT_CTX} and is not read:
 * `hashFor` drops the room for a detail address, and `router.go` parses the
 * result against whichever room the reader is standing in, so Escape still
 * returns them there.
 */
export function addressOf(item: InboxItem): string {
  if (!item.entity_id) return hashFor({ view: "inbox", ctx: null });
  if (item.category === "alert") {
    return hashFor({ view: "assets", tab: "tree", assetId: item.entity_id });
  }
  return hashFor({
    view: "room",
    ctx: DEFAULT_CTX,
    detail: { kind: null, entityId: item.entity_id },
  });
}
