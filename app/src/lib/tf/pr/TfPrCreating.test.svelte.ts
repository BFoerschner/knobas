/**
 * The page of a pull request being created (#2584): `#/pr/new/<write id>`,
 * drawn from what was typed while the create is out, the pull request's own
 * page once its read-back names it, and the queue's answers where it won't go.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { QueuedWrite } from "../../ipc/sources";
import { echoEvents, echoFollower } from "../echo-events.fixture";
import { pageContext } from "../kept-pages.svelte";
import { noteCreate } from "./creating.svelte";
import { prHash } from "./pr";
import TfPrCreating, { type CreatingPorts } from "./TfPrCreating.svelte";

const REPO = "gitea:tidewater/payout-service";
const MADE = `${REPO}#153`;
const PR = { entity: REPO, title: "Instant SEPA spike", body: "", head: "spike/sepa-instant", base: "main" };

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
const settle = async () => {
  for (let i = 0; i < 5; i++) await Promise.resolve();
  flushSync();
};
const text = (el: Element | null | undefined) => (el?.textContent ?? "").replace(/\s+/g, " ").trim();
const button = (name: string) => [...target.querySelectorAll("button")].find((b) => text(b) === name);

function row(over: Partial<QueuedWrite> = {}): QueuedWrite {
  return {
    id: 1,
    source_id: "gitea",
    entity_id: REPO,
    op: "create_pull_request",
    payload: { CreatePullRequest: PR },
    target_snapshot: null,
    state: "pending",
    wait_reason: null,
    detail: null,
    queued_at: "2026-11-27T10:00:00Z",
    attempted_at: null,
    attempts: 0,
    held_snapshot: null,
    settled_at: null,
    source_enabled: true,
    ...over,
  };
}

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
  location.hash = "#/pr/new/1";
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

async function draw(ports: Partial<CreatingPorts> & Pick<CreatingPorts, "follower">, visible = () => true) {
  const all: CreatingPorts = {
    getEntity: async () => ({ row: { entity_id: REPO, title: "tidewater/payout-service" } }) as never,
    applyHeldWrite: async () => {},
    discardWrite: async () => {},
    ...ports,
  };
  view = mount(TfPrCreating, { target, props: { write: 1, back: null, ports: all }, context: pageContext(visible) });
  await settle();
}

test("while the create is out the page draws it as typed, and takes the pull request's address once the read-back names it (#2584)", async () => {
  const events = echoEvents(settle);
  const queue = echoFollower();
  noteCreate(1, PR, events.listen);
  await draw({ follower: queue.follower });
  queue.hear(1, row());
  await settle();
  expect(text(target.querySelector("h1"))).toBe("PR Instant SEPA spike");
  expect(text(target.querySelector(".tf-fhead .tf-sub"))).toBe("Queued spike/sepa-instant into main");
  expect(text(target.querySelector(".tf-prcreating [role=status]"))).toBe("Creating this pull request…");
  expect(text(target.querySelector(".tf-crumbs"))).toContain("tidewater/payout-service");

  queue.sent(1);
  await settle();
  expect(text(target.querySelector(".tf-fhead .tf-sub"))).toBe("Sent spike/sepa-instant into main");
  expect(location.hash).toBe("#/pr/new/1");

  await events.refreshed(MADE, 1);
  await settle();
  expect(location.hash).toBe(prHash(MADE));
});

test("a page not shown stays where it is when the read-back lands, and goes once it shows (#2584)", async () => {
  const events = echoEvents(settle);
  const queue = echoFollower();
  noteCreate(1, PR, events.listen);
  let shown = $state(false);
  await draw({ follower: queue.follower }, () => shown);
  await events.refreshed(MADE, 1);
  expect(location.hash).toBe("#/pr/new/1");
  shown = true;
  await settle();
  expect(location.hash).toBe(prHash(MADE));
});

test("a create the source refused says why and offers what the write queue offers: Discard (#2584)", async () => {
  const events = echoEvents(settle);
  const queue = echoFollower();
  noteCreate(1, PR, events.listen);
  const discarded: number[] = [];
  await draw({ follower: queue.follower, discardWrite: async (id) => void discarded.push(id) });
  queue.hear(1, row({ state: "refused", detail: "pull request already exists for these targets" }));
  await settle();
  expect(text(target.querySelector(".tf-fhead .tf-sub"))).toBe("Refused spike/sepa-instant into main");
  expect(text(target.querySelector(".tf-prcreating [role=status]"))).toBe("The source refused it: pull request already exists for these targets");
  expect(button("Send again")).toBeUndefined();
  button("Discard")!.click();
  await settle();
  expect(discarded).toEqual([1]);
  expect(text(target.querySelector(".tf-prcreating [role=status]"))).toBe("Discarded. No pull request was created.");
});

test("a held create offers Send again and Discard, as the queue does (#2584)", async () => {
  const queue = echoFollower();
  const applied: number[] = [];
  await draw({ follower: queue.follower, applyHeldWrite: async (id) => void applied.push(id) });
  // After a reload: nothing noted this window, so the page reads what was typed off the queue's row.
  queue.hear(1, row({ state: "held", detail: "the repository is gone", held_snapshot: { op: "create_pull_request", live: false } }));
  await settle();
  expect(text(target.querySelector("h1"))).toBe("PR Instant SEPA spike");
  expect(text(target.querySelector(".tf-prcreating [role=status]"))).toBe("It was held: the repository is gone");
  button("Send mine anyway")!.click();
  await settle();
  expect(applied).toEqual([1]);
  expect(button("Discard")).not.toBeUndefined();
});

test("a create waiting on its source offers Cancel, as the queue does (#2584)", async () => {
  const queue = echoFollower();
  const discarded: number[] = [];
  await draw({ follower: queue.follower, discardWrite: async (id) => void discarded.push(id) });
  queue.hear(1, row());
  await settle();
  expect(button("Cancel")).toBeUndefined();
  queue.hear(1, row({ wait_reason: "unreachable", detail: "connection timed out", attempts: 1 }));
  await settle();
  expect(text(target.querySelector(".tf-prcreating [role=status]"))).toBe("Creating this pull request… Waiting: connection timed out");
  button("Cancel")!.click();
  await settle();
  expect(discarded).toEqual([1]);
});

test("after a reload a create still queued takes the pull request's address once its read-back names it (#2584)", async () => {
  // Nothing noted this window: the page notes the open row itself, so the read-back is heard.
  const events = echoEvents(settle);
  const queue = echoFollower();
  await draw({ follower: queue.follower, listen: events.listen });
  queue.hear(1, row());
  await settle();
  expect(text(target.querySelector("h1"))).toBe("PR Instant SEPA spike");
  queue.sent(1);
  await settle();
  await events.refreshed(MADE, 1);
  await settle();
  expect(location.hash).toBe(prHash(MADE));
});
