/**
 * Settings › Sources and a source's page (#830), against stand-ins for every
 * command, so what each button sends is what the test reads.
 *
 * `update_source` and `backfill_source` are the two commands this ticket is
 * the first UI for; their live behaviour is `crates/knobas-app/tests/it`'s.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { CredentialHealth, SourceSummary, SyncRunRow } from "../../ipc/sources";
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
    syncAll: vi.fn(() => Promise.resolve([1, 2])),
    listen: vi.fn(() => Promise.resolve(() => {})),
    ...over,
  };
}

async function settle() {
  for (let turn = 0; turn < 6; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

async function open(rest: string[], p: SettingsPorts = ports(), health = createHealthStub()) {
  view = mount(TfSettings, {
    target,
    props: { rest, job: null, ready: true, onrooms: () => {}, ports: p, health, layers: createLayers(), now: () => NOW },
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

  const onrooms = vi.fn();
  view = mount(TfSettings, {
    target,
    props: { rest: ["checkouts"], job: null, ready: true, onrooms, ports: ports(), health: createHealthStub(), layers: createLayers() },
  });
  await settle();
  // Built since #844: its own section, and no offer of the rooms UI.
  expect(target.querySelector(".tf-stg-body h1")?.textContent).toBe("Checkouts and Open in…");
  expect(target.querySelector(".tf-stg-body")?.textContent).not.toContain("not built in the new shell yet");
  expect(onrooms).not.toHaveBeenCalled();
});

test("Esc closes a dialog through the shell's layer stack", async () => {
  const layers = createLayers();
  view = mount(TfSettings, {
    target,
    props: { rest: ["source", "jira"], job: null, ready: true, onrooms: () => {}, ports: ports(), health: createHealthStub(), layers, now: () => NOW },
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
