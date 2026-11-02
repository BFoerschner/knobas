/**
 * The job's conversation (#819) and its Connections drawer (#820), mounted
 * with stand-ins for every read and write. What the backend does with the two
 * conversation reads is `crates/knobas-app/tests/it/conversation_ipc.rs`'s.
 *
 * The suite runs in Europe/Berlin, so the visit at 07:00Z reads 09:00.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { EntityDetail, LinkEntry, SuggestionEntry, Thread } from "../../ipc/entity";
import type { WriteOpPayload } from "../../ipc/sources";
import type { Hear, WriteFollower } from "../../shell/follow-write";
import { toasts } from "../../shell/toasts.svelte";
import { forgetEchoes } from "../queued.svelte";
import { page as codeRowPage } from "../code-row/page";
import { createLayers, type Layers } from "../layers.svelte";
import { pageFor } from "../pages";
import { parseTfHash } from "../route";
import type { JobPorts } from "./ports";
import TfJob from "./TfJob.svelte";
import { sourceFacts } from "./source-facts.fixture";

const ID = "jira:PAY-231";
const VISIT = "2026-10-15T07:00:00Z";

let target: HTMLElement;
let job: ReturnType<typeof mount> | null = null;
const calls = {
  writes: [] as WriteOpPayload[],
  unlinks: [] as string[],
  links: [] as [string, string, string | undefined, string | undefined][],
  reads: 0,
};

beforeEach(() => {
  calls.writes.length = 0;
  calls.unlinks.length = 0;
  calls.links.length = 0;
  calls.reads = 0;
  toasts.items.length = 0;
  target = document.createElement("div");
  document.body.appendChild(target);
});

/** What a page queued is held for the session (#1424), and each test is one. */
afterEach(() => {
  forgetEchoes();
  if (job) unmount(job);
  job = null;
  target.remove();
});

async function settle() {
  for (let turn = 0; turn < 8; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

function link(id: string, otherId: string, kind: string, title: string): LinkEntry {
  return {
    link: {
      id,
      from_id: otherId,
      to_id: ID,
      relation: "implements",
      origin: "manual",
      note: "why",
      created_by: "mara",
      created_at: "2026-10-01T10:00:00Z",
      confirmed_at: "2026-10-01T10:00:00Z",
      rule: null,
      rule_class: null,
      reason: null,
    },
    other: { entity_id: otherId, kind, title, deleted_at: null },
  };
}

const PR = link("link:1", "gitea:tidewater/payout-service#142", "pr", "Idempotent retry window");
const PAGE = link("link:2", "confluence:ENG-SEPA", "page", "SEPA payout retry design");

function ticket(links: LinkEntry[] = [PR, PAGE]): EntityDetail {
  return {
    row: {
      entity_id: ID,
      kind: "ticket",
      source_id: "jira",
      title: "Retry failed SEPA payouts",
      updated_at: null,
      synced_at: "2026-10-15T09:00:00Z",
      path: null,
      state: null,
    },
    source: { id: "jira", display_name: "Jira", adapter_kind: "jira", enabled: true },
    kind_info: null,
    refreshes: true,
    body_text: "",
    author: null,
    payload: {},
    web_url: null,
    deleted_at: null,
    links,
    activity: [],
    builds: [],
    status: "In Progress",
    issue_type: null,
    priority: null,
    estimate_seconds: null,
    description: null,
    markup: "text",
  };
}

/** Eight old comments, the reader's own after the visit, and two new ones, one naming the reader. */
function thread(): Thread {
  const old = Array.from({ length: 8 }, (_, n) => ({
    author: n % 2 ? "Priya Nair" : "Lena Hoffmann",
    at: `2026-08-${12 + n}T08:00:00Z`,
    body: `old comment ${n}`,
    mine: false,
    mentions_me: false,
    markup: "text" as const,
  }));
  return {
    comments: [
      ...old,
      { author: "Mara Lindqvist", at: "2026-10-15T07:30:00Z", body: "Starting.", mine: true, mentions_me: false, markup: "text" as const },
      { author: "Jonas Becker", at: "2026-10-15T08:00:00Z", body: "Left a comment on the PR.", mine: false, mentions_me: false, markup: "text" as const },
      { author: "Lena Hoffmann", at: "2026-10-15T10:45:00Z", body: "[~mara] will the queue get an SLA?", mine: false, mentions_me: true, markup: "text" as const },
    ],
    me: ["mara"],
  };
}

const PROPOSAL: SuggestionEntry = {
  link: { ...PR.link, id: "link:p1", from_id: "teamcity:Payout_Build", origin: "suggested", confirmed_at: null },
  from: { entity_id: "teamcity:Payout_Build", kind: "build", title: "Payout build", deleted_at: null },
  to: { entity_id: ID, kind: "ticket", title: "Retry failed SEPA payouts", deleted_at: null },
};
const ELSEWHERE: SuggestionEntry = {
  ...PROPOSAL,
  link: { ...PROPOSAL.link, id: "link:p2", to_id: "jira:PAY-228" },
};

function ports(over: Partial<JobPorts> = {}): JobPorts {
  let links = [PR, PAGE];
  return {
    listSources: () => Promise.resolve([sourceFacts("jira")]),
    getEntity: (id) => {
      calls.reads += 1;
      return id === ID ? Promise.resolve(ticket(links)) : Promise.reject({ code: "not_found", message: "", source_id: null });
    },
    promoteContext: (id) => Promise.resolve({ id: "ctx:1", kind: "ticket", title: "", anchor_id: id, created_at: "", archived_at: null }),
    listEntities: () => Promise.resolve({ rows: [], total: 0, tally: null, groups: null }),
    submitWrite: (payload) => {
      calls.writes.push(payload);
      return Promise.resolve({} as never);
    },
    visitEntity: () => Promise.resolve(VISIT),
    entityThread: () => Promise.resolve(thread()),
    createLink: (from, to, relation, note) => {
      calls.links.push([from, to, relation, note]);
      links = [PR, PAGE];
      return Promise.resolve({} as never);
    },
    unlink: (id) => {
      calls.unlinks.push(id);
      links = links.filter((entry) => entry.link.id !== id);
      return Promise.resolve();
    },
    roomSuggestions: () => Promise.resolve({ rows: [PROPOSAL, ELSEWHERE], total: 2, more: [] }),
    search: () => Promise.reject(new Error("not in this test")),
    launcherHome: () => Promise.resolve({ recent: [], smart_lists: [], stale_sources: [] } as never),
    jobReach: () => Promise.resolve([]),
    jobLinkSets: () => Promise.resolve({ sets: [], parent: null }),
    hideLinkSetMember: (_ticket, _member, hidden) => Promise.resolve(hidden),
    getAsset: () => Promise.reject(new Error("not in this test")),
    startWork: {
      flow: () => Promise.resolve([]),
      run: () => Promise.resolve([]),
      retry: () => Promise.resolve([]),
      skip: () => Promise.resolve([]),
      amend: () => Promise.resolve([]),
    },
    ...over,
  };
}

function open(over: { ports?: JobPorts; layers?: Layers; follower?: WriteFollower } = {}) {
  job = mount(TfJob, {
    target,
    props: {
      segment: "PAY-231",
      timer: { current: null, elapsed: null },
      layers: over.layers ?? createLayers(),
      ontimer: () => {},
      onstarttimer: () => {},
      ports: over.ports ?? ports(),
      today: () => new Date(2026, 9, 15, 14, 0),
      ...(over.follower ? { follower: over.follower } : {}),
    },
  });
  flushSync();
}

function squash(node: Element | null | undefined): string {
  return node?.textContent?.replace(/\s+/g, " ").trim() ?? "";
}

/** The drawer, opened as the page opens it: `L` (its row on the card comes with #2401). */
function drawerOpen() {
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "l", bubbles: true, cancelable: true }));
  flushSync();
  return target.querySelector("[aria-label='Connections']")!;
}

function messages(): string[] {
  return [...target.querySelectorAll(".tf-convo .tf-msg p")].map((p) => squash(p));
}

// ---- the conversation in the reading column (R-5.2 to R-5.4, #2399) ----

test("the thread is drawn whole with no fold and no filter, marks the last visit, edges new comments and mentions", async () => {
  open();
  await settle();
  expect(target.querySelector(".tf-fold"), "no fold").toBeNull();
  expect(target.querySelector(".tf-convo-bar, .tf-fchips"), "no filter bar").toBeNull();
  expect(messages()).toHaveLength(11);
  expect(squash(target.querySelector(".tf-newline"))).toBe("New since your last visit, 09:00");
  // The rule sits right above the first new comment, not above the reader's own.
  expect(target.querySelector(".tf-newline + .tf-msg p")?.textContent).toBe("Left a comment on the PR.");

  const fresh = [...target.querySelectorAll(".tf-msg.tf-new p")].map((p) => squash(p));
  expect(fresh).toEqual(["Left a comment on the PR.", "[~mara] will the queue get an SLA?"]);
  const mention = target.querySelector(".tf-msg.tf-mention")!;
  expect(squash(mention.querySelector(".tf-chip"))).toBe("mentions you");
  expect(squash(mention.querySelector(".tf-at"))).toBe("[~mara]");
  expect(squash(target.querySelector(".tf-msg .tf-who b"))).not.toBe("");
  expect([...target.querySelectorAll(".tf-who b")].map((b) => b.textContent)).toContain("You");

  // The conversation is the comments (#2163): every entry drawn is one.
  const drawnEntries = [...target.querySelectorAll(".tf-convo > .tf-msg")];
  expect(drawnEntries).toHaveLength(11);
  expect(drawnEntries.every((entry) => entry.tagName === "ARTICLE")).toBe(true);
});

test("Comment queues a Comment write and clears the box; a refused one keeps the text", async () => {
  open();
  await settle();
  const box = target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Comment on PAY-231']")!;
  const send = [...target.querySelectorAll<HTMLButtonElement>(".tf-compose button")].find((b) => squash(b) === "Comment")!;
  expect(send.disabled).toBe(true);
  box.value = "Priya, can you decide the SLA?";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  send.click();
  await settle();
  expect(calls.writes).toEqual([{ Comment: { entity: ID, body: "Priya, can you decide the SLA?" } }]);
  expect(box.value).toBe("");
  expect(toasts.items.map((t) => t.text)).toContain("Queued: comment on PAY-231.");

  job && unmount(job);
  job = null;
  open({ ports: ports({ submitWrite: () => Promise.reject({ code: "invalid", message: "no", source_id: null }) }) });
  await settle();
  const again = target.querySelector<HTMLTextAreaElement>("textarea")!;
  again.value = "kept";
  again.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  [...target.querySelectorAll<HTMLButtonElement>(".tf-compose button")][0]!.click();
  await settle();
  expect(again.value).toBe("kept");
});

test("a comment the source's credential cannot make draws Comment disabled saying why, and queues nothing (#2052)", async () => {
  const reason = "This Jira user cannot comment in PAY: it lacks the Add Comments permission";
  open({ ports: ports({ refusedWrites: () => Promise.resolve([{ entity: ID, op: "comment", reason }]) }) });
  await settle();
  const box = target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Comment on PAY-231']")!;
  box.value = "Priya, can you decide the SLA?";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  const send = [...target.querySelectorAll<HTMLButtonElement>(".tf-compose button")].find((b) => squash(b) === "Comment")!;
  expect(send.getAttribute("aria-disabled")).toBe("true");
  expect(send.title).toBe(reason);
  expect(squash(target.querySelector(".tf-compose"))).toContain(reason);
  send.click();
  await settle();
  expect(calls.writes).toEqual([]);
  expect(box.value).toBe("Priya, can you decide the SLA?");
});

// ---- a queued comment is drawn at once (#1424) ----

/** The queued comments at the end of the thread, as read. */
function queuedShown(): string[] {
  return [...target.querySelectorAll(".tf-convo .tf-msg.tf-queued")].map((m) => squash(m));
}

async function comment(words: string) {
  const box = target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Comment on PAY-231']")!;
  box.value = words;
  box.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  [...target.querySelectorAll<HTMLButtonElement>(".tf-compose button")].find((b) => squash(b) === "Comment")!.click();
  await settle();
}

test("a sent comment is drawn at the end of the thread marked queued, and goes once the conversation holds it", async () => {
  open();
  await settle();
  await comment("Looking at the jitter now");
  expect(queuedShown()).toEqual(["ML Youqueued Looking at the jitter now"]);
  expect(target.querySelector(".tf-convo .tf-msg.tf-queued + .tf-compose, .tf-convo .tf-msg.tf-queued ~ .tf-compose")).not.toBeNull();

  // Opened again before the sync: still on its way.
  unmount(job!);
  open();
  await settle();
  expect(queuedShown()).toEqual(["ML Youqueued Looking at the jitter now"]);

  // The sync brought it back.
  unmount(job!);
  const synced = thread();
  synced.comments.push({ author: "Mara Lindqvist", at: "2026-10-15T13:00:00Z", body: "Looking at the jitter\nnow", mine: true, mentions_me: false, markup: "text" });
  open({ ports: ports({ entityThread: () => Promise.resolve(synced) }) });
  await settle();
  expect(queuedShown()).toEqual([]);
  expect(messages().at(-1)).toBe("Looking at the jitter now");
});

test("a comment the queue holds drops its echo and toasts; Edit and resend puts the words back in the box", async () => {
  const hears = new Map<number, Hear>();
  const follower: WriteFollower = {
    follow(id, hear) {
      hears.set(id, hear);
      return () => hears.delete(id);
    },
  };
  const numbered = ports({
    submitWrite: (payload) => {
      calls.writes.push(payload);
      return Promise.resolve({ id: 77 } as never);
    },
  });
  open({ ports: numbered, follower });
  await settle();
  await comment("Looking at the jitter now");
  expect(queuedShown()).toHaveLength(1);

  hears.get(77)!({ id: 77, state: "held", detail: "Jira sign-in is failing" } as never);
  flushSync();
  expect(queuedShown()).toEqual([]);
  const toast = toasts.items.at(-1)!;
  expect(toast.text).toBe("Comment on PAY-231 was held: Jira sign-in is failing");
  const box = target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Comment on PAY-231']")!;
  expect(box.value).toBe("");
  toast.more!.find((action) => action.label === "Edit and resend")!.run();
  await settle();
  expect(location.hash).toBe("#/ticket/jira%3APAY-231");
  expect(box.value).toBe("Looking at the jitter now");
});

// ---- the Connections drawer (R-4.3, R-6.1, R-6.2) ----

test("the drawer lists the ticket's links and its own proposals; the page draws no connected count (#2399)", async () => {
  const layers = createLayers();
  open({ layers });
  await settle();
  expect(target.querySelector(".tf-conn"), "no Connections count button").toBeNull();
  const drawer = drawerOpen();
  expect(squash(drawer.querySelector("h2"))).toBe("Connections of PAY-231");
  expect(squash(drawer.querySelector("h3"))).toBe("Linked on PAY-231 2");
  expect([...drawer.querySelectorAll(".tf-lk")].map((row) => squash(row))).toEqual([
    "Idempotent retry window Code Unlink",
    "SEPA payout retry design Pages Unlink",
  ]);
  expect(squash(drawer.querySelector(".tf-proposed"))).toBe("1 proposed for PAY-231 Judge them one by one");
  expect(drawer.querySelector(".tf-proposed a")?.getAttribute("href")).toBe("#/review/link%3Ap1");
  expect(squash(drawer)).toContain("Links and link sets stay in knobas and are never written to a source.");
  expect(document.activeElement).toBe(drawer.querySelector("input[aria-label='Filter connections']"));

  // `Esc` is the shell's: it closes the drawer layer.
  expect(layers.open.map((layer) => layer.kind)).toEqual(["drawer"]);
  layers.open[0]!.close();
  flushSync();
  expect(target.querySelector("[aria-label='Connections']")).toBeNull();
  expect(layers.open).toHaveLength(0);
});

test("L opens the drawer, outside a text field; a click on the scrim closes it", async () => {
  open();
  await settle();
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "l", bubbles: true }));
  flushSync();
  expect(target.querySelector("[aria-label='Connections']")).not.toBeNull();
  target.querySelector<HTMLElement>(".tf-scrim")!.click();
  flushSync();
  expect(target.querySelector("[aria-label='Connections']")).toBeNull();

  const field = document.createElement("input");
  document.body.appendChild(field);
  field.focus();
  field.dispatchEvent(new KeyboardEvent("keydown", { key: "l", bubbles: true }));
  flushSync();
  expect(target.querySelector("[aria-label='Connections']")).toBeNull();
  field.remove();
});

test("the drawer's filter keeps the rows it matches", async () => {
  open();
  await settle();
  drawerOpen();
  const input = target.querySelector<HTMLInputElement>("input[aria-label='Filter connections']")!;
  input.value = "pages";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  expect([...target.querySelectorAll(".tf-lk a")].map((a) => squash(a))).toEqual(["SEPA payout retry design"]);
});

test("a linked branch or commit opens its own address, which the repository page's redirect draws (#1440)", async () => {
  const branch = link("link:b", "gitea:tidewater/payout-service@refs/heads/feat/x", "branch", "feat/x");
  const commit = link("link:c", "gitea:tidewater/payout-service@9f2c1ab", "commit", "payout: key the retry");
  open({
    ports: ports({
      getEntity: (id) => (id === ID ? Promise.resolve(ticket([branch, commit])) : Promise.reject({ code: "not_found", message: "", source_id: null })),
    }),
  });
  await settle();
  drawerOpen();
  const hrefs = [...target.querySelectorAll(".tf-lk a")].map((a) => a.getAttribute("href")!);
  expect(hrefs).toEqual([
    "#/branch/gitea%3Atidewater%2Fpayout-service%40refs%2Fheads%2Ffeat%2Fx",
    "#/commit/gitea%3Atidewater%2Fpayout-service%409f2c1ab",
  ]);
  for (const href of hrefs) expect(pageFor(parseTfHash(href)), href).toBe(codeRowPage);
});

test("Unlink withdraws the link in knobas only and Undo draws it again", async () => {
  open();
  await settle();
  drawerOpen();
  target.querySelector<HTMLButtonElement>("button[aria-label='Unlink Idempotent retry window']")!.click();
  await settle();
  expect(calls.unlinks).toEqual(["link:1"]);
  expect([...target.querySelectorAll(".tf-lk a")].map((a) => squash(a))).toEqual(["SEPA payout retry design"]);
  const toast = toasts.items.find((t) => t.text === "Unlinked in knobas only.")!;
  expect(toast.action?.label).toBe("Undo");

  toast.action!.run();
  await settle();
  expect(calls.links).toEqual([[PR.link.from_id, ID, "implements", "why"]]);
  expect([...target.querySelectorAll(".tf-lk a")]).toHaveLength(2);
});

test("Link to… opens a dialog over the drawer that says the link stays in knobas", async () => {
  const layers = createLayers();
  open({ layers });
  await settle();
  drawerOpen();
  [...target.querySelectorAll<HTMLButtonElement>(".tf-dacts button")].find((b) => squash(b) === "Link to…")!.click();
  flushSync();
  const dialog = target.querySelector("[role='dialog']")!;
  expect(squash(dialog.querySelector("h2"))).toBe("Link PAY-231 to…");
  expect(squash(dialog)).toContain("Links stay in knobas and are never written to a source.");
  expect(layers.open.map((layer) => layer.kind)).toEqual(["drawer", "dialog"]);
  // `Esc` closes the dialog first and leaves the drawer open.
  layers.open[1]!.close();
  flushSync();
  expect(target.querySelector("[role='dialog']")).toBeNull();
  expect(target.querySelector("[aria-label='Connections']")).not.toBeNull();
});

test("a ticket with nothing linked says so, and no proposals draw no box", async () => {
  open({
    ports: ports({
      getEntity: () => Promise.resolve(ticket([])),
      roomSuggestions: () => Promise.resolve({ rows: [ELSEWHERE], total: 1, more: [] }),
    }),
  });
  await settle();
  drawerOpen();
  expect(squash(target.querySelector(".tf-dbody"))).toContain("Nothing linked on the ticket itself.");
  expect(target.querySelector(".tf-proposed")).toBeNull();
});

// ---- link sets on the job (#867, R-4.4, R-6.2, R-7.5) ----

/**
 * Payout stack applies through the epic and holds the page the ticket links
 * too, a repo and a pipeline; the parent PAY-230 links one page. `hides` is
 * what `link_set_hidden` holds for PAY-231, and `jobLinkSets` reads it.
 */
function setPorts(hides = new Set<string>(), writes: [string, string, boolean][] = []): JobPorts {
  const member = (kind: "repo" | "page" | "pipeline", entity_id: string, entity_kind: string, title: string) => ({
    kind,
    entity_id,
    entity_kind,
    title,
    hidden: hides.has(entity_id),
  });
  return ports({
    jobLinkSets: () =>
      Promise.resolve({
        sets: [
          {
            id: "payout-stack",
            name: "Payout stack",
            via: { kind: "epic", value: "jira:PAY-200", title: "PAY-200" },
            members: [
              member("page", PAGE.other.entity_id, "page", PAGE.other.title),
              member("repo", "gitea:tidewater/payout-service", "repo", "payout-service"),
              member("pipeline", "teamcity:Payout_Build", "build_config", "Payout_Build"),
            ],
          },
        ],
        parent: { ticket: "jira:PAY-230", title: "PAY-230", links: [{ entity_id: "confluence:RUN", kind: "page", title: "Payout runbook" }] },
      }),
    hideLinkSetMember: (ticket, memberId, hidden) => {
      writes.push([ticket, memberId, hidden]);
      if (hidden) hides.add(memberId);
      else hides.delete(memberId);
      return Promise.resolve(hidden);
    },
  });
}


test("the drawer draws a group per set and the parent; a linked row says which set also holds it", async () => {
  open({ ports: setPorts() });
  await settle();
  const drawer = drawerOpen();
  expect([...drawer.querySelectorAll("h3")].map((h) => squash(h))).toEqual([
    "Linked on PAY-231 2",
    "From Payout stack, via epic PAY-200 3 Edit set",
    "From Parent PAY-230, sub-tasks show their parent’s connections 1",
  ]);
  expect(drawer.querySelector("h3 a")?.getAttribute("href")).toBe("#/sets/payout-stack");
  expect([...drawer.querySelectorAll(".tf-lk")].map((row) => squash(row))).toEqual([
    "Idempotent retry window Code Unlink",
    "SEPA payout retry design Pages, also in Payout stack Unlink",
    "SEPA payout retry design Pages Hide here",
    "payout-service Code Hide here",
    "Payout_Build Builds Hide here",
    "Payout runbook Pages",
  ]);
  expect(drawer.querySelector("a[href='#/builds/teamcity%3APayout_Build']")).not.toBeNull();
});

test("Hide here hides one member on this ticket only and never edits the set; Undo and Show again bring it back", async () => {
  const writes: [string, string, boolean][] = [];
  open({ ports: setPorts(new Set(), writes) });
  await settle();
  const drawer = drawerOpen();
  drawer.querySelector<HTMLButtonElement>("button[aria-label='Hide here: payout-service']")!.click();
  await settle();
  expect(writes).toEqual([[ID, "gitea:tidewater/payout-service", true]]);
  expect(calls.writes).toEqual([]);
  expect(calls.links).toEqual([]);
  const hidden = [...drawer.querySelectorAll("h3")].find((h) => squash(h).startsWith("Hidden on"))!;
  expect(squash(hidden)).toBe("Hidden on PAY-231 1");
  expect(drawer.querySelector("button[aria-label='Hide here: payout-service']")).toBeNull();
  const toast = toasts.items.find((t) => t.text === "Hidden on PAY-231. The link set stays as it is.")!;
  expect(toast.action?.label).toBe("Undo");

  toast.action!.run();
  await settle();
  expect(writes.at(-1)).toEqual([ID, "gitea:tidewater/payout-service", false]);
  expect([...drawer.querySelectorAll("h3")].some((h) => squash(h).startsWith("Hidden on"))).toBe(false);

  drawer.querySelector<HTMLButtonElement>("button[aria-label='Hide here: Payout_Build']")!.click();
  await settle();
  drawer.querySelector<HTMLButtonElement>("button[aria-label='Show again: Payout_Build']")!.click();
  await settle();
  expect(writes.at(-1)).toEqual([ID, "teamcity:Payout_Build", false]);
  expect(drawer.querySelector("button[aria-label='Hide here: Payout_Build']")).not.toBeNull();
});

test("Undo after moving to another job shows the member again on the ticket it was hidden on", async () => {
  const writes: [string, string, boolean][] = [];
  const base = setPorts(new Set(), writes);
  const other = { ...ticket(), row: { ...ticket().row, entity_id: "jira:PAY-232" } };
  const props = $state({
    segment: "PAY-231",
    timer: { current: null, elapsed: null },
    layers: createLayers(),
    ontimer: () => {},
    onstarttimer: () => {},
    ports: { ...base, getEntity: (id: string) => (id === other.row.entity_id ? Promise.resolve(other) : base.getEntity(id)) },
    today: () => new Date(2026, 9, 15, 14, 0),
  });
  job = mount(TfJob, { target, props });
  await settle();
  drawerOpen().querySelector<HTMLButtonElement>("button[aria-label='Hide here: payout-service']")!.click();
  await settle();
  const toast = toasts.items.find((t) => t.text === "Hidden on PAY-231. The link set stays as it is.")!;
  props.segment = "PAY-232";
  await settle();
  toast.action!.run();
  await settle();
  expect(writes.at(-1)).toEqual([ID, "gitea:tidewater/payout-service", false]);
});

test("a comment its source writes in markup is drawn as Markdown (#2308)", async () => {
  const marked = thread();
  marked.comments.push({ author: "Priya Nair", at: "2026-10-15T13:00:00Z", body: "*Agreed*, see {{retry_window}}", mine: false, mentions_me: false, markup: "jira_wiki" });
  open({ ports: ports({ entityThread: () => Promise.resolve(marked) }) });
  await settle();
  const last = [...target.querySelectorAll(".tf-msg")].at(-1)!;
  expect(last.querySelector("strong")?.textContent).toBe("Agreed");
  expect(last.querySelector("code")?.textContent).toBe("retry_window");
  // A text comment stays text.
  expect(target.querySelector(".tf-newline + .tf-msg p")?.textContent).toBe("Left a comment on the PR.");
});
