/**
 * The Tab sheet (#2426, spec #2419 stories 22-38): Tab lifts the selected row
 * under the box, the box narrows the row's actions, and each action is an
 * address or a write of the app. Driven through the keyboard with fake ports;
 * the assertions are what is drawn, where the launcher went and what it wrote.
 *
 * The search fixture answers a word with one hit of the kind it names
 * (`repo` → a repository), so each test types the kind it is about.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { LauncherHome, SearchHit, SearchQuery, SearchResponse } from "../ipc";
import type { CheckoutSettings, InboxItem, LinkSetDraft, LinkSetRow, ViewAction } from "../ipc/entity";
import { reset as resetToasts, toasts } from "../shell/toasts.svelte";
import Launcher from "./Launcher.svelte";
import { DEBOUNCE_MS } from "./session.svelte";

let target: HTMLElement;
let app: ReturnType<typeof mount> | undefined;

const HITS: Record<string, Partial<SearchHit> & { kind: string; entity_id: string; title: string }> = {
  ticket: { kind: "ticket", entity_id: "jira:PAY-231", title: "Retry failed SEPA payouts" },
  pr: { kind: "pr", entity_id: "gitea:acme/svc#142", title: "Backoff", source_id: "gitea", path: "acme/svc" },
  repo: { kind: "repo", entity_id: "gitea:acme/svc", title: "acme/svc", source_id: "gitea" },
  branch: { kind: "branch", entity_id: "gitea:acme/svc@main", title: "main", source_id: "gitea", path: "acme/svc" },
  pipeline: { kind: "pipeline", entity_id: "teamcity:Pay_Build", title: "Pay build", source_id: "teamcity" },
  monitor: { kind: "monitor", entity_id: "kuma:7", title: "payout-api", source_id: "kuma", cell: "http" },
  page: { kind: "page", entity_id: "confluence:ENG:SEPA", title: "SEPA design", source_id: "confluence" },
  view: { kind: "view", entity_id: "my-open", title: "My open", source_id: "view" },
  set: { kind: "set", entity_id: "payments", title: "Payments", source_id: "set" },
  list: { kind: "list", entity_id: "sepa-payouts", title: "sepa payouts", source_id: "list", path: "#t sepa" },
};

function response(raw: string): SearchResponse {
  const word = raw.trim();
  const inbox = word === "#i";
  const one = HITS[word];
  const hits = one
    ? [
        {
          source_id: "jira",
          updated_at: null,
          synced_at: "2026-08-25T11:56:00Z",
          path: null,
          state: null,
          rank: 1,
          snippet: [],
          ...one,
        } as SearchHit,
      ]
    : [];
  return {
    interpreted: {
      text: inbox ? "" : word,
      prefix: null,
      filters: { sources: [], kinds: inbox ? ["inbox"] : [], updated_within_days: null, mine: false, authors: [] },
      unknown_tokens: [],
      left_out_tokens: [],
    },
    groups: one ? [{ kind: one.kind, label: one.kind, plural: `${one.kind}s`, monogram: "XX", total: 1, hits }] : [],
    total: hits.length,
    took_ms: 1,
    coverage: [],
  };
}

const HOME = { recent: [], favourites: [], sources: [], pending_writes: 0, notes_shown: true } as unknown as LauncherHome;

const APPS: CheckoutSettings = {
  applications: [
    { id: "idea", name: "IntelliJ", command: "idea {path}", file_command: null },
    { id: "code", name: "VS Code", command: "code {path}", file_command: null },
  ],
  default_application: "code",
} as unknown as CheckoutSettings;

const LINE: InboxItem = {
  key: "failed_build:teamcity:1187",
  category: "failed_build",
  source_id: "teamcity",
  entity_id: "teamcity:1187",
  kind: "build",
  title: "Pay build failed",
  reason: "failed",
  occurred_at: "2026-08-25T11:00:00Z",
  snoozed_until: null,
} as unknown as InboxItem;

const JOB = { entityId: "jira:PAY-999", label: "PAY-999" };

/** Two link sets, one already holding a repository and a ticket by hand. */
const SETS = [
  {
    id: "payments",
    name: "Payments",
    description: "The payout stack",
    members: [{ kind: "repo", entity_id: "gitea:acme/ledger", title: "acme/ledger" }],
    patterns: [{ kind: "page", query: "SEPA", count: 2 }],
    rules: [{ kind: "manual", value: "jira:PAY-100", title: null }],
    tickets: 3,
    yours: 1,
  },
  { id: "ops", name: "Ops", description: "On call", members: [], patterns: [], rules: [], tickets: 0, yours: 0 },
] as unknown as LinkSetRow[];

/** A source's declared merge, as Gitea's `land.rs` declares it: three styles, the branch deleted, squash's message prefilled. */
function declaredMerge(disabled: string | null = null): ViewAction {
  return {
    write_op: "merge_pull_request",
    label: "Merge…",
    fields: [
      {
        name: "style",
        label: "Style",
        kind: { type: "choice", options: [{ value: "merge", label: "Merge" }, { value: "rebase", label: "Rebase" }, { value: "squash", label: "Squash" }] },
        value: "merge",
        required: true,
        placeholder: null,
      },
      { name: "delete_branch", label: "Delete branch", kind: { type: "flag" }, value: "true", required: false, placeholder: null },
      { name: "message", label: "Message", kind: { type: "long_text" }, value: "Backoff (#142)", required: false, placeholder: null },
    ],
    confirm: null,
    primary: true,
    danger: false,
    disabled,
    with_settings: false,
    values: {},
    entity: null,
  } as unknown as ViewAction;
}

/** A repository's branches as its Branches view lists them: not the default first, so the sheet puts it there. */
const BRANCHES = [
  { name: "feature/PAY-231-retry", isDefault: false },
  { name: "main", isDefault: true },
  { name: "release/2.1", isDefault: false },
];

function mountIt(props: Record<string, unknown> = {}) {
  const calls = {
    navigate: vi.fn(),
    timer: vi.fn(),
    createLink: vi.fn(async () => ({})),
    submitWrite: vi.fn(async () => ({})),
    ackAlert: vi.fn(async () => ({})),
    copyText: vi.fn(async () => {}),
    openCheckout: vi.fn(async () => ["code", "/src/svc"]),
    repoOf: vi.fn(async () => "gitea:acme/svc"),
    clone: vi.fn(async () => {}),
    inboxAck: vi.fn(async () => {}),
    inboxDone: vi.fn(async () => {}),
    branches: vi.fn(async (_repoId: string) => BRANCHES),
    // No sets unless a test gives some: the table's other rows stay as they were.
    linkSets: vi.fn(async (): Promise<LinkSetRow[]> => []),
    saveLinkSet: vi.fn(async (_draft: LinkSetDraft) => ({})),
    inboxSnooze: vi.fn(async (_key: string, _until: Date) => {}),
    mergeAction: vi.fn(async (_prId: string): Promise<ViewAction | null> => null),
    renameSmartList: vi.fn(async (_id: string, _label: string) => {}),
    deleteSmartList: vi.fn(async (_id: string) => {}),
    searched: [] as string[],
  };
  app = mount(Launcher, {
    target,
    props: {
      open: true,
      onnavigate: calls.navigate,
      onclose: () => {},
      ontimer: calls.timer,
      openEntity: JOB,
      inbox: { count: 1, items: [LINE], actions: { [LINE.key]: ["rerun_build"] } },
      ports: {
        search: async (query: SearchQuery) => {
          calls.searched.push(query.raw);
          return response(query.raw);
        },
        launcherHome: async () => HOME,
        renameSmartList: calls.renameSmartList,
        deleteSmartList: calls.deleteSmartList,
      },
      sheetPorts: {
        createLink: calls.createLink,
        submitWrite: calls.submitWrite,
        ackAlert: calls.ackAlert,
        copyText: calls.copyText,
        openCheckout: calls.openCheckout,
        checkoutSettings: async () => APPS,
        repoOf: calls.repoOf,
        clone: calls.clone,
        inboxAck: calls.inboxAck,
        inboxDone: calls.inboxDone,
        branches: calls.branches,
        linkSets: calls.linkSets,
        saveLinkSet: calls.saveLinkSet,
        inboxSnooze: calls.inboxSnooze,
        mergeAction: calls.mergeAction,
      },
      ...props,
    },
  });
  flushSync();
  return calls;
}

async function settle() {
  for (let i = 0; i < 6; i += 1) await Promise.resolve();
  flushSync();
}

const input = () => target.querySelector("input")!;

async function type(text: string) {
  input().value = text;
  input().dispatchEvent(new Event("input", { bubbles: true }));
  vi.advanceTimersByTime(DEBOUNCE_MS);
  await settle();
}

/** Typing while the sheet is open: no debounce, nothing sent. */
function narrow(text: string) {
  input().value = text;
  input().dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

function press(key: string, options: KeyboardEventInit = {}) {
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...options });
  input().dispatchEvent(event);
  flushSync();
  return event;
}

const sheet = () => target.querySelector<HTMLElement>('[aria-label="Actions on the selected result"]');
const labels = () => [...(sheet()?.querySelectorAll(".tf-ln-nm") ?? [])].map((el) => el.textContent?.trim());
const modeLabel = () => target.querySelector(".tf-ln-mode")?.textContent ?? null;
const lastToast = () => toasts.items.at(-1)?.text;
/** The flow's step header: its title, and the picks so far. */
const stepHead = () => target.querySelector(".tf-ln-sheet .tf-ln-h")?.textContent?.replace(/\s+/gu, " ").trim() ?? null;

/** ⌘K after an action closed the launcher: open again on an empty box. */
async function reopen() {
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true, cancelable: true }));
  await settle();
}

/** Type `word`, wait for its row, and Tab onto it. */
async function sheetOn(word: string) {
  await type(word);
  press("Tab");
  await settle();
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  vi.useRealTimers();
  target.remove();
  resetToasts();
});

test("Tab lifts the row under the box, labels the box Actions and empties it; the footer says Tab actions", async () => {
  mountIt();
  await settle();
  await type("repo");
  expect(target.querySelector(".tf-launch footer")?.textContent).toContain("Tab actions");
  const event = press("Tab");
  await settle();
  expect(event.defaultPrevented).toBe(true);
  expect(modeLabel()).toBe("Actions");
  expect(input().value).toBe("");
  const pinned = target.querySelector(".tf-ln-pinned .tf-ln-res");
  expect(pinned?.querySelector(".tf-ln-t")?.textContent).toBe("svc");
  expect(pinned?.querySelector(".tf-ln-k")?.textContent).toBe("acme");
  expect(labels()).toEqual([
    "Link to PAY-999",
    "Create pull request…",
    "Pull requests",
    "Branches",
    "Open in VS Code",
    "Open in IntelliJ",
    "Clone",
    "Favourite",
    "Copy title",
  ]);
});

test("the pinned row draws the key cell the engine sends (#2452)", async () => {
  mountIt();
  await settle();
  await sheetOn("monitor");
  const pinned = target.querySelector(".tf-ln-pinned .tf-ln-res");
  expect(pinned?.querySelector(".tf-ln-k")?.textContent).toBe("http");
  expect(pinned?.querySelector(".tf-ln-t")?.textContent).toBe("payout-api");
});

test("a row with no actions keeps the sheet shut and the footer says nothing about Tab", async () => {
  mountIt({ openEntity: undefined });
  await settle();
  await type("nothing");
  press("Tab");
  expect(sheet()).toBeNull();
  expect(target.querySelector(".tf-launch footer")?.textContent).not.toContain("Tab actions");
});

test("typing narrows the actions and never reaches the search", async () => {
  const calls = mountIt();
  await settle();
  await sheetOn("repo");
  const before = calls.searched.length;
  narrow("br");
  expect(labels()[0]).toBe("Branches");
  vi.advanceTimersByTime(DEBOUNCE_MS);
  await settle();
  expect(calls.searched.length, "the sheet's filter was sent as a search").toBe(before);
});

test.each([
  ["Escape", ""],
  ["Tab", ""],
  ["Backspace", ""],
])("%s on the empty box leaves the sheet with the search words back exactly", async (key) => {
  mountIt();
  await settle();
  await type("repo");
  press("Tab");
  narrow("cl");
  if (key === "Backspace") narrow("");
  else narrow("");
  const event = press(key);
  expect(event.defaultPrevented).toBe(true);
  expect(sheet()).toBeNull();
  expect(input().value).toBe("repo");
  expect(modeLabel()).toBeNull();
  expect(target.querySelector('[role="dialog"]')).not.toBeNull();
});

test("a click on a relation chip or a row, then Esc, never closes the dialog", async () => {
  mountIt();
  await settle();
  await sheetOn("page");
  const chip = [...target.querySelectorAll<HTMLButtonElement>('[aria-label="Relation"] button')].find((b) => b.textContent === "documents")!;
  chip.focus();
  chip.click();
  flushSync();
  const esc = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
  (document.activeElement ?? chip).dispatchEvent(esc);
  flushSync();
  expect(target.querySelector('[role="dialog"]'), "Esc after a chip click closed the dialog").not.toBeNull();
  expect(input().value).toBe("page");
});

test("⌘K while open gives the box its focus back and changes nothing else", async () => {
  mountIt();
  await settle();
  await sheetOn("repo");
  narrow("br");
  (target.querySelector<HTMLButtonElement>('[aria-label="Relation"] button') ?? document.body).focus();
  input().blur();
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true, cancelable: true }));
  await settle();
  expect(document.activeElement).toBe(input());
  expect(input().value).toBe("br");
  expect(modeLabel()).toBe("Actions");
});

test("Link to the job writes the link with the relation picked by the arrows, through the dialog's mapping", async () => {
  const calls = mountIt();
  await settle();
  await sheetOn("page");
  expect(labels()[0]).toBe("Link to PAY-999");
  press("ArrowRight");
  press("ArrowRight");
  expect(target.querySelector('[aria-label="Relation"] [aria-pressed="true"]')?.textContent).toBe("documents");
  press("Enter");
  await settle();
  // `drawn("documents", job, page)`: from the job to the page, as TfLinkTo draws it.
  expect(calls.createLink).toHaveBeenCalledWith("jira:PAY-999", "confluence:ENG:SEPA", "documents");
  expect(lastToast()).toBe("Linked SEPA design to PAY-999 as documents");
  expect(target.querySelector('[role="dialog"]')).toBeNull();
});

test("Link to is offered on any page the shell names a job for, and never on the job's own ticket", async () => {
  mountIt({ openEntity: { entityId: "jira:PAY-231", label: "PAY-231" } });
  await settle();
  await sheetOn("ticket");
  expect(labels()).not.toContain("Link to PAY-231");
  expect(labels()[0]).toBe("Start timer");
});

test("a ticket: Start timer, Work on this, Copy key; Start timer hands the ticket to the shell", async () => {
  const calls = mountIt();
  await settle();
  await sheetOn("ticket");
  expect(labels()).toEqual(["Link to PAY-999", "Start timer", "Work on this", "Favourite", "Copy key"]);
  press("ArrowDown");
  press("Enter");
  await settle();
  expect(calls.timer).toHaveBeenCalledWith("jira:PAY-231", "Retry failed SEPA payouts");
});

test("Start timer is on tickets only", async () => {
  mountIt();
  await settle();
  for (const word of ["pr", "repo", "pipeline", "monitor", "page"]) {
    await sheetOn(word);
    expect(labels(), word).not.toContain("Start timer");
    press("Escape");
  }
});

test("Work on this opens the ticket as the job", async () => {
  const calls = mountIt();
  await settle();
  await sheetOn("ticket");
  narrow("work");
  press("Enter");
  expect(calls.navigate).toHaveBeenCalledWith("#/job/jira%3APAY-231");
});

test("a pull request: the files, the conversation, Approve queued as a write", async () => {
  const calls = mountIt();
  await settle();
  await sheetOn("pr");
  expect(labels()).toEqual(["Link to PAY-999", "Review the files", "Conversation", "Approve", "Open in VS Code", "Open in IntelliJ", "Favourite", "Copy title"]);
  narrow("approve");
  press("Enter");
  await settle();
  expect(calls.submitWrite).toHaveBeenCalledWith({ Approve: { entity: "gitea:acme/svc#142", body: "" } });
  expect(lastToast()).toBe("Approval queued for Backoff");
});

test("Review the files and Conversation go to the pull request's tabs", async () => {
  const calls = mountIt();
  await settle();
  await sheetOn("pr");
  narrow("files");
  press("Enter");
  expect(calls.navigate).toHaveBeenCalledWith("#/pr/gitea%3Aacme%2Fsvc%23142/diff");
});

test("Open in lists the applications in the settings' order, the default first, and opens the checkout", async () => {
  const calls = mountIt();
  await settle();
  await sheetOn("repo");
  narrow("intellij");
  press("Enter");
  await settle();
  expect(calls.openCheckout).toHaveBeenCalledWith("gitea:acme/svc", { application: "idea" });
  expect(lastToast()).toBe("Started IntelliJ: code /src/svc");
});

test("Open in on a pull request opens its repository's checkout", async () => {
  const calls = mountIt();
  await settle();
  await sheetOn("pr");
  narrow("vs code");
  press("Enter");
  await settle();
  expect(calls.repoOf).toHaveBeenCalledWith({ sourceId: "gitea", path: "acme/svc" });
  expect(calls.openCheckout).toHaveBeenCalledWith("gitea:acme/svc", { application: "code" });
});

test("a branch opens in each application", async () => {
  const calls = mountIt();
  await settle();
  await sheetOn("branch");
  expect(labels()).toEqual(["Link to PAY-999", "Open in VS Code", "Open in IntelliJ", "Favourite", "Copy title"]);
  narrow("code");
  press("Enter");
  await settle();
  expect(calls.openCheckout).toHaveBeenCalledWith("gitea:acme/svc@main", { application: "code" });
});

test("a repository's Pull requests, Branches and Clone", async () => {
  const calls = mountIt();
  await settle();
  await sheetOn("repo");
  narrow("branches");
  press("Enter");
  expect(calls.navigate).toHaveBeenCalledWith("#/repo/gitea%3Aacme%2Fsvc/branches");
  await reopen();
  await sheetOn("repo");
  narrow("clone");
  press("Enter");
  await settle();
  expect(calls.clone).toHaveBeenCalledWith({ entityId: "gitea:acme/svc", title: "acme/svc" });
});

test("a pipeline runs, queued as a write", async () => {
  const calls = mountIt();
  await settle();
  await sheetOn("pipeline");
  narrow("run");
  press("Enter");
  await settle();
  expect(calls.submitWrite).toHaveBeenCalledWith({ TriggerBuild: { entity: "teamcity:Pay_Build" } });
});

test("a monitor is acknowledged", async () => {
  const calls = mountIt();
  await settle();
  await sheetOn("monitor");
  narrow("ack");
  press("Enter");
  await settle();
  expect(calls.ackAlert).toHaveBeenCalledWith("kuma:7");
  expect(lastToast()).toBe("Acknowledged payout-api");
});

test("Copy title and Copy key put the text on the clipboard", async () => {
  const calls = mountIt();
  await settle();
  await sheetOn("ticket");
  narrow("copy");
  press("Enter");
  await settle();
  expect(calls.copyText).toHaveBeenCalledWith("PAY-231");
});

test("a view opens and copies its address; a set opens", async () => {
  const calls = mountIt();
  await settle();
  await sheetOn("view");
  expect(labels()).toEqual(["Open", "Copy address"]);
  press("ArrowDown");
  press("Enter");
  await settle();
  expect(calls.copyText).toHaveBeenCalledWith("#/tickets/my-open");
  await reopen();
  await type("set");
  press("Tab");
  expect(labels()).toEqual(["Open"]);
  press("Enter");
  expect(calls.navigate).toHaveBeenCalledWith("#/sets/payments");
});

test("a saved list applies its text, renames in the box and deletes", async () => {
  const calls = mountIt();
  await settle();
  await sheetOn("list");
  expect(labels()).toEqual(["Apply", "Rename", "Delete", "Favourite", "Assign key…"]);
  press("ArrowDown");
  press("Enter");
  await settle();
  expect(modeLabel()).toBe("New name");
  expect(input().value).toBe("sepa payouts");
  narrow("SEPA out");
  press("Enter");
  await settle();
  expect(calls.renameSmartList).toHaveBeenCalledWith("sepa-payouts", "SEPA out");
  expect(lastToast()).toBe("Renamed to SEPA out");
  expect(modeLabel()).toBeNull();

  await sheetOn("list");
  narrow("delete");
  press("Enter");
  await settle();
  expect(calls.deleteSmartList).toHaveBeenCalledWith("sepa-payouts");

  await sheetOn("list");
  press("Enter");
  await settle();
  expect(input().value).toBe("#t sepa ");
  expect(sheet()).toBeNull();
});

test("Esc while renaming goes back to the list's actions and renames nothing", async () => {
  const calls = mountIt();
  await settle();
  await sheetOn("list");
  narrow("rename");
  press("Enter");
  press("Escape");
  expect(modeLabel()).toBe("Actions");
  expect(calls.renameSmartList).not.toHaveBeenCalled();
  expect(target.querySelector('[role="dialog"]')).not.toBeNull();
});

test("a refused rename or delete toasts no success", async () => {
  const calls = mountIt();
  calls.renameSmartList.mockRejectedValueOnce(new Error("name taken"));
  calls.deleteSmartList.mockRejectedValueOnce(new Error("gone already"));
  await settle();
  await sheetOn("list");
  narrow("rename");
  press("Enter");
  await settle();
  narrow("SEPA out");
  press("Enter");
  await settle();
  expect(calls.renameSmartList).toHaveBeenCalled();
  expect(toasts.items.map((toast) => toast.text)).not.toContain("Renamed to SEPA out");

  await sheetOn("list");
  narrow("delete");
  press("Enter");
  await settle();
  expect(calls.deleteSmartList).toHaveBeenCalled();
  expect(toasts.items.map((toast) => toast.text)).not.toContain("Deleted sepa payouts");
});

test("an inbox line: Open, Re-run where the source offers it, Snooze…, Done", async () => {
  const calls = mountIt();
  await settle();
  await sheetOn("#i");
  expect(labels()).toEqual(["Open", "Re-run", "Snooze…", "Done"]);
  press("ArrowDown");
  press("Enter");
  await settle();
  expect(calls.submitWrite).toHaveBeenCalledWith({ RerunBuild: { entity: "teamcity:1187" } });
  await reopen();
  await sheetOn("#i");
  narrow("done");
  press("Enter");
  await settle();
  expect(calls.inboxDone).toHaveBeenCalledWith(LINE.key);
});

test("a failed write says why in a toast and keeps the sheet", async () => {
  const calls = mountIt();
  calls.submitWrite.mockRejectedValueOnce(new Error("source refused"));
  await settle();
  await sheetOn("pipeline");
  narrow("run");
  press("Enter");
  await settle();
  expect(toasts.items.at(-1)?.tone).toBe("err");
  expect(target.querySelector('[role="dialog"]')).not.toBeNull();
});

test("a two-step flow: Create pull request picks the head, then the base, the box narrowing each, and opens the compare page with both", async () => {
  const calls = mountIt();
  await settle();
  await sheetOn("repo");
  expect(labels()).toContain("Create pull request…");
  narrow("create");
  press("Enter");
  await settle();
  // The first step: its title on the box, the repository's branches with the default first; the row stays pinned.
  expect(modeLabel()).toBe("From branch");
  expect(input().value).toBe("");
  expect(calls.branches).toHaveBeenCalledWith("gitea:acme/svc");
  expect(labels()).toEqual(["main", "feature/PAY-231-retry", "release/2.1"]);
  expect(target.querySelector(".tf-ln-pinned .tf-ln-t")?.textContent).toBe("svc");
  narrow("pay");
  expect(labels()).toEqual(["feature/PAY-231-retry"]);
  press("Enter");
  await settle();
  // The second step: the head picked is said and left out of the bases.
  expect(modeLabel()).toBe("Into branch");
  expect(input().value).toBe("");
  expect(stepHead()).toContain("feature/PAY-231-retry");
  expect(labels()).toEqual(["main", "release/2.1"]);
  // Backspace on the empty box steps back one step, and once more back to the actions.
  press("Backspace");
  await settle();
  expect(modeLabel()).toBe("From branch");
  expect(labels()).toEqual(["main", "feature/PAY-231-retry", "release/2.1"]);
  press("Backspace");
  expect(modeLabel()).toBe("Actions");
  expect(labels()).toContain("Create pull request…");
  // Again, to the end: the last pick runs the action.
  narrow("create");
  press("Enter");
  await settle();
  press("ArrowDown");
  press("Enter");
  await settle();
  narrow("rel");
  press("Enter");
  await settle();
  expect(calls.navigate).toHaveBeenCalledWith("#/repo/gitea%3Aacme%2Fsvc/compare/release%2F2.1...feature%2FPAY-231-retry");
});

test("a step's late answer for picks since stepped back from is dropped", async () => {
  const calls = mountIt();
  const held: (() => void)[] = [];
  await settle();
  await sheetOn("repo");
  narrow("create");
  press("Enter");
  await settle();
  // From here every read waits until the test answers it.
  calls.branches.mockImplementation(() => new Promise((resolve) => held.push(() => resolve(BRANCHES))));
  press("Enter"); // head: main
  await settle();
  press("Backspace");
  await settle();
  press("ArrowDown");
  press("Enter"); // head: feature/PAY-231-retry
  await settle();
  expect(held).toHaveLength(2);
  // The read for head main answers first: it was asked for other picks.
  held[0]!();
  await settle();
  expect(labels()).not.toContain("feature/PAY-231-retry");
  expect(labels()).toEqual([]);
  held[1]!();
  await settle();
  expect(labels()).toEqual(["main", "release/2.1"]);
});

test("Esc in a flow steps back too, and never closes the dialog", async () => {
  const onclose = vi.fn();
  mountIt({ onclose });
  await settle();
  await sheetOn("repo");
  narrow("create");
  press("Enter");
  await settle();
  press("Enter");
  await settle();
  expect(modeLabel()).toBe("Into branch");
  const event = press("Escape");
  await settle();
  expect(modeLabel()).toBe("From branch");
  press("Escape");
  expect(modeLabel()).toBe("Actions");
  expect(event.defaultPrevented).toBe(true);
  expect(onclose).not.toHaveBeenCalled();
});

test("Add to a set… on a page picks a set and re-saves it with the page as one fixed member", async () => {
  const calls = mountIt();
  calls.linkSets.mockImplementation(async () => SETS);
  await settle();
  await sheetOn("page");
  expect(labels().slice(0, 2)).toEqual(["Link to PAY-999", "Add to a set…"]);
  narrow("add");
  press("Enter");
  await settle();
  expect(modeLabel()).toBe("Add to");
  expect(labels()).toEqual(["Payments", "Ops"]);
  press("Enter");
  await settle();
  expect(calls.saveLinkSet).toHaveBeenCalledTimes(1);
  // Everything the set held stays, in order; the page joins its members.
  expect(calls.saveLinkSet.mock.calls[0]![0]).toEqual({
    id: "payments",
    name: "Payments",
    description: "The payout stack",
    members: [
      { kind: "repo", entity_id: "gitea:acme/ledger" },
      { kind: "page", entity_id: "confluence:ENG:SEPA" },
    ],
    patterns: [{ kind: "page", query: "SEPA" }],
    rules: [{ kind: "manual", value: "jira:PAY-100" }],
  });
  expect(lastToast()).toBe("Added SEPA design to Payments");
});

test("Show a set on a ticket picks a set and adds one manual rule for the ticket", async () => {
  const calls = mountIt();
  calls.linkSets.mockImplementation(async () => SETS);
  await settle();
  await sheetOn("ticket");
  narrow("show");
  expect(labels()[0]).toBe("Show a set on PAY-231…");
  press("Enter");
  await settle();
  expect(modeLabel()).toBe("Show on PAY-231");
  narrow("ops");
  press("Enter");
  await settle();
  expect(calls.saveLinkSet.mock.calls[0]![0]).toMatchObject({ id: "ops", members: [], rules: [{ kind: "manual", value: "jira:PAY-231" }] });
  expect(lastToast()).toBe("Ops now shows on PAY-231");
});

test("a set: Show on the job adds one manual rule for the job's ticket (story 38)", async () => {
  const calls = mountIt();
  calls.linkSets.mockImplementation(async () => SETS);
  await settle();
  await sheetOn("set");
  expect(labels()).toEqual(["Open", "Show on PAY-999"]);
  narrow("show");
  press("Enter");
  await settle();
  expect(calls.saveLinkSet).toHaveBeenCalledTimes(1);
  expect(calls.saveLinkSet.mock.calls[0]![0]).toMatchObject({
    id: "payments",
    rules: [
      { kind: "manual", value: "jira:PAY-100" },
      { kind: "manual", value: "jira:PAY-999" },
    ],
  });
  expect(lastToast()).toBe("Payments now shows on PAY-999");
});

test("no link sets, no set row; a kind a set cannot hold has none either", async () => {
  const calls = mountIt();
  await settle();
  await sheetOn("page");
  expect(labels()).not.toContain("Add to a set…");
  await reopen();
  calls.linkSets.mockImplementation(async () => SETS);
  await sheetOn("pr");
  expect(labels().some((label) => label?.includes("set"))).toBe(false);
});

test("Snooze… offers the Inbox page's presets and snoozes the line until the one picked", async () => {
  // Tuesday 25 August 2026, noon: tomorrow is Wednesday, next Monday the 31st, both at 09:00.
  const calls = mountIt({ now: new Date(2026, 7, 25, 12, 0) });
  await settle();
  await sheetOn("#i");
  narrow("snooze");
  press("Enter");
  await settle();
  expect(modeLabel()).toBe("Snooze until");
  expect(labels()).toEqual(["Tomorrow", "Next Monday"]);
  press("ArrowDown");
  press("Enter");
  await settle();
  expect(calls.inboxSnooze).toHaveBeenCalledWith(LINE.key, new Date(2026, 7, 31, 9, 0));
  expect(lastToast()).toBe("Snoozed until Monday 09:00");
});

test("Snooze… on a Sunday: Tomorrow and Next Monday are the same morning, and both are drawn", async () => {
  // Sunday 30 August 2026: both presets are Monday the 31st at 09:00, so a choice keyed by the time would repeat.
  const calls = mountIt({ now: new Date(2026, 7, 30, 12, 0) });
  await settle();
  await sheetOn("#i");
  narrow("snooze");
  press("Enter");
  await settle();
  expect(labels()).toEqual(["Tomorrow", "Next Monday"]);
  press("ArrowDown");
  press("Enter");
  await settle();
  expect(calls.inboxSnooze).toHaveBeenCalledWith(LINE.key, new Date(2026, 7, 31, 9, 0));
});

test("Merge… follows the source's declared merge: picks the style and queues the write with the source's other answers", async () => {
  const calls = mountIt();
  calls.mergeAction.mockImplementation(async () => declaredMerge());
  await settle();
  await sheetOn("pr");
  expect(calls.mergeAction).toHaveBeenCalledWith("gitea:acme/svc#142");
  expect(labels()).toContain("Merge…");
  narrow("merge");
  press("Enter");
  await settle();
  expect(modeLabel()).toBe("Merge as");
  expect(labels()).toEqual(["Merge commit", "Rebase", "Squash"]);
  narrow("squ");
  press("Enter");
  await settle();
  expect(calls.submitWrite).toHaveBeenCalledWith({
    MergePullRequest: { entity: "gitea:acme/svc#142", style: "squash", delete_branch: true, message: "Backoff (#142)" },
  });
  expect(lastToast()).toBe("Merge queued for Backoff");
});

test("Merge… the source draws disabled shows its reason and writes nothing; no declared merge, no row", async () => {
  const calls = mountIt();
  calls.mergeAction.mockImplementation(async () => declaredMerge("1 of 2 required approvals"));
  await settle();
  await sheetOn("pr");
  narrow("merge");
  const row = [...(sheet()?.querySelectorAll(".tf-ln-opt") ?? [])].find((el) => el.querySelector(".tf-ln-nm")?.textContent === "Merge…");
  expect(row?.querySelector(".tf-ln-s")?.textContent).toBe("1 of 2 required approvals");
  press("Enter");
  await settle();
  expect(calls.submitWrite).not.toHaveBeenCalled();
  expect(lastToast()).toBe("1 of 2 required approvals");
  expect(modeLabel()).toBe("Actions");
  await reopen();
  calls.mergeAction.mockImplementation(async () => null);
  await sheetOn("pr");
  expect(labels()).not.toContain("Merge…");
});
