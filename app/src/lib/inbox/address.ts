/**
 * Where an inbox item opens — one rule, read by both ways in (issue #447).
 *
 * The inbox row's link and the desktop notification's click are the same
 * promise made twice: *this item, opened*. Since #1085 the notification uses
 * the ticket-first inbox's {@link opener}: a review request opens its pull
 * request, a ticket its job's conversation, a credential expiry its source's
 * page in Settings. Before #1085 the notifier still built the rooms UI's
 * `#/entity/<id>`, which the new shell answered with *Nothing lives at …*.
 *
 * **An alert opens the asset tree at the affected asset** (spec #427 story
 * 61), `#/asset/<id>` in Dependencies. Its `entity_id` is the asset the
 * monitor watches, and that address is more precise than the row's
 * *Open monitor* (`#/monitors`); both are ticket-first pages.
 *
 * An item with nothing to open (no entity, and no page of its own) opens the
 * inbox itself, where it is listed with its *Open in browser* and its snooze.
 */
import type { InboxItem } from "../ipc/entity";
import { entityAddress } from "../tf/address";
import { opener } from "../tf/inbox/inbox";

/** The address behind an item, as a hash the shell can be handed. */
export function addressOf(item: InboxItem): string {
  if (item.category === "alert" && item.entity_id) return entityAddress("asset", item.entity_id);
  return opener(item)?.href ?? "#/inbox";
}
