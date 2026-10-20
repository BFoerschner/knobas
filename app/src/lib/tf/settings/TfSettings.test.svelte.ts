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
  AdapterStatus,
  CredentialHealth,
  SourceSummary,
  SourceSyncStatus,
  SyncProgress,
  SyncRunRow,
} from "../../ipc/sources";
import { JIRA_SCHEMA } from "../../sources/fixtures";
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
    installAdapter: vi.fn(() => Promise.reject(new Error("not in this test"))),
    listInstalledAdapters: vi.fn(() => Promise.resolve([])),
    checkAdapterUpdates: vi.fn(() => Promise.resolve([])),
    updateAdapter: vi.fn(() => Promise.reject(new Error("not in this test"))),
    uninstallAdapter: vi.fn(() => Promise.resolve()),
    listen: vi.fn(() => Promise.resolve(() => {})),
    ticketViews: vi.fn(() => Promise.resolve(JIRA_VIEWS.map((row) => ({ ...row })))),
    hideSavedView: vi.fn(() => Promise.resolve()),
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
  const { p } = await open(["source", "jira"]);
  const boards = section("Boards and filters");
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
};

test("Add a source installs an adapter from a manifest, and it joins the radios chosen (#1202)", async () => {
  const p = ports();
  const jira = (await p.listAdapters())[0]!;
  p.listAdapters = vi.fn().mockResolvedValueOnce([jira]).mockResolvedValue([jira, SURVEYS]);
  p.installAdapter = vi.fn(() =>
    Promise.resolve({
      adapter_kind: "wasm-fixture",
      adapter_version: "0.1.0",
      interface_version: "1.0.0",
      source_url: "/releases/knobas-adapter.json",
      tag: null,
      artifact_sha256: "a".repeat(64),
      hosts: ["{base_url}", "api.example.com", "{mirror}"],
      template: SURVEYS,
    }),
  );
  await open([], p);
  button("+ Add a source").click();
  await settle();
  expect(dialogButton("Install").disabled).toBe(true);

  type("#tf-add-manifest", "  /releases/knobas-adapter.json ");
  dialogButton("Install").click();
  await settle();

  expect(p.installAdapter).toHaveBeenCalledWith("/releases/knobas-adapter.json");
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
    installAdapter: vi.fn(() =>
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
  dialogButton("Install").click();
  await settle();

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
    updateAdapter: vi.fn(() => Promise.resolve(adapterRow({ tag: "v0.2.0", adapter_version: "0.2.0" }))),
  });
  await open([], p);
  expect(adaptersSection().textContent).toContain("Harbour surveys 0.1.0");
  expect(adaptersSection().textContent).toContain("v0.1.0 · https://github.test/harbour/surveys");
  expect(adaptersSection().textContent).toContain("Update available: v0.2.0");
  expect(p.updateAdapter).not.toHaveBeenCalled();

  button("Update").click();
  await settle();
  expect(p.updateAdapter).toHaveBeenCalledWith("wasm-fixture");
  expect(adaptersSection().textContent).toContain("Harbour surveys 0.2.0");
  expect(adaptersSection().textContent).not.toContain("Update available");
});

test("a refused update says why and that the installed version stays (#1209)", async () => {
  const reason = "install refused: the manifest asks for knobas:adapter@2.0.0, and this knobas hosts knobas:adapter@1.x up to 1.0.0";
  const before = adapterRow({ update: { state: "available", tag: "v0.2.0", adapter_version: "0.2.0" } });
  const after = adapterRow({ update: { state: "refused", tag: "v0.2.0", reason } });
  const listInstalledAdapters = vi.fn().mockResolvedValueOnce([before]).mockResolvedValue([after]);
  const p = ports({
    listInstalledAdapters,
    updateAdapter: vi.fn(() => Promise.reject({ code: "invalid", message: reason, source_id: null })),
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

/** The Result column of *Sync runs*, newest first. */
function runResults(): string[] {
  const section = [...target.querySelectorAll("section")].find((el) => el.querySelector("h2")?.textContent === "Sync runs");
  return [...(section?.querySelectorAll("tbody tr td:nth-child(3)") ?? [])].map((td) => td.textContent ?? "");
}

/** `watch_sync` as a stand-in: every channel the page hands it, and the run it answers. */
function watching(answer: number | null) {
  const channels: ((message: SyncProgress) => void)[] = [];
  const watchSync = vi.fn((_: string, onmessage: (message: SyncProgress) => void) => {
    channels.push(onmessage);
    return Promise.resolve(answer);
  });
  return { channels, watchSync };
}

test("a source's page opened mid-run draws the walk, and the run's row fills in when it ends (#1384)", async () => {
  const runs: SyncRunRow[] = [
    { ...SOURCES[0]!.last_run!, id: 7, trigger: "schedule", started_at: AGO(2), finished_at: null, outcome: null, upserted: 0 },
    SOURCES[0]!.last_run!,
  ];
  const { channels, watchSync } = watching(7);
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
});

test("a run that starts while the page is open is watched, and leaving the page stops listening (#1384)", async () => {
  let syncState: ((event: { payload: unknown }) => void) | null = null;
  const { channels, watchSync } = watching(8);
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
  props.rest = ["source", "jira"];
  await settle();
  expect(watchSync).toHaveBeenCalledTimes(2);
  channels[0]!(progress(8, { items: 999, elapsed_ms: 2_000, walked: 3, of: 4, unit: "projects" }));
  await settle();
  expect(runReading() ?? "", "the left page's channel still reaches the page").not.toContain("999");
  channels[1]!(progress(8, { items: 5, elapsed_ms: 2_000, walked: 2, of: 4, unit: "projects" }));
  await settle();
  expect(runReading()).toBe("2 of 4 projects · 5 items · 2 s");
});
