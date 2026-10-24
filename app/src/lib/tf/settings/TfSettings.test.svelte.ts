/**
 * Settings › Sources and a source's page (#830), against stand-ins for every
 * command, so what each button sends is what the test reads.
 *
 * `update_source` and `backfill_source` are the two commands this ticket is
 * the first UI for; their live behaviour is `crates/knobas-app/tests/it`'s.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { TicketViewRow } from "../../ipc/search";
import type {
  AdapterPreview,
  AdapterStatus,
  CredentialHealth,
  KindPaths,
  SourceSummary,
  SourceSyncStatus,
  SyncProgress,
  SyncRunRow,
  UpdatePreview,
} from "../../ipc/sources";
import { toasts } from "../../shell/toasts.svelte";
import { GITEA_SCHEMA, JIRA_SCHEMA } from "../../sources/fixtures";
import { createLayers } from "../layers.svelte";
import type { SettingsPorts } from "./ports";
import TfSettings from "./TfSettings.svelte";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  location.hash = "";
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

const NOW = new Date("2026-10-15T14:30:00Z");
const AGO = (min: number) => new Date(NOW.getTime() - min * 60_000).toISOString();
const ticket = { id: "ticket", label: "Ticket", plural: "Tickets", monogram: "TK", full_sync_exhaustive: true };

function healthOf(id: string, state: CredentialHealth["state"]): CredentialHealth {
  return { source_id: id, state, checked_at: AGO(38), detail: null, secret_expires_at: null };
}

function summary(id: string, name: string, state: CredentialHealth["state"], over: Partial<SourceSummary> = {}): SourceSummary {
  return {
    id,
    adapter_kind: id,
    display_name: name,
    base_url: id === "jira" ? "https://jira.tidewater.example" : "https://teamcity.tidewater.example",
    enabled: true,
    sync_interval_secs: 300,
    // A field besides the user, so Save is seen to send the config whole:
    // `update_source` replaces it, and a field left out is lost.
    config: { username: "mara", projects: ["TW", "OPS"] },
    health: healthOf(id, state),
    last_run: {
      id: id === "jira" ? 1 : 2,
      source_id: id,
      trigger: "schedule",
      started_at: AGO(5),
      finished_at: AGO(4),
      outcome: state === "unauthorized" ? "unauthorized" : "ok",
      upserted: 12,
      deleted: 0,
      swept: 0,
      error: state === "unauthorized" ? "401 Unauthorized" : null,
      cursor_after: null,
    },
    next_run_at: null,
    item_count: 177,
    auth_kind: "UserPassword",
    kinds: [ticket],
    roles: [],
    role_misses: [],
    ...over,
  };
}

const SOURCES = [summary("jira", "Jira", "ok"), summary("teamcity", "TeamCity", "unauthorized")];

function createHealthStub() {
  let rows = $state<CredentialHealth[]>([]);
  return {
    patched: [] as CredentialHealth[],
    get(id: string) {
      return rows.find((row) => row.source_id === id) ?? null;
    },
    patch(next: CredentialHealth) {
      this.patched.push(next);
      rows = [...rows.filter((row) => row.source_id !== next.source_id), next];
    },
    replace(next: CredentialHealth[]) {
      rows = next;
    },
  };
}

function ports(over: Partial<SettingsPorts> = {}): SettingsPorts {
  const runs: SyncRunRow[] = SOURCES.map((source) => source.last_run!);
  return {
    listSources: vi.fn(() => Promise.resolve(SOURCES)),
    listAdapters: vi.fn(() =>
      Promise.resolve([
        {
          id: "jira",
          adapter_kind: "jira",
          name: "Jira Data Center",
          capabilities: [],
          adapter_version: "1",
          auth_methods: ["UserPassword" as const],
          accepts_account: false,
          min_sync_interval_secs: 60,
          roles: [],
          source_actions: [],
          clone_from: [],
          url_forms: [],
          write_ops: [],
          entity_kinds: [ticket],
          config_schema: JIRA_SCHEMA,
          payload_paths: [],
        },
      ]),
    ),
    syncStatus: vi.fn(() => Promise.resolve([])),
    listSyncRuns: vi.fn((id: string) => Promise.resolve(runs.filter((run) => run.source_id === id))),
    updateSource: vi.fn((id: string, patch) =>
      Promise.resolve({ ...SOURCES.find((source) => source.id === id)!, ...(patch as object) } as SourceSummary),
    ),
    backfillSource: vi.fn(() => Promise.resolve(41)),
    deleteSource: vi.fn(() => Promise.resolve()),
    setSourceSecret: vi.fn((id: string) => Promise.resolve(healthOf(id, "ok"))),
    testSource: vi.fn(() =>
      Promise.resolve({
        ok: true,
        account: "mara",
        server_version: "9.12.4",
        secret_expires_at: null,
        error: null,
        code: null,
        elapsed_ms: 214,
        detail: null,
        discovered: {},
      }),
    ),
    syncNow: vi.fn(() => Promise.resolve(42)),
    watchSync: vi.fn(() => Promise.resolve(null)),
    syncAll: vi.fn(() => Promise.resolve([1, 2])),
    addSource: vi.fn(() => Promise.reject(new Error("not in this test"))),
    previewInstallAdapter: vi.fn(() => Promise.reject(new Error("not in this test"))),
    installAdapter: vi.fn(() => Promise.reject(new Error("not in this test"))),
    listInstalledAdapters: vi.fn(() => Promise.resolve([])),
    checkAdapterUpdates: vi.fn(() => Promise.resolve([])),
    previewUpdateAdapter: vi.fn(() => Promise.reject(new Error("not in this test"))),
    updateAdapter: vi.fn(() => Promise.reject(new Error("not in this test"))),
    uninstallAdapter: vi.fn(() => Promise.resolve()),
    listen: vi.fn(() => Promise.resolve(() => {})),
    ticketViews: vi.fn(() => Promise.resolve(JIRA_VIEWS.map((row) => ({ ...row })))),
    hideSavedView: vi.fn(() => Promise.resolve()),
    appStatus: vi.fn(() => Promise.resolve({ secrets_in_memory: false })),
    ...over,
  };
}

/** The Jira views the `jira` source keeps (#878): a board with tickets in an unsynced project, a hidden filter. */
const JIRA_VIEWS: TicketViewRow[] = [
  ["jira-board-21", "Payments and Ops flow", "board", "kanban", false, [{ project: "WEB", count: 3 }], 18],
  ["jira-filter-10300", "PAY without a fix version", "filter", null, true, [], 4],
].map(([id, name, kind, type, hidden, unsynced, count], position) => ({
  id: id as string,
  name: name as string,
  group: kind === "board" ? "Jira boards" : "Jira filters",
  position,
  query: "",
  layout: "board" as const,
  columns: [],
  lanes: "none" as const,
  card_fields: [],
  list_columns: ["key" as const],
  list_sort: null,
  saved_view: `jira:${(id as string).replace("jira-", "")}`,
  count: count as number,
  unreadable: null,
  saved_view_source: {
    kind: kind as "board" | "filter",
    system: "Jira",
    board_type: type as string | null,
    source_query: "",
    sub_query: null,
    sprint: null,
    last_read: "",
    web_url: null,
    hidden: hidden as boolean,
    new: false,
    unsynced: unsynced as { project: string; count: number }[],
  },
}));

test("Boards and filters: the choice and the favourite filters saved into the config, and each view with Hide or Show (#878)", async () => {
  const p = ports();
  // Its saved views' ticket system, which the section names (#2032): not the adapter's name.
  const [jira] = await p.listAdapters();
  p.listAdapters = vi.fn(() =>
    Promise.resolve([
      {
        ...jira!,
        entity_kinds: [...jira!.entity_kinds, { id: "jira_view", label: "Jira view", plural: "Jira views", monogram: "JV", full_sync_exhaustive: false }],
        payload_paths: [{ kind: "jira_view", frame: "saved_view", system_name: "Jira" } as KindPaths],
      },
    ]),
  );
  await open(["source", "jira"], p);
  const boards = section("Boards and filters");
  expect(boards.querySelector("p")!.textContent).toBe("Each one is a ticket view in Tickets, kept at every sync. Jira decides which tickets it holds.");
  const radios = [...boards.querySelectorAll<HTMLInputElement>('input[type="radio"]')];
  expect(radios.map((radio) => radio.parentElement!.textContent!.replace(/\s+/g, " ").trim())).toEqual([
    "Boards on the projects this source syncs1 board",
    "Only boards built on my favourite filters",
    "Every board I can see",
    "No boards",
  ]);
  expect(radios[0]!.checked).toBe(true);
  // Not rows of the generated form: this section draws them.
  expect(section("What it syncs").querySelector("#tf-src-cfg-boards")).toBeNull();

  radios[2]!.click();
  await settle();
  expect(p.updateSource).toHaveBeenLastCalledWith("jira", {
    config: { username: "mara", projects: ["TW", "OPS"], boards: "all" },
  });
  expect(toasts.items.at(-1)?.text).toBe("Saved. The next Jira sync reads the boards again.");
  boards.querySelector<HTMLButtonElement>('button[aria-label="Favourite filters"]')!.click();
  await settle();
  expect(p.updateSource).toHaveBeenLastCalledWith("jira", {
    config: { username: "mara", projects: ["TW", "OPS"], favourite_filters: false },
  });

  const rows = [...boards.querySelectorAll("tbody tr")].map((row) =>
    [...row.querySelectorAll("td")].map((cell) => cell.textContent!.replace(/\s+/g, " ").trim()),
  );
  expect(rows).toEqual([
    ["Payments and Ops flow", "Kanban board", "18 and 3 in WEB, not synced", "Hide"],
    ["PAY without a fix version hidden", "Filter", "4", "Show"],
  ]);
  boards.querySelector<HTMLButtonElement>('button[aria-label="Show PAY without a fix version"]')!.click();
  await settle();
  expect(p.hideSavedView).toHaveBeenCalledWith("jira-filter-10300", false);
});

async function settle() {
  for (let turn = 0; turn < 6; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

async function open(rest: string[], p: SettingsPorts = ports(), health = createHealthStub()) {
  view = mount(TfSettings, {
    target,
    props: { rest, job: null, ready: true, ports: p, health, layers: createLayers(), now: () => NOW },
  });
  await settle();
  return { p, health };
}

function button(text: string): HTMLButtonElement {
  const found = [...target.querySelectorAll("button")].find((el) => el.textContent?.trim() === text);
  if (!found) throw new Error(`no button "${text}"`);
  return found;
}

function type(selector: string, value: string) {
  const input = target.querySelector<HTMLInputElement>(selector)!;
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

test("Sources lists each source with its state, address, holdings and last sync", async () => {
  const { p } = await open([]);
  const rows = [...target.querySelectorAll(".tf-stg-src")];
  const cells = (row: Element) =>
    [...row.querySelectorAll("b, .tf-dim, .tf-stg-si")].map((cell) => cell.textContent?.trim());
  expect(rows.map(cells)).toEqual([
    ["Jira", "jira.tidewater.example", "177 tickets", "Synced 4 min ago"],
    ["TeamCity", "teamcity.tidewater.example", "177 tickets", "Sign-in failed"],
  ]);
  expect(rows[1]!.getAttribute("href")).toBe("#/settings/source/teamcity");
  expect(rows[1]!.querySelector(".tf-dot")?.classList.contains("tf-err")).toBe(true);
  // A source that needs the reader marks Sources in the left-hand navigation.
  expect(target.querySelector('.tf-stg-nav a[aria-current="page"] .tf-dot.tf-err')).not.toBeNull();

  button("Sync all").click();
  await settle();
  expect(p.syncAll).toHaveBeenCalledOnce();
});

test("Save sends what changed in the connection through update_source", async () => {
  const { p } = await open(["source", "jira"]);
  expect(button("Save").disabled).toBe(true);

  type("#tf-src-address", "https://jira2.tidewater.example");
  type("#tf-src-user", "mlindqvist");
  button("Save").click();
  await settle();

  expect(p.updateSource).toHaveBeenCalledWith("jira", {
    base_url: "https://jira2.tidewater.example",
    config: { username: "mlindqvist", projects: ["TW", "OPS"] },
  });
});

function field(key: string): HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement {
  const found = target.querySelector<HTMLInputElement>(`#tf-src-cfg-${key}`);
  if (!found) throw new Error(`no config field ${key}`);
  return found;
}

function section(heading: string): HTMLElement {
  const found = [...target.querySelectorAll<HTMLElement>(".tf-stg-sec")].find(
    (sec) => sec.querySelector("h2")?.textContent === heading,
  );
  if (!found) throw new Error(`no section "${heading}"`);
  return found;
}

function sectionButton(heading: string, text: string): HTMLButtonElement {
  const found = [...section(heading).querySelectorAll("button")].find((el) => el.textContent?.trim() === text);
  if (!found) throw new Error(`no button "${text}" in ${heading}`);
  return found;
}

test("a source's page edits its adapter's config, drawn from the schema, and sends the whole config (#904)", async () => {
  const { p } = await open(["source", "jira"]);
  const syncs = section("What it syncs");
  // Generated from Jira's schema: its own fields, the user excepted (Connection draws it).
  expect(syncs.querySelector("#tf-src-cfg-projects")).not.toBeNull();
  expect(syncs.querySelector("#tf-src-cfg-jql_filter")).not.toBeNull();
  expect(syncs.querySelector("#tf-src-cfg-username")).toBeNull();
  expect(field("projects").value).toBe("TW\nOPS");
  expect(sectionButton("What it syncs", "Save").disabled).toBe(true);

  type("#tf-src-cfg-jql_filter", "project = TW AND updated >= -30d");
  sectionButton("What it syncs", "Save").click();
  await settle();

  expect(p.updateSource).toHaveBeenCalledWith("jira", {
    config: { username: "mara", projects: ["TW", "OPS"], jql_filter: "project = TW AND updated >= -30d" },
  });
});

test("Sync WEB too opens the source's page with the project added, for the reader to save (#879)", async () => {
  const { p } = await open(["source", "jira", "add", "projects", "WEB"]);
  expect(field("projects").value).toBe("TW\nOPS\nWEB");
  sectionButton("What it syncs", "Save").click();
  await settle();
  expect(p.updateSource).toHaveBeenCalledWith("jira", { config: { username: "mara", projects: ["TW", "OPS", "WEB"] } });
});

test("the added project is in view, with a note beside Save until it is saved (#1404)", async () => {
  const scrolled: Element[] = [];
  const scrollIntoView = vi.fn(function (this: Element) {
    scrolled.push(this);
  });
  Element.prototype.scrollIntoView = scrollIntoView;
  try {
    const jira = summary("jira", "Jira", "ok", { config: { username: "mara", projects: ["PAY", "OPS", "CUS", "TRK"] } });
    const { p } = await open(
      ["source", "jira", "add", "projects", "WEB"],
      ports({ listSources: vi.fn(() => Promise.resolve([jira, SOURCES[1]!])) }),
    );
    // Five lines, five rows: WEB is on the last line and nothing scrolls inside the field.
    expect(field("projects").value).toBe("PAY\nOPS\nCUS\nTRK\nWEB");
    expect((field("projects") as HTMLTextAreaElement).rows).toBe(5);
    // Save and its note brought up from below, then the field, so both are in sight.
    expect(scrolled).toEqual([section("What it syncs").querySelector(".tf-stg-acts"), field("projects")]);
    expect(addedNote()).toBe("WEB added, not saved yet");

    sectionButton("What it syncs", "Save").click();
    await settle();
    expect(p.updateSource).toHaveBeenCalledWith("jira", {
      config: { username: "mara", projects: ["PAY", "OPS", "CUS", "TRK", "WEB"] },
    });
    expect(addedNote()).toBeNull();
  } finally {
    delete (Element.prototype as { scrollIntoView?: unknown }).scrollIntoView;
  }
});

test("editing the added field takes the note away; a page opened without an addition has none (#1404)", async () => {
  await open(["source", "jira", "add", "projects", "WEB"]);
  expect(addedNote()).toBe("WEB added, not saved yet");
  type("#tf-src-cfg-projects", "TW\nOPS\nWEB\nCUS");
  expect(addedNote()).toBeNull();
  unmount(view!);
  view = null;

  await open(["source", "jira"]);
  expect(addedNote()).toBeNull();
  // Two values, the list's floor of three rows, as before.
  expect((field("projects") as HTMLTextAreaElement).rows).toBe(3);
});

test("a project the source already syncs is not added twice", async () => {
  await open(["source", "jira", "add", "projects", "OPS"]);
  expect(field("projects").value).toBe("TW\nOPS");
  expect(sectionButton("What it syncs", "Save").disabled).toBe(true);
});

test("a Jira source's page shows the two field ids, or says none is set (#1021)", async () => {
  const jira = summary("jira", "Jira", "ok", { config: { username: "mara", epic_link_field: "customfield_10108" } });
  await open(["source", "jira"], ports({ listSources: vi.fn(() => Promise.resolve([jira, SOURCES[1]!])) }));
  expect(field("epic_link_field").value).toBe("customfield_10108");
  expect(field("sprint_field").value).toBe("");
  expect((field("sprint_field") as HTMLInputElement).placeholder).toBe("None set");
  expect(target.querySelector('label[for="tf-src-cfg-sprint_field"]')?.textContent).toContain("Sprint field id");
});

test("saving keeps a field id the sync recorded after the page was read, beside one the reader typed (#1021)", async () => {
  const recorded = summary("jira", "Jira", "ok", {
    config: { username: "mara", projects: ["TW", "OPS"], sprint_field: "customfield_10107" },
  });
  const listSources = vi.fn().mockResolvedValueOnce(SOURCES).mockResolvedValue([recorded, SOURCES[1]!]);
  const { p } = await open(["source", "jira"], ports({ listSources }));
  // The page was read before the sync recorded the sprint id.
  expect(field("sprint_field").value).toBe("");

  type("#tf-src-cfg-epic_link_field", "customfield_10108");
  sectionButton("What it syncs", "Save").click();
  await settle();
  expect(p.updateSource).toHaveBeenLastCalledWith("jira", {
    config: {
      username: "mara",
      projects: ["TW", "OPS"],
      sprint_field: "customfield_10107",
      epic_link_field: "customfield_10108",
    },
  });

  // The Connection's Save reads the stored config again too.
  type("#tf-src-user", "mlindqvist");
  sectionButton("Connection", "Save").click();
  await settle();
  expect(p.updateSource).toHaveBeenLastCalledWith("jira", {
    config: { username: "mlindqvist", projects: ["TW", "OPS"], sprint_field: "customfield_10107" },
  });
});

test("clearing an id the reader typed removes it, so the next sync can record one", async () => {
  const jira = summary("jira", "Jira", "ok", { config: { username: "mara", sprint_field: "customfield_1" } });
  const { p } = await open(["source", "jira"], ports({ listSources: vi.fn(() => Promise.resolve([jira, SOURCES[1]!])) }));
  type("#tf-src-cfg-sprint_field", "");
  sectionButton("What it syncs", "Save").click();
  await settle();
  expect(p.updateSource).toHaveBeenLastCalledWith("jira", { config: { username: "mara" } });
});

function dialog(): HTMLElement {
  const found = target.querySelector<HTMLElement>('.tf-dialog[role="dialog"]');
  if (!found) throw new Error("no dialog");
  return found;
}

function dialogButton(text: string): HTMLButtonElement {
  const found = [...dialog().querySelectorAll("button")].find((el) => el.textContent?.trim() === text);
  if (!found) throw new Error(`no dialog button "${text}"`);
  return found;
}

test("Add a source opens in the ticket-first look, and Esc closes it through the layer stack (#903)", async () => {
  const layers = createLayers();
  view = mount(TfSettings, {
    target,
    props: { rest: [], job: null, ready: true, ports: ports(), health: createHealthStub(), layers, now: () => NOW },
  });
  await settle();
  button("+ Add a source").click();
  await settle();
  expect(dialog().querySelector("h2")?.textContent).toBe("Add a source");
  expect(dialog().querySelector(".tf-pane-h")?.textContent).toBe("What is it?");
  // One radio per adapter, with what it holds as its description.
  const choice = dialog().querySelector<HTMLLabelElement>(".tf-radio label")!;
  expect(choice.querySelector("input")?.type).toBe("radio");
  expect(choice.textContent).toContain("Jira Data Center");
  expect(choice.querySelector("small")?.textContent).toBe("Tickets");
  expect(layers.open.map((layer) => layer.kind)).toEqual(["dialog"]);

  layers.open[0]!.close();
  flushSync();
  expect(target.querySelector('[role="dialog"]')).toBeNull();
});

test("Add a source: the generated form, Test connection's fills, then one add_source (#903)", async () => {
  const added = summary("jira-2", "Jira Data Center", "ok");
  const p = ports({
    addSource: vi.fn(() => Promise.resolve(added)),
    testSource: vi.fn(() =>
      Promise.resolve({
        ok: true,
        account: "mara",
        server_version: "9.12.4",
        secret_expires_at: null,
        error: null,
        code: null,
        elapsed_ms: 214,
        detail: null,
        discovered: { epic_link_field: "customfield_10101", sprint_field: "customfield_10107" },
      }),
    ),
  });
  await open([], p);
  button("+ Add a source").click();
  await settle();
  dialog().querySelector<HTMLInputElement>(".tf-radio input")!.click();
  flushSync();

  // Generated from the chosen adapter's schema: no per-adapter branch.
  expect(dialog().querySelector("#tf-add-cfg-projects")).not.toBeNull();
  expect(dialog().querySelector<HTMLInputElement>("#tf-add-id")!.value).toBe("jira");
  expect(dialogButton("Add source").disabled).toBe(true);

  type("#tf-add-id", "jira-2");
  type("#tf-add-address", "https://jira2.tidewater.example");
  type("#tf-add-secret", "hunter2");
  type("#tf-add-cfg-sprint_field", "customfield_99");
  dialogButton("Test connection").click();
  await settle();

  expect(p.testSource).toHaveBeenCalledWith({
    source_id: null,
    adapter_kind: "jira",
    base_url: "https://jira2.tidewater.example",
    auth_kind: "UserPassword",
    config: { flavor: "datacenter", sprint_field: "customfield_99" },
    secret: { value: "hunter2", account: null },
  });
  expect(dialog().textContent).toContain("Connected as mara · 9.12.4 · 214 ms");
  // The identity fill (#82) and the discovered fill (#297), only into empty fields.
  expect(dialog().querySelector<HTMLInputElement>("#tf-add-cfg-username")!.value).toBe("mara");
  expect(dialog().querySelector<HTMLInputElement>("#tf-add-cfg-epic_link_field")!.value).toBe("customfield_10101");
  expect(dialog().querySelector<HTMLInputElement>("#tf-add-cfg-sprint_field")!.value).toBe("customfield_99");

  dialogButton("Add source").click();
  await settle();
  expect(p.addSource).toHaveBeenCalledWith({
    id: "jira-2",
    adapter_kind: "jira",
    display_name: "Jira Data Center",
    base_url: "https://jira2.tidewater.example",
    auth_kind: "UserPassword",
    config: {
      flavor: "datacenter",
      username: "mara",
      epic_link_field: "customfield_10101",
      sprint_field: "customfield_99",
    },
    secret: { value: "hunter2", account: null },
    sync_interval_secs: 900,
    enabled: true,
  });
  expect(target.querySelector('[role="dialog"]')).toBeNull();
  // The list is read again, so the new source has its row.
  expect(p.listSources).toHaveBeenCalledTimes(2);
});

/** The fixture component's template as `list_adapters` answers it once installed (#1202). */
const SURVEYS = {
  id: "wasm-fixture",
  adapter_kind: "wasm-fixture",
  name: "Harbour surveys",
  capabilities: [],
  adapter_version: "0.1.0",
  auth_methods: ["Pat" as const],
  accepts_account: false,
  min_sync_interval_secs: 60,
  roles: [{ role: "wasm-fixture:survey-desk", reads: [] }],
  source_actions: [],
  write_ops: [],
  entity_kinds: [{ id: "survey", label: "Survey", plural: "Surveys", monogram: "SV", full_sync_exhaustive: true }],
  config_schema: {
    type: "object",
    properties: { fault: { type: "string", enum: ["none", "unauthorized", "unreachable", "swallow_sink"] } },
    additionalProperties: false,
  },
  payload_paths: [],
  clone_from: [],
  url_forms: [],
};

/** What `install_adapter` answers for [`SURVEYS`]'s manifest. */
function installAnswer() {
  return {
    adapter_kind: "wasm-fixture",
    adapter_version: "0.1.0",
    interface_version: "1.0.0",
    source_url: "/releases/knobas-adapter.json",
    tag: null,
    artifact_sha256: "a".repeat(64),
    hosts: ["{base_url}", "api.example.com", "{mirror}"],
    template: SURVEYS,
  };
}

/** What `preview_install_adapter` answers for [`SURVEYS`]'s manifest (#2031). */
function previewAnswer(): AdapterPreview {
  return {
    adapter_kind: "wasm-fixture",
    name: "Harbour surveys",
    adapter_version: "0.1.0",
    interface_version: "1.0.0",
    source_url: "/releases/knobas-adapter.json",
    tag: null,
    artifact_sha256: "a".repeat(64),
    confirm_sha256: "c".repeat(64),
    hosts: ["{base_url}", "api.example.com", "{mirror}"],
    roles: [
      { role: "wasm-fixture:survey-desk", reads: [] },
      { role: "ci", reads: ["run_log"] },
    ],
    limits: { deadline_secs: 30, deadline_asked: true, memory_mib: 256, memory_asked: false },
  };
}

/** The dialog over Add a source: *Install <name>?* (#2031). */
function confirmDialog(): HTMLElement {
  const found = [...target.querySelectorAll<HTMLElement>('.tf-dialog[role="dialog"]')];
  if (found.length < 2) throw new Error("no dialog over the dialog");
  return found.at(-1)!;
}

function confirmButton(text: string): HTMLButtonElement {
  const found = [...confirmDialog().querySelectorAll("button")].find((el) => el.textContent?.trim() === text);
  if (!found) throw new Error(`no button "${text}" in the dialog over the dialog`);
  return found;
}

/** A source of [`SURVEYS`]'s kind, as `list_sources` lists it. */
const HARBOUR = summary("harbour", "Harbour", "ok", {
  adapter_kind: "wasm-fixture",
  base_url: "https://surveys.harbour.example",
  auth_kind: "Pat",
  config: { fault: "none" },
  kinds: SURVEYS.entity_kinds,
});

/** Settings mounted with props a test can move, as the shell moves `rest` on a hash change. */
async function openMovable(rest: string[], p: SettingsPorts) {
  const props = $state({ rest, job: null, ready: true, ports: p, health: createHealthStub(), layers: createLayers(), now: () => NOW });
  view = mount(TfSettings, { target, props });
  await settle();
  return props;
}

test("a source of an adapter just installed in Add a source opens with its descriptor, without leaving Settings (#1406)", async () => {
  const p = ports();
  const jira = (await p.listAdapters())[0]!;
  let installed = false;
  let added = false;
  p.listAdapters = vi.fn(() => Promise.resolve(installed ? [jira, SURVEYS] : [jira]));
  p.previewInstallAdapter = vi.fn(() => Promise.resolve(previewAnswer()));
  p.installAdapter = vi.fn(() => {
    installed = true;
    return Promise.resolve(installAnswer());
  });
  p.addSource = vi.fn(() => {
    added = true;
    return Promise.resolve(HARBOUR);
  });
  p.listSources = vi.fn(() => Promise.resolve(added ? [...SOURCES, HARBOUR] : SOURCES));
  const props = await openMovable([], p);

  button("+ Add a source").click();
  await settle();
  type("#tf-add-manifest", "/releases/knobas-adapter.json");
  dialogButton("Install…").click();
  await settle();
  confirmButton("Install").click();
  await settle();
  type("#tf-add-id", "harbour");
  type("#tf-add-address", "https://surveys.harbour.example");
  type("#tf-add-secret", "s3cret");
  dialogButton("Test connection").click();
  await settle();
  dialogButton("Add source").click();
  await settle();
  expect(p.addSource).toHaveBeenCalledOnce();

  props.rest = ["source", "harbour"];
  await settle();
  expect(target.querySelector(".tf-stg-srch p")?.textContent).toContain("Harbour surveys");
  expect(section("What it syncs").querySelector("#tf-src-cfg-fault")).not.toBeNull();
});

test("an Update that changes an adapter's schema shows the new fields on its source's page (#1406)", async () => {
  const deeper = {
    ...SURVEYS,
    adapter_version: "0.2.0",
    config_schema: {
      ...SURVEYS.config_schema,
      properties: { ...SURVEYS.config_schema.properties, depth: { type: "string" } },
    },
  };
  let updated = false;
  const p = ports({
    listSources: vi.fn(() => Promise.resolve([...SOURCES, HARBOUR])),
    listAdapters: vi.fn(() => Promise.resolve([updated ? deeper : SURVEYS])),
    listInstalledAdapters: vi.fn(() =>
      Promise.resolve([adapterRow({ update: { state: "available", tag: "v0.2.0", adapter_version: "0.2.0" } })]),
    ),
    previewUpdateAdapter: vi.fn(() => Promise.resolve(updatePreview())),
    updateAdapter: vi.fn(() => {
      updated = true;
      return Promise.resolve(adapterRow({ tag: "v0.2.0", adapter_version: "0.2.0" }));
    }),
  });
  const props = await openMovable(["source", "harbour"], p);
  expect(section("What it syncs").querySelector("#tf-src-cfg-depth")).toBeNull();

  props.rest = [];
  await settle();
  button("Update").click();
  await settle();
  props.rest = ["source", "harbour"];
  await settle();
  expect(section("What it syncs").querySelector("#tf-src-cfg-fault")).not.toBeNull();
  expect(section("What it syncs").querySelector("#tf-src-cfg-depth")).not.toBeNull();
});

test("an Uninstall takes the adapter out of what Settings offers (#1406)", async () => {
  let uninstalled = false;
  const p = ports({
    listSources: vi.fn(() => Promise.resolve(uninstalled ? SOURCES : [...SOURCES, HARBOUR])),
    listAdapters: vi.fn(async () => [...(await ports().listAdapters()), ...(uninstalled ? [] : [SURVEYS])]),
    listInstalledAdapters: vi.fn(() =>
      Promise.resolve(uninstalled ? [] : [adapterRow({ sources: [{ id: "harbour", display_name: "Harbour" }] })]),
    ),
    uninstallAdapter: vi.fn(() => {
      uninstalled = true;
      return Promise.resolve();
    }),
  });
  await openMovable([], p);
  button("Uninstall…").click();
  flushSync();
  dialogButton("Uninstall").click();
  await settle();

  button("+ Add a source").click();
  await settle();
  const radios = [...dialog().querySelectorAll<HTMLLabelElement>(".tf-radio label")];
  expect(radios.map((label) => label.querySelector("span")?.firstChild?.textContent)).toEqual(["Jira Data Center"]);
});

test("Add a source shows an adapter's roles, hosts and limits before it installs (#2031)", async () => {
  const p = ports({
    previewInstallAdapter: vi.fn(() => Promise.resolve(previewAnswer())),
    installAdapter: vi.fn(() => Promise.resolve(installAnswer())),
  });
  await open([], p);
  button("+ Add a source").click();
  await settle();
  type("#tf-add-manifest", "  /releases/knobas-adapter.json ");
  dialogButton("Install…").click();
  await settle();

  expect(p.previewInstallAdapter).toHaveBeenCalledWith("/releases/knobas-adapter.json");
  expect(p.installAdapter).not.toHaveBeenCalled();
  expect(confirmDialog().querySelector("header h2")?.textContent?.trim()).toBe("Install Harbour surveys?");
  const facts = [...confirmDialog().querySelectorAll(".tf-facts dt")].map((dt) => [dt.textContent, dt.nextElementSibling?.textContent]);
  expect(facts).toEqual([
    ["Version", "0.1.0 · no release"],
    ["From", "/releases/knobas-adapter.json"],
    ["Roles", "wasm-fixture:survey-desk, CI"],
    ["Hosts", "It may reach this source's address, api.example.com and the host in its mirror setting, and no other host."],
    ["Limits", "30 s per call (asked for), 256 MiB of memory (the default)"],
  ]);
});

test("Cancel on Install <name>? installs nothing and offers nothing new (#2031)", async () => {
  const p = ports({ previewInstallAdapter: vi.fn(() => Promise.resolve(previewAnswer())) });
  await open([], p);
  button("+ Add a source").click();
  await settle();
  type("#tf-add-manifest", "/releases/knobas-adapter.json");
  dialogButton("Install…").click();
  await settle();
  confirmButton("Cancel").click();
  await settle();

  expect(p.installAdapter).not.toHaveBeenCalled();
  expect(target.querySelectorAll('.tf-dialog[role="dialog"]')).toHaveLength(1);
  expect(dialog().querySelectorAll(".tf-radio label")).toHaveLength(1);
  expect(dialog().querySelector<HTMLInputElement>("#tf-add-manifest")!.value).toBe("/releases/knobas-adapter.json");
});

test("Add a source installs an adapter from a manifest, and it joins the radios chosen (#1202)", async () => {
  const p = ports();
  const jira = (await p.listAdapters())[0]!;
  p.listAdapters = vi.fn().mockResolvedValueOnce([jira]).mockResolvedValue([jira, SURVEYS]);
  p.previewInstallAdapter = vi.fn(() => Promise.resolve(previewAnswer()));
  p.installAdapter = vi.fn(() => Promise.resolve(installAnswer()));
  await open([], p);
  button("+ Add a source").click();
  await settle();
  expect(dialogButton("Install…").disabled).toBe(true);

  type("#tf-add-manifest", "  /releases/knobas-adapter.json ");
  dialogButton("Install…").click();
  await settle();
  confirmButton("Install").click();
  await settle();

  // Held to the artifact the preview showed (#2031).
  expect(p.installAdapter).toHaveBeenCalledWith("/releases/knobas-adapter.json", "c".repeat(64));
  expect(target.querySelectorAll('.tf-dialog[role="dialog"]')).toHaveLength(1);
  expect(dialog().querySelector('[role="status"]#tf-add-manifest-note')?.textContent?.trim()).toBe(
    "Installed Harbour surveys 0.1.0. It may reach this source's address, api.example.com and the host in its mirror setting, and no other host.",
  );
  const radios = [...dialog().querySelectorAll<HTMLLabelElement>(".tf-radio label")];
  expect(radios.map((label) => label.querySelector("span")?.firstChild?.textContent)).toEqual([
    "Jira Data Center",
    "Harbour surveys",
  ]);
  expect(radios[1]!.querySelector("input")!.checked).toBe(true);
  // Its form is generated from its config schema, as a built-in's is.
  expect(dialog().querySelector("#tf-add-cfg-fault")).not.toBeNull();
  expect(dialog().querySelector<HTMLInputElement>("#tf-add-id")!.value).toBe("wasm-fixture");
});

test("Add a source shows a refused install with its reason, and offers nothing new (#1202)", async () => {
  const p = ports({
    previewInstallAdapter: vi.fn(() =>
      Promise.reject({
        code: "invalid",
        message:
          "install refused: the adapter's role claims are refused: role \"other:thing\" is neither a built-in role",
        source_id: null,
      }),
    ),
  });
  await open([], p);
  button("+ Add a source").click();
  await settle();
  type("#tf-add-manifest", "/releases/knobas-adapter.json");
  dialogButton("Install…").click();
  await settle();

  // Refused at the check (#2031): no Install <name>? to confirm, and nothing installed.
  expect(target.querySelectorAll('.tf-dialog[role="dialog"]')).toHaveLength(1);
  expect(p.installAdapter).not.toHaveBeenCalled();
  expect(dialog().querySelector('[role="alert"]#tf-add-manifest-note')?.textContent?.trim()).toBe(
    "Install refused: the adapter's role claims are refused: role \"other:thing\" is neither a built-in role",
  );
  expect(dialog().querySelectorAll(".tf-radio label")).toHaveLength(1);
  // The path stays, so a fixed manifest is one press away.
  expect(dialog().querySelector<HTMLInputElement>("#tf-add-manifest")!.value).toBe("/releases/knobas-adapter.json");
});

test("Add a source waits for a passing test of what is typed now", async () => {
  await open([]);
  button("+ Add a source").click();
  await settle();
  dialog().querySelector<HTMLInputElement>(".tf-radio input")!.click();
  flushSync();
  type("#tf-add-address", "https://jira2.tidewater.example");
  type("#tf-add-secret", "hunter2");
  dialogButton("Test connection").click();
  await settle();
  expect(dialogButton("Add source").disabled).toBe(false);
  type("#tf-add-address", "https://jira3.tidewater.example");
  expect(dialogButton("Add source").disabled).toBe(true);
});

test("the schedule and the pause switch are saved as they change", async () => {
  const { p } = await open(["source", "jira"]);
  const every = target.querySelector<HTMLSelectElement>("#tf-src-every")!;
  every.value = "900";
  every.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(p.updateSource).toHaveBeenLastCalledWith("jira", { sync_interval_secs: 900 });

  target.querySelector<HTMLButtonElement>('[role="switch"][aria-label="Syncing"]')!.click();
  await settle();
  expect(p.updateSource).toHaveBeenLastCalledWith("jira", { enabled: false });
  // A pause holds the sync, never a write the reader makes (#1438).
  expect(toasts.items.at(-1)?.text).toBe("Paused. Nothing syncs until you turn it on; writes you make still go.");
});

test("Backfill runs backfill_source for this source", async () => {
  const { p } = await open(["source", "jira"]);
  button("Backfill").click();
  await settle();
  expect(p.backfillSource).toHaveBeenCalledWith("jira");
});

test("Remove asks whether to keep what the source synced, and keeps it unless told otherwise", async () => {
  const { p } = await open(["source", "jira"]);
  button("Remove…").click();
  flushSync();
  expect(target.querySelector('[role="dialog"]')?.textContent).toContain("Keep what it synced");
  button("Remove").click();
  await settle();
  expect(p.deleteSource).toHaveBeenLastCalledWith("jira", false);
  expect(location.hash).toBe("#/settings");

  location.hash = "";
  unmount(view!);
  const second = await open(["source", "jira"]);
  button("Remove…").click();
  flushSync();
  const radios = target.querySelectorAll<HTMLInputElement>('[role="dialog"] input[type="radio"]');
  radios[1]!.click();
  flushSync();
  button("Remove").click();
  await settle();
  expect(second.p.deleteSource).toHaveBeenLastCalledWith("jira", true);
});

test("a failing credential is said on top, and Re-enter password stores the new one and syncs", async () => {
  const { p, health } = await open(["source", "teamcity"]);
  const alert = target.querySelector('[role="alert"]');
  expect(alert?.textContent).toContain(
    "Sign-in failed 38 min ago (401). Writes to TeamCity wait in the queue until you re-enter the password.",
  );

  button("Re-enter password").click();
  flushSync();
  expect(target.querySelector('[role="dialog"] h2')?.textContent).toBe("Re-enter the TeamCity password");
  expect(button("Sign in").disabled).toBe(true);
  type("#tf-reenter-secret", "hunter2");
  button("Sign in").click();
  await settle();

  expect(p.setSourceSecret).toHaveBeenCalledWith("teamcity", { value: "hunter2", account: null });
  expect(health.patched.at(-1)).toMatchObject({ source_id: "teamcity", state: "ok" });
  expect(p.syncNow).toHaveBeenCalledWith("teamcity");
  expect(target.querySelector('[role="dialog"]')).toBeNull();
});

/** The credential row's line, and the Re-enter dialog's note under it. */
async function storedLines(secretsInMemory: boolean): Promise<{ row: string; note: string }> {
  await open(["source", "teamcity"], ports({ appStatus: vi.fn(() => Promise.resolve({ secrets_in_memory: secretsInMemory })) }));
  const row = [...target.querySelectorAll(".tf-stg-row")].find((el) => el.querySelector(".tf-stg-l")?.textContent === "Password");
  const line = row?.querySelector(".tf-stg-c > span")?.textContent ?? "";
  button("Re-enter password").click();
  flushSync();
  const note = [...target.querySelectorAll('[role="dialog"] .tf-dlg-note')].map((el) => el.textContent ?? "").join(" ");
  return { row: line, note };
}

test("a source's credential is said to be in the system keychain off a Mac (#1585)", async () => {
  const { row, note } = await storedLines(false);
  // jsdom's user agent names no Mac; on one the line reads "the macOS keychain".
  expect(row).toBe("Stored in the system keychain");
  expect(note).toContain("Stored in the system keychain. Writes waiting for TeamCity go out once it signs in.");
});

test("under the memory secret store a source's credential is said to be kept for this session only (#1585)", async () => {
  const { row, note } = await storedLines(true);
  expect(row).toBe("Stored for this session only");
  expect(note).toContain("Stored for this session only. Writes waiting for TeamCity go out once it signs in.");
  expect(note).not.toContain("keychain");
});

/** Open Add a source over these adapters (Jira's descriptor from `ports()` is the template). */
async function addDialogOver(over: (jira: AdapterRow) => AdapterRow[], p: Partial<SettingsPorts> = {}) {
  const base = ports();
  const [jira] = (await base.listAdapters()) as AdapterRow[];
  const all = ports({ ...p, listAdapters: vi.fn(() => Promise.resolve(over(jira!) as never)) });
  await open([], all);
  button("+ Add a source").click();
  await settle();
  return all;
}

type AdapterRow = Awaited<ReturnType<SettingsPorts["listAdapters"]>>[number];

async function fillAndTest() {
  type("#tf-add-address", "https://jira2.tidewater.example");
  type("#tf-add-secret", "s3cret");
  dialogButton("Test connection").click();
  await settle();
}

test("Add a source: the secret is written nowhere but the requests (#1506)", async () => {
  const stored = new Map<string, string>();
  const storage = {
    getItem: (k: string) => stored.get(k) ?? null,
    setItem: (k: string, v: string) => void stored.set(k, String(v)),
    removeItem: (k: string) => void stored.delete(k),
    clear: () => stored.clear(),
    key: (i: number) => [...stored.keys()][i] ?? null,
    get length() {
      return stored.size;
    },
  };
  const real = { local: window.localStorage, session: window.sessionStorage };
  Object.defineProperty(window, "localStorage", { value: storage, configurable: true, writable: true });
  Object.defineProperty(window, "sessionStorage", { value: storage, configurable: true, writable: true });
  try {
    // The control: this dump can see a value that is stored, so a clean dump
    // below is a fact about the secret and not about the dump.
    storage.setItem("canary", "s3cret");
    expect(JSON.stringify([...stored])).toContain("s3cret");
    storage.clear();

    const p = await addDialogOver((jira) => [jira], { addSource: vi.fn(() => Promise.resolve(summary("jira", "Jira Data Center", "ok"))) });
    dialog().querySelector<HTMLInputElement>(".tf-radio input")!.click();
    flushSync();
    type("#tf-add-address", "https://jira2.tidewater.example");
    type("#tf-add-secret", "s3cret");
    // On screen the value lives in the input's property, not in an attribute.
    expect(target.innerHTML).not.toContain("s3cret");
    dialogButton("Test connection").click();
    await settle();
    dialogButton("Add source").click();
    await settle();

    expect(JSON.stringify([...stored])).not.toContain("s3cret");
    expect(target.innerHTML).not.toContain("s3cret");
    // Once in the test and once in the add, never a second round trip.
    expect(p.testSource).toHaveBeenCalledTimes(1);
    expect(p.addSource).toHaveBeenCalledTimes(1);
    expect(vi.mocked(p.addSource).mock.calls[0]![0].secret).toEqual({ value: "s3cret", account: null });
  } finally {
    Object.defineProperty(window, "localStorage", { value: real.local, configurable: true, writable: true });
    Object.defineProperty(window, "sessionStorage", { value: real.session, configurable: true, writable: true });
  }
});

test("Add a source: a failed save keeps the dialog open with its reason and the draft (#1506)", async () => {
  const p = await addDialogOver((jira) => [jira], {
    addSource: vi.fn(() => Promise.reject({ code: "conflict", message: "a source with id jira already exists", source_id: "jira" })),
  });
  await fillAndTest();
  dialogButton("Add source").click();
  await settle();

  expect(p.addSource).toHaveBeenCalledTimes(1);
  expect(dialog().querySelector('[role="alert"]')!.textContent).toContain("a source with id jira already exists");
  // A dialog that closed on failure would throw the whole draft away.
  expect(dialog().querySelector<HTMLInputElement>("#tf-add-address")!.value).toBe("https://jira2.tidewater.example");
  expect(dialog().querySelector<HTMLInputElement>("#tf-add-secret")!.value).toBe("s3cret");
  expect(p.listSources).toHaveBeenCalledTimes(1);
});

test("Add a source: choosing another adapter resets the config, so no field of the old one survives (#1506)", async () => {
  const p = await addDialogOver((jira) => [
    jira,
    { ...jira, id: "gitea", adapter_kind: "gitea", name: "Gitea", config_schema: GITEA_SCHEMA as never },
  ], { addSource: vi.fn(() => Promise.resolve(summary("gitea", "Gitea", "ok"))) });
  const radios = dialog().querySelectorAll<HTMLInputElement>(".tf-radio input");
  radios[0]!.click();
  flushSync();
  expect(dialog().querySelector("#tf-add-cfg-flavor")).not.toBeNull();
  type("#tf-add-cfg-sprint_field", "customfield_99");

  radios[1]!.click();
  flushSync();
  // Gitea's schema and nothing of Jira's: a config carrying `flavor` would be
  // written to a Gitea source's config column, where it means nothing.
  expect(dialog().querySelector("#tf-add-cfg-flavor")).toBeNull();
  expect(dialog().querySelector("#tf-add-cfg-owners")).not.toBeNull();
  expect(dialog().querySelector<HTMLInputElement>("#tf-add-id")!.value).toBe("gitea");

  await fillAndTest();
  dialogButton("Add source").click();
  await settle();
  const sent = vi.mocked(p.addSource).mock.calls[0]![0];
  expect(sent.adapter_kind).toBe("gitea");
  expect(sent.config).not.toHaveProperty("flavor");
  expect(sent.config).not.toHaveProperty("sprint_field");
});

test("Add a source: an adapter whose schema declares a secret is refused, not drawn (#1506)", async () => {
  await addDialogOver((jira) => [
    { ...jira, id: "broken", adapter_kind: "broken", name: "Broken Adapter", config_schema: { type: "object", properties: { api_token: { type: "string" } } } as never },
  ]);
  dialog().querySelector<HTMLInputElement>(".tf-radio input")!.click();
  flushSync();
  type("#tf-add-address", "https://x.example");
  type("#tf-add-secret", "s3cret");

  // Contract §3: a credential in the config column would be written to
  // Postgres. Saying so beats drawing a field that quietly does it.
  expect(dialog().textContent).toMatch(/secret/i);
  expect(dialog().querySelector("#tf-add-cfg-api_token")).toBeNull();
  expect(dialogButton("Test connection").disabled).toBe(true);
  expect(dialogButton("Add source").disabled).toBe(true);
});

test("Add a source: a discovered key the adapter does not declare never reaches the config (#1506)", async () => {
  // What enforces it is `validate`, which builds the config from the schema's
  // fields; `fillFound` skipping an undeclared key is a second layer this test
  // does not witness on its own.
  const p = await addDialogOver((jira) => [jira], {
    addSource: vi.fn(() => Promise.resolve(summary("jira", "Jira Data Center", "ok"))),
    testSource: vi.fn(() =>
      Promise.resolve({ ...GAP_REPORT, discovered: { not_a_property: "x", epic_link_field: "customfield_10101" } }),
    ),
  });
  dialog().querySelector<HTMLInputElement>(".tf-radio input")!.click();
  flushSync();
  await fillAndTest();
  dialogButton("Add source").click();
  await settle();

  const config = vi.mocked(p.addSource).mock.calls[0]![0].config as Record<string, unknown>;
  expect(config).toMatchObject({ epic_link_field: "customfield_10101" });
  expect(config).not.toHaveProperty("not_a_property");
});

test("a password the source refuses keeps the dialog open and says so", async () => {
  const refuse = ports({ setSourceSecret: vi.fn((id: string) => Promise.resolve(healthOf(id, "unauthorized"))) });
  await open(["source", "teamcity"], refuse);
  button("Re-enter password").click();
  flushSync();
  type("#tf-reenter-secret", "wrong");
  button("Sign in").click();
  await settle();
  expect(target.querySelector('[role="dialog"]')?.textContent).toContain("TeamCity refused it.");
  expect(refuse.syncNow).not.toHaveBeenCalled();
});

test("Test connection tests what is stored and says what it found", async () => {
  const { p } = await open(["source", "jira"]);
  type("#tf-src-address", "https://typed.tidewater.example");
  button("Test connection").click();
  await settle();
  expect(p.testSource).toHaveBeenCalledWith(
    expect.objectContaining({ source_id: "jira", base_url: "https://jira.tidewater.example", secret: null }),
  );
  expect(target.querySelector(".tf-stg-note")?.textContent).toContain("Found when tested: Connected as mara · 9.12.4 · 214 ms");
});

const GAP_REPORT = {
  ok: true,
  account: "mara",
  server_version: "9.12.4",
  secret_expires_at: null,
  error: null,
  code: null,
  elapsed_ms: 214,
  detail: "Epic Link customfield_10101 found but not configured: epic membership is not mirrored",
  discovered: { epic_link_field: "customfield_10101" },
};

async function testInAddDialog(prefillEpic: string | null) {
  const p = ports({ testSource: vi.fn(() => Promise.resolve(GAP_REPORT)) });
  await open([], p);
  button("+ Add a source").click();
  await settle();
  dialog().querySelector<HTMLInputElement>(".tf-radio input")!.click();
  flushSync();
  type("#tf-add-address", "https://jira2.tidewater.example");
  type("#tf-add-secret", "hunter2");
  if (prefillEpic !== null) type("#tf-add-cfg-epic_link_field", prefillEpic);
  dialogButton("Test connection").click();
  await settle();
  return p;
}

test("Add a source: a filled Epic Link reads found and set, not found but not configured (#1479)", async () => {
  await testInAddDialog(null);
  const text = dialog().querySelector('[role="status"]')!.textContent!;
  expect(text).toContain("Epic Link customfield_10101 found and set");
  expect(text).not.toContain("not configured");
  expect(dialog().querySelector<HTMLInputElement>("#tf-add-cfg-epic_link_field")!.value).toBe("customfield_10101");
  // A second test keeps one id: nothing is written into a field that has one.
  dialogButton("Test connection").click();
  await settle();
  expect(dialog().querySelector<HTMLInputElement>("#tf-add-cfg-epic_link_field")!.value).toBe("customfield_10101");
});

test("Add a source: an Epic Link typed before the test leaves the report's own note (#1479)", async () => {
  await testInAddDialog("customfield_77");
  expect(dialog().querySelector<HTMLInputElement>("#tf-add-cfg-epic_link_field")!.value).toBe("customfield_77");
  expect(dialog().querySelector('[role="status"]')!.textContent).toContain("found but not configured");
});

test("a saved source's Test keeps found but not configured (#1479)", async () => {
  const p = ports({ testSource: vi.fn(() => Promise.resolve(GAP_REPORT)) });
  await open(["source", "jira"], p);
  button("Test connection").click();
  await settle();
  expect(target.querySelector(".tf-stg-note")?.textContent).toContain("found but not configured");
});

test("a source's page shows its runs, and Checkouts draws its own section", async () => {
  await open(["source", "teamcity"]);
  expect(target.querySelector(".tf-tbl tbody td.tf-stg-errt")?.textContent).toBe(
    "401 Unauthorized. Re-enter the password.",
  );
  expect(target.querySelector(".tf-crumbs strong")?.textContent).toBe("TeamCity");
  unmount(view!);

  view = mount(TfSettings, {
    target,
    props: { rest: ["checkouts"], job: null, ready: true, ports: ports(), health: createHealthStub(), layers: createLayers() },
  });
  await settle();
  // Built since #844: its own section.
  expect(target.querySelector(".tf-stg-body h1")?.textContent).toBe("Checkouts and Open in…");
});

test("Esc closes a dialog through the shell's layer stack", async () => {
  const layers = createLayers();
  view = mount(TfSettings, {
    target,
    props: { rest: ["source", "jira"], job: null, ready: true, ports: ports(), health: createHealthStub(), layers, now: () => NOW },
  });
  await settle();
  button("Remove…").click();
  flushSync();
  expect(layers.open.map((layer) => layer.kind)).toEqual(["dialog"]);
  layers.open[0]!.close();
  flushSync();
  expect(target.querySelector('[role="dialog"]')).toBeNull();
  expect(layers.open).toEqual([]);
});

test("a schedule the backend refuses goes back to the stored one", async () => {
  await open(["source", "jira"], ports({ updateSource: vi.fn(() => Promise.reject({ code: "invalid", message: "no", source_id: "jira" })) }));
  const every = target.querySelector<HTMLSelectElement>("#tf-src-every")!;
  every.value = "900";
  every.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(every.value).toBe("300");
});

// ---- The adapter's sync-interval floor (#1188) ----

/** An adapter template declaring `floor`, so the dropdown and the dialog read it and know no kind. */
function floored(kind: string, name: string, floor: number) {
  return {
    id: kind,
    adapter_kind: kind,
    name,
    capabilities: [],
    adapter_version: "1",
    auth_methods: ["ApiToken" as const],
    accepts_account: false,
    min_sync_interval_secs: floor,
    roles: [],
    source_actions: [],
    clone_from: [],
    url_forms: [],
    write_ops: [],
    entity_kinds: [ticket],
    config_schema: { type: "object", properties: {} },
    payload_paths: [],
  };
}

/** The adapters as the ticket names them; the kinds are only names here, the floors are what is read. */
const FLOORED = [
  floored("kuma", "Uptime Kuma", 45),
  floored("teamcity", "TeamCity", 20),
  floored("jira", "Jira Data Center", 60),
  floored("confluence", "Confluence", 60),
  floored("gitea", "Gitea", 60),
];

function flooredPorts(sources: SourceSummary[], over: Partial<SettingsPorts> = {}) {
  return ports({
    listSources: vi.fn(() => Promise.resolve(sources)),
    listAdapters: vi.fn(() => Promise.resolve(FLOORED)),
    ...over,
  });
}

function everyOptions(): string[] {
  return [...target.querySelectorAll<HTMLOptionElement>("#tf-src-every option")].map((o) => o.textContent!.trim());
}

test("a source page's schedule starts at its adapter's floor (#1188)", async () => {
  const expected: Record<string, string[]> = {
    kuma: ["45 s", "1 min", "5 min", "15 min", "30 min", "1 hour"],
    teamcity: ["20 s", "30 s", "45 s", "1 min", "5 min", "15 min", "30 min", "1 hour"],
    jira: ["1 min", "5 min", "15 min", "30 min", "1 hour"],
    confluence: ["1 min", "5 min", "15 min", "30 min", "1 hour"],
    gitea: ["1 min", "5 min", "15 min", "30 min", "1 hour"],
  };
  const sources = FLOORED.map((a) => summary(a.adapter_kind, a.name, "ok"));
  for (const [kind, options] of Object.entries(expected)) {
    await open(["source", kind], flooredPorts(sources));
    expect(everyOptions(), kind).toEqual(options);
    unmount(view!);
    view = null;
  }
});

test("a stored schedule off the ladder is still offered and selected (#1188)", async () => {
  await open(["source", "teamcity"], flooredPorts([summary("teamcity", "TeamCity", "ok", { sync_interval_secs: 25 })]));
  expect(everyOptions()).toEqual(["20 s", "25 s", "30 s", "45 s", "1 min", "5 min", "15 min", "30 min", "1 hour"]);
  expect(target.querySelector<HTMLSelectElement>("#tf-src-every")!.value).toBe("25");
});

test("a 20-second source reads Syncs every 20 s (#1188)", async () => {
  await open(["source", "teamcity"], flooredPorts([summary("teamcity", "TeamCity", "ok", { sync_interval_secs: 20 })]));
  expect(target.textContent).toContain("Syncs every 20 s");
  expect(target.textContent).not.toMatch(/Syncs every 0 min/);
});

test("Add a source pre-fills a floor under a minute, else 15 minutes (#1188)", async () => {
  const expected: Record<string, number> = { kuma: 45, teamcity: 20, jira: 900, confluence: 900, gitea: 900 };
  for (const [kind, secs] of Object.entries(expected)) {
    const p = flooredPorts([], {
      addSource: vi.fn(() => Promise.resolve(summary(`${kind}-2`, kind, "ok"))),
    });
    await open([], p);
    button("+ Add a source").click();
    await settle();
    const radio = [...dialog().querySelectorAll<HTMLLabelElement>(".tf-radio label")].find((label) =>
      label.textContent!.includes(FLOORED.find((a) => a.adapter_kind === kind)!.name),
    )!;
    radio.querySelector("input")!.click();
    flushSync();
    type("#tf-add-id", `${kind}-2`);
    type("#tf-add-address", "https://example.invalid");
    type("#tf-add-secret", "hunter2");
    dialogButton("Test connection").click();
    await settle();
    dialogButton("Add source").click();
    await settle();
    expect(p.addSource, kind).toHaveBeenCalledWith(expect.objectContaining({ adapter_kind: kind, sync_interval_secs: secs }));
    unmount(view!);
    view = null;
  }
});

test("a source whose records missed a role's path says so on its row and its page (#1199)", async () => {
  const miss = { role: "ticket-system", kind: "ticket", field: "priority", path: "fields.priority.name" };
  const missing = [summary("jira", "Jira", "ok", { role_misses: [miss] })];
  await open([], ports({ listSources: vi.fn(() => Promise.resolve(missing)) }));
  expect(target.querySelector(".tf-stg-src")?.textContent).toContain("ticket system not drawn (fields.priority.name)");
  unmount(view!);
  view = null;

  await open(["source", "jira"], ports({ listSources: vi.fn(() => Promise.resolve(missing)) }));
  expect(target.querySelector('.tf-stg-alert[role="status"]')?.textContent).toBe(
    "Not drawn as a ticket system: no ticket resolves priority at fields.priority.name. Its tickets stay searchable and linkable.",
  );
});

// -- Installed adapters (#1209, #1210) ------------------------------------------------

function adapterRow(over: Partial<AdapterStatus> = {}): AdapterStatus {
  return {
    adapter_kind: "wasm-fixture",
    name: "Harbour surveys",
    adapter_version: "0.1.0",
    interface_version: "1.0.0",
    source_url: "https://github.test/harbour/surveys",
    tag: "v0.1.0",
    artifact_sha256: "5".repeat(64),
    hosts: ["{base_url}"],
    sources: [],
    refused: null,
    update: null,
    ...over,
  };
}

/** The update's preview: `release` laid over the installed release's facts (#2031). */
function updatePreview(release: Partial<AdapterPreview> = {}): UpdatePreview {
  const installed: AdapterPreview = {
    ...previewAnswer(),
    source_url: "https://github.test/harbour/surveys",
    tag: "v0.1.0",
    artifact_sha256: "5".repeat(64),
    confirm_sha256: "7".repeat(64),
    hosts: ["{base_url}"],
  };
  return {
    installed,
    release: {
      ...installed,
      tag: "v0.2.0",
      adapter_version: "0.2.0",
      artifact_sha256: "6".repeat(64),
      confirm_sha256: "8".repeat(64),
      ...release,
    },
  };
}

function adaptersSection(): HTMLElement {
  const found = target.querySelector<HTMLElement>('section[aria-labelledby="tf-ext-h"]');
  if (!found) throw new Error("no Installed adapters section");
  return found;
}

test("Installed adapters is drawn only once an adapter is installed (#1209)", async () => {
  await open([]);
  expect(target.querySelector('section[aria-labelledby="tf-ext-h"]')).toBeNull();
});

test("an update available is said, and nothing updates until Update is pressed (#1209)", async () => {
  const p = ports({
    listInstalledAdapters: vi.fn(() =>
      Promise.resolve([adapterRow({ update: { state: "available", tag: "v0.2.0", adapter_version: "0.2.0" } })]),
    ),
    previewUpdateAdapter: vi.fn(() => Promise.resolve(updatePreview())),
    updateAdapter: vi.fn(() => Promise.resolve(adapterRow({ tag: "v0.2.0", adapter_version: "0.2.0" }))),
  });
  await open([], p);
  expect(adaptersSection().textContent).toContain("Harbour surveys 0.1.0");
  expect(adaptersSection().textContent).toContain("v0.1.0 · https://github.test/harbour/surveys");
  expect(adaptersSection().textContent).toContain("Update available: v0.2.0");
  expect(p.updateAdapter).not.toHaveBeenCalled();

  button("Update").click();
  await settle();
  // The same hosts, roles and limits: nothing to ask, and the switch is held to what was checked (#2031).
  expect(target.querySelector('.tf-dialog[role="dialog"]')).toBeNull();
  expect(p.updateAdapter).toHaveBeenCalledWith("wasm-fixture", "8".repeat(64));
  expect(adaptersSection().textContent).toContain("Harbour surveys 0.2.0");
  expect(adaptersSection().textContent).not.toContain("Update available");
});

test("an update that may reach more hosts or claims more roles says what changed and waits for Update (#2031)", async () => {
  const p = ports({
    listInstalledAdapters: vi.fn(() =>
      Promise.resolve([adapterRow({ update: { state: "available", tag: "v0.2.0", adapter_version: "0.2.0" } })]),
    ),
    previewUpdateAdapter: vi.fn(() =>
      Promise.resolve(
        updatePreview({
          hosts: ["{base_url}", "archive.harbour.example"],
          roles: [...previewAnswer().roles, { role: "ticket-system", reads: [] }],
          limits: { deadline_secs: 30, deadline_asked: true, memory_mib: 512, memory_asked: true },
        }),
      ),
    ),
    updateAdapter: vi.fn(() => Promise.resolve(adapterRow({ tag: "v0.2.0", adapter_version: "0.2.0" }))),
  });
  await open([], p);
  button("Update").click();
  await settle();

  expect(p.updateAdapter).not.toHaveBeenCalled();
  expect(dialog().querySelector("header h2")?.textContent).toBe("Update Harbour surveys to v0.2.0?");
  expect([...dialog().querySelectorAll(".tf-ext-changes li")].map((li) => li.textContent)).toEqual([
    "It may newly reach archive.harbour.example.",
    "It newly claims ticket system.",
    "Each call may use 512 MiB of memory, was 256 MiB.",
  ]);
  expect(dialog().textContent).toContain("Harbour surveys stays on v0.1.0 until you press Update.");

  dialogButton("Cancel").click();
  await settle();
  expect(p.updateAdapter).not.toHaveBeenCalled();
  expect(adaptersSection().textContent).toContain("Harbour surveys 0.1.0");

  button("Update").click();
  await settle();
  dialogButton("Update").click();
  await settle();
  expect(p.updateAdapter).toHaveBeenCalledExactlyOnceWith("wasm-fixture", "8".repeat(64));
  expect(adaptersSection().textContent).toContain("Harbour surveys 0.2.0");
});

test("a refused update says why and that the installed version stays (#1209)", async () => {
  const reason = "install refused: the manifest asks for knobas:adapter@2.0.0, and this knobas hosts knobas:adapter@1.x up to 1.0.0";
  const before = adapterRow({ update: { state: "available", tag: "v0.2.0", adapter_version: "0.2.0" } });
  const after = adapterRow({ update: { state: "refused", tag: "v0.2.0", reason } });
  const listInstalledAdapters = vi.fn().mockResolvedValueOnce([before]).mockResolvedValue([after]);
  const p = ports({
    listInstalledAdapters,
    previewUpdateAdapter: vi.fn(() => Promise.reject({ code: "invalid", message: reason, source_id: null })),
  });
  await open([], p);
  button("Update").click();
  await settle();
  expect(adaptersSection().textContent).toContain(`Update to v0.2.0 refused, still on v0.1.0: ${reason}`);
  expect(adaptersSection().querySelector(".tf-stg-errt")).not.toBeNull();
});

test("Check for updates asks the backend and draws what it found (#1209)", async () => {
  const p = ports({
    listInstalledAdapters: vi.fn(() => Promise.resolve([adapterRow()])),
    checkAdapterUpdates: vi.fn(() =>
      Promise.resolve([adapterRow({ update: { state: "available", tag: "v0.3.0", adapter_version: "0.3.0" } })]),
    ),
  });
  await open([], p);
  expect(adaptersSection().textContent).not.toContain("Update available");
  button("Check for updates").click();
  await settle();
  expect(p.checkAdapterUpdates).toHaveBeenCalledTimes(1);
  expect(adaptersSection().textContent).toContain("Update available: v0.3.0");
});

test("an adapter refused at restore says why (#1210)", async () => {
  const refused = "the release's artifact SHA-256 is now 6666, and the backup pinned 5555";
  await open([], ports({ listInstalledAdapters: vi.fn(() => Promise.resolve([adapterRow({ refused })])) }));
  expect(adaptersSection().textContent).toContain(`Refused: ${refused}`);
  expect(() => button("Update")).toThrow();
});

test("Uninstall asks once per source and sends each answer (#1210)", async () => {
  const p = ports({
    listInstalledAdapters: vi.fn(() =>
      Promise.resolve([
        adapterRow({
          sources: [
            { id: "quay-a", display_name: "Quay A" },
            { id: "quay-b", display_name: "Quay B" },
          ],
        }),
      ]),
    ),
  });
  await open([], p);
  button("Uninstall…").click();
  flushSync();
  const groups = [...dialog().querySelectorAll('[role="radiogroup"]')];
  expect(groups.map((g) => g.getAttribute("aria-label"))).toEqual(["What Quay A synced", "What Quay B synced"]);
  // Keep is each one's default; delete Quay B's.
  groups[1]!.querySelectorAll<HTMLInputElement>('input[type="radio"]')[1]!.click();
  flushSync();
  dialogButton("Uninstall").click();
  await settle();
  expect(p.uninstallAdapter).toHaveBeenCalledWith("wasm-fixture", [
    { source_id: "quay-a", purge_items: false },
    { source_id: "quay-b", purge_items: true },
  ]);
  expect(target.querySelector('section[aria-labelledby="tf-ext-h"]')).toBeNull();
  // The sources it removed leave the list.
  expect(p.listSources).toHaveBeenCalledTimes(2);
});

test("an uninstall that fails part-way reloads the sources it may have removed (#1210)", async () => {
  const p = ports({
    listInstalledAdapters: vi.fn(() =>
      Promise.resolve([adapterRow({ sources: [{ id: "quay-a", display_name: "Quay A" }] })]),
    ),
    uninstallAdapter: vi.fn(() => Promise.reject({ kind: "internal", message: "cannot remove quay-b" })),
  });
  await open([], p);
  button("Uninstall…").click();
  flushSync();
  dialogButton("Uninstall").click();
  await settle();
  expect(p.listInstalledAdapters).toHaveBeenCalledTimes(2);
  expect(p.listSources).toHaveBeenCalledTimes(2);
});

// ---- The run in flight on a source's page (#1384) ----

/** `jira`'s status while run `runId` goes, as `sync_status` and `sync:state` say it. */
function runningStatus(runId: number): SourceSyncStatus {
  return {
    source_id: "jira",
    running: true,
    run_id: runId,
    started_at: AGO(2),
    last_finished_at: AGO(4),
    last_outcome: "ok",
    next_run_at: null,
    backoff_until: null,
    last_changed: null,
  };
}

/** One message on a watched run's channel. */
function progress(runId: number, over: Partial<SyncProgress>): SyncProgress {
  return { run_id: runId, source_id: "jira", phase: "fetching", items: 0, elapsed_ms: 0, message: null, ...over };
}

/** The page's progress reading, or `null` when none is drawn. */
function runReading(): string | null {
  return target.querySelector(".tf-stg-run p")?.textContent?.replace(/\s+/g, " ").trim() ?? null;
}

/** The note beside the config *Save* about a value the address added (#1404), or `null`. */
function addedNote(): string | null {
  return section("What it syncs").querySelector(".tf-stg-acts .tf-stg-pend")?.textContent?.trim() ?? null;
}

/** The Result column of *Sync runs*, newest first. */
function runResults(): string[] {
  const section = [...target.querySelectorAll("section")].find((el) => el.querySelector("h2")?.textContent === "Sync runs");
  return [...(section?.querySelectorAll("tbody tr td:nth-child(3)") ?? [])].map((td) => td.textContent ?? "");
}

/** `watch_sync` as a stand-in: every channel and stop signal the page hands it, and the run it answers. */
function watching(answer: number | null) {
  const channels: ((message: SyncProgress) => void)[] = [];
  const stops: AbortSignal[] = [];
  const watchSync = vi.fn((_: string, onmessage: (message: SyncProgress) => void, stop: AbortSignal) => {
    channels.push(onmessage);
    stops.push(stop);
    return Promise.resolve(answer);
  });
  return { channels, stops, watchSync };
}

test("a source's page opened mid-run draws the walk, and the run's row fills in when it ends (#1384)", async () => {
  const runs: SyncRunRow[] = [
    { ...SOURCES[0]!.last_run!, id: 7, trigger: "schedule", started_at: AGO(2), finished_at: null, outcome: null, upserted: 0 },
    SOURCES[0]!.last_run!,
  ];
  const { channels, stops, watchSync } = watching(7);
  await open(
    ["source", "jira"],
    ports({
      syncStatus: vi.fn(() => Promise.resolve([runningStatus(7)])),
      listSyncRuns: vi.fn(() => Promise.resolve(runs.map((run) => ({ ...run })))),
      watchSync,
    }),
  );
  expect(watchSync).toHaveBeenCalledTimes(1);
  expect(watchSync.mock.calls[0]?.[0]).toBe("jira");
  expect(button("Sync now").disabled).toBe(true);
  expect(runResults()[0]).toBe("Running…");

  channels[0]!(progress(7, { items: 1240, elapsed_ms: 83_000, walked: 120, of: 300, unit: "repositories" }));
  await settle();
  expect(runReading()).toBe("120 of 300 repositories · 1,240 items · 83 s");
  const bar = target.querySelector<HTMLProgressElement>(".tf-stg-run progress")!;
  expect([bar.max, bar.value]).toEqual([300, 120]);

  // A message without a reading keeps the last one: the writing phase sends none.
  channels[0]!(progress(7, { phase: "writing", items: 1302, elapsed_ms: 91_400 }));
  await settle();
  expect(runReading()).toBe("120 of 300 repositories · 1,302 items · 91 s");

  // The ending: the reading goes, and the runs are read again with the row filled in.
  runs[0] = { ...runs[0]!, finished_at: AGO(0), outcome: "ok", upserted: 1302 };
  channels[0]!(progress(7, { phase: "finished", items: 1302, elapsed_ms: 95_000 }));
  await settle();
  expect(runReading()).toBeNull();
  expect(runResults()[0]).toBe("1,302 updated");
  expect(stops[0]?.aborted, "the ended run's channel is still listened to").toBe(true);
});

test("a run that starts while the page is open is watched, and leaving the page stops listening (#1384)", async () => {
  let syncState: ((event: { payload: unknown }) => void) | null = null;
  const { channels, stops, watchSync } = watching(8);
  const p = ports({
    watchSync,
    listen: vi.fn((event: string, handler: (event: { payload: unknown }) => void) => {
      if (event === "sync:state") syncState = handler;
      return Promise.resolve(() => {});
    }),
  });
  const props = $state({
    rest: ["source", "jira"],
    job: null,
    ready: true,
    ports: p,
    health: createHealthStub(),
    layers: createLayers(),
    now: () => NOW,
  });
  view = mount(TfSettings, { target, props });
  await settle();
  expect(watchSync, "an idle source is not watched").not.toHaveBeenCalled();
  expect(runReading()).toBeNull();

  syncState!({ payload: runningStatus(8) });
  await settle();
  expect(watchSync).toHaveBeenCalledTimes(1);
  channels[0]!(progress(8, { items: 3, elapsed_ms: 1_000, walked: 1, of: 4, unit: "projects" }));
  await settle();
  expect(runReading()).toBe("1 of 4 projects · 3 items · 1 s");

  // Leave for the list and come back: the page that left hears nothing more,
  // and the page that came back watches the run again on a channel of its own.
  props.rest = [];
  await settle();
  expect(stops[0]?.aborted, "the page left and still listens to the run").toBe(true);
  props.rest = ["source", "jira"];
  await settle();
  expect(watchSync).toHaveBeenCalledTimes(2);
  channels[0]!(progress(8, { items: 999, elapsed_ms: 2_000, walked: 3, of: 4, unit: "projects" }));
  await settle();
  expect(runReading() ?? "", "the left page's channel still reaches the page").not.toContain("999");
  channels[1]!(progress(8, { items: 5, elapsed_ms: 2_000, walked: 2, of: 4, unit: "projects" }));
  await settle();
  expect(runReading()).toBe("2 of 4 projects · 5 items · 2 s");
  expect(stops[1]?.aborted).toBe(false);
});

test("a watch that finds nothing while the run's id arrives asks again for that run (#1384)", async () => {
  // A local *Sync now* marks the source running with no id; the page asks
  // before the trigger lands, and `sync:state` with the id arrives before the
  // empty answer does. The page must ask again for the run the event named.
  let syncState: ((event: { payload: unknown }) => void) | null = null;
  const answers: ((run: number | null) => void)[] = [];
  const watchSync = vi.fn(
    () =>
      new Promise<number | null>((resolve) => {
        answers.push(resolve);
      }),
  );
  const p = ports({
    watchSync,
    listen: vi.fn((event: string, handler: (event: { payload: unknown }) => void) => {
      if (event === "sync:state") syncState = handler;
      return Promise.resolve(() => {});
    }),
  });
  view = mount(TfSettings, {
    target,
    props: {
      rest: ["source", "jira"],
      job: null,
      ready: true,
      ports: p,
      health: createHealthStub(),
      layers: createLayers(),
      now: () => NOW,
    },
  });
  await settle();

  syncState!({ payload: { ...runningStatus(9), run_id: null } });
  await settle();
  expect(watchSync).toHaveBeenCalledTimes(1);
  syncState!({ payload: runningStatus(9) });
  await settle();
  expect(watchSync, "the event's status attached a second time while the first ask was out").toHaveBeenCalledTimes(1);

  answers[0]!(null);
  await settle();
  expect(watchSync, "run 9 is going and nobody watches it").toHaveBeenCalledTimes(2);
  answers[1]!(9);
  await settle();
  expect(watchSync).toHaveBeenCalledTimes(2);
});

test("a token source with no token stored is never told about a password (#1405)", async () => {
  // The fake fixture's Uptime Kuma (`shell/dev/fake/sources.ts`, which a test
  // here may not import): an API token, none stored, its last run fine.
  const kuma = summary("kuma", "Uptime Kuma", "missing_secret", { auth_kind: "ApiToken", config: {} });
  const listSources = vi.fn(() => Promise.resolve([...SOURCES, kuma]));
  const listSyncRuns = vi.fn((id: string) => Promise.resolve([kuma.last_run!].filter((run) => run.source_id === id)));
  await open(["source", "kuma"], ports({ listSources, listSyncRuns }));

  const alert = target.querySelector('[role="alert"]')!;
  expect(alert.textContent).toContain("No token is stored for Uptime Kuma.");
  expect(button("Re-enter token")).toBeTruthy();
  expect(target.textContent).not.toMatch(/password/i);
  unmount(view!);
  view = null;

  await open([], ports({ listSources, listSyncRuns }));
  const row = [...target.querySelectorAll(".tf-stg-src")].find((el) => el.textContent?.includes("Uptime Kuma"))!;
  expect(row.querySelectorAll(".tf-stg-si")[1]?.textContent?.trim()).toBe("No token stored");
});

test("an unreachable run says the source did not answer, its transport error behind Details, and wraps (#1478)", async () => {
  // As the fake fixture's TeamCity has it: one older run the server never answered.
  const transport =
    "source: unreachable: error sending request for url (https://ci.tidewater.example/app/rest/builds?locator=defaultFilter%3Afalse%2Ccount%3A100%2CsinceBuild%3A(id%3A48211)%2Cbranch%3A(default%3Aany)&fields=build(id%2CbuildTypeId%2Cnumber%2Cstatus%2Cstate%2CbranchName%2CstartDate%2CfinishDate))";
  const teamcity = SOURCES[1]!;
  const unanswered: SyncRunRow = { ...teamcity.last_run!, id: 88, outcome: "unreachable", upserted: 0, error: transport };
  const listSyncRuns = vi.fn((id: string) =>
    Promise.resolve(id === "teamcity" ? [teamcity.last_run!, unanswered] : []),
  );
  await open(["source", "teamcity"], ports({ listSyncRuns }));

  const cells = [...section("Sync runs").querySelectorAll("tbody tr td:nth-child(3)")];
  const unreachable = cells.find((cell) => cell.textContent?.includes("did not answer"))!;
  expect(unreachable.firstChild?.textContent?.trim()).toBe("TeamCity did not answer.");
  const details = unreachable.querySelector("details")!;
  expect(details.querySelector("summary")?.textContent).toBe("Details");
  const raw = details.querySelector("code")!.textContent!;
  expect(raw.length).toBeGreaterThan(200);
  expect(raw).toBe(transport);
  // Every result cell wraps anywhere, the refused run's sentence too.
  for (const cell of cells) expect(cell.classList.contains("tf-stg-res")).toBe(true);
  // A refused run keeps its sentence and has nothing behind it.
  const refused = cells.find((cell) => cell.textContent?.startsWith("401 Unauthorized"))!;
  expect(refused.textContent).toBe("401 Unauthorized. Re-enter the password.");
  expect(refused.querySelector("details")).toBeNull();
});

// ---- The credential expiry countdown (#1499) ----

/** Jira and TeamCity, Jira's credential expiring at `at` as its health check reports. */
function expiring(at: string | null): SettingsPorts {
  const jira = summary("jira", "Jira", "ok");
  const rows = [{ ...jira, health: { ...jira.health, secret_expires_at: at } }, SOURCES[1]!];
  return ports({ listSources: vi.fn(() => Promise.resolve(rows)) });
}

/** In `days` days from the tests' clock. */
const IN_DAYS = (days: number) => new Date(NOW.getTime() + days * 86_400_000).toISOString();

/** The credential row of Connection: its label is the noun, Token or Password. */
function credentialRow(noun: string): HTMLElement {
  const found = [...section("Connection").querySelectorAll<HTMLElement>(".tf-stg-row")].find(
    (row) => row.querySelector(".tf-stg-l")?.textContent?.trim() === noun,
  );
  if (!found) throw new Error(`no ${noun} row`);
  return found;
}

/** The countdown on the credential row, its text and tone, or `null` when none is drawn. */
function countdown(noun: string): { text: string; amber: boolean; fail: boolean } | null {
  const note = credentialRow(noun).querySelector(".tf-stg-exp");
  if (!note) return null;
  return {
    text: note.textContent!.trim(),
    amber: note.classList.contains("tf-stg-warnt"),
    fail: note.classList.contains("tf-stg-errt"),
  };
}

test("a source's page counts its credential down on the Password row: plain over 14 days, amber at 14, failed once expired (#1499)", async () => {
  await open(["source", "jira"], expiring(IN_DAYS(41)));
  expect(countdown("Password")).toEqual({ text: "expires in 41 days", amber: false, fail: false });
  unmount(view!);

  await open(["source", "jira"], expiring(IN_DAYS(14)));
  expect(countdown("Password")).toEqual({ text: "expires in 14 days", amber: true, fail: false });
  unmount(view!);

  await open(["source", "jira"], expiring(IN_DAYS(-3)));
  expect(countdown("Password")).toEqual({ text: "expired", amber: false, fail: true });
  unmount(view!);

  await open(["source", "jira"], expiring(null));
  expect(countdown("Password")).toBeNull();
});

test("the Sources row says the credential is running out only while amber or expired (#1499)", async () => {
  /** Jira's state column. */
  const state = () => {
    const cell = [...target.querySelectorAll(".tf-stg-src")][0]!.querySelectorAll(".tf-stg-si")[1]!;
    return cell.textContent!.replace(/\s+/g, " ").trim();
  };
  await open([], expiring(IN_DAYS(41)));
  expect(state()).toBe("Synced 4 min ago");
  unmount(view!);

  await open([], expiring(IN_DAYS(9)));
  expect(state()).toBe("Synced 4 min ago Credential expires in 9 days");
  expect(target.querySelector(".tf-stg-src .tf-stg-exp.tf-stg-warnt")).not.toBeNull();
  unmount(view!);

  await open([], expiring(IN_DAYS(-1)));
  expect(state()).toBe("Synced 4 min ago Credential expired");
  expect(target.querySelector(".tf-stg-src .tf-stg-exp.tf-stg-errt")).not.toBeNull();
  unmount(view!);

  await open([], expiring(null));
  expect(state()).toBe("Synced 4 min ago");
});
