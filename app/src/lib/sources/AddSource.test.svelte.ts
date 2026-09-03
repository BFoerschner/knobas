/**
 * The five-step Add-source flow (spec §3: type → URL → auth → *Test
 * connection* → schedule → save).
 *
 * The behaviours pinned here are the ones that decide whether a source works
 * before the reader ever leaves the dialog: that step 1 is the adapter
 * registry and not a hardcoded list, that the instance id is validated at the
 * moment it becomes permanent (P10 makes it the entity namespace and therefore
 * immutable), that *Test connection* reports what actually answered, and that
 * the secret goes into exactly one request and nowhere else.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { ConnectionReport, NewSource, SourceDescriptor, SourceDraft } from "../ipc/sources";
import { GITEA_SCHEMA, JIRA_SCHEMA, TEAMCITY_SCHEMA } from "./fixtures";

const calls = {
  listAdapters: 0,
  testSource: [] as SourceDraft[],
  addSource: [] as NewSource[],
};

let adapters: SourceDescriptor[] = [];
let adaptersFail: unknown = null;
let report: ConnectionReport = {
  ok: true,
  account: "mara.oyelaran",
  server_version: "9.12.4",
  secret_expires_at: null,
  error: null,
  code: null,
  elapsed_ms: 214,
  discovered: {},
};
let addFails: unknown = null;

vi.mock("../ipc/sources", () => ({
  listAdapters: () => {
    calls.listAdapters += 1;
    return adaptersFail ? Promise.reject(adaptersFail) : Promise.resolve(adapters);
  },
  testSource: (draft: SourceDraft) => {
    calls.testSource.push(draft);
    return Promise.resolve(report);
  },
  addSource: (input: NewSource) => {
    calls.addSource.push(input);
    return addFails
      ? Promise.reject(addFails)
      : Promise.resolve({
          id: input.id,
          adapter_kind: input.adapter_kind,
          display_name: input.display_name,
          base_url: input.base_url,
          enabled: true,
          sync_interval_secs: input.sync_interval_secs,
          config: input.config,
          health: {
            source_id: input.id,
            state: "ok" as const,
            checked_at: null,
            detail: null,
            secret_expires_at: null,
          },
          last_run: null,
          next_run_at: null,
          item_count: 0,
          kinds: [],
        });
  },
}));

const { default: AddSource } = await import("./AddSource.svelte");

function descriptor(over: Partial<SourceDescriptor> = {}): SourceDescriptor {
  return {
    id: "jira",
    adapter_kind: "jira",
    name: "Jira Data Center",
    capabilities: [],
    adapter_version: "0.1.0",
    auth_methods: ["UserPassword", "Pat"],
    write_ops: [],
    entity_kinds: [
      { id: "ticket", label: "Ticket", plural: "Tickets", monogram: "TK", full_sync_exhaustive: false },
    ],
    config_schema: JIRA_SCHEMA,
    payload_paths: [],
    ...over,
  };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;
let saved: NewSource[] = [];

function render() {
  saved = [];
  app = mount(AddSource, {
    target,
    props: {
      onclose: () => {},
      onsaved: (source: { id: string }) => void saved.push(source as unknown as NewSource),
    },
  });
  flushSync();
  return app;
}

async function settle() {
  for (let i = 0; i < 8; i += 1) await Promise.resolve();
  flushSync();
}

/**
 * A button by the label a reader would point at.
 *
 * An adapter tile carries a monogram and a sub-line as well as its name, so
 * its whole `textContent` is not what anyone calls it; the `.nm` span is.
 */
function button(label: string) {
  return [...target.querySelectorAll<HTMLButtonElement>("button")].find(
    (b) =>
      b.textContent?.trim() === label ||
      b.querySelector(".nm")?.textContent?.trim() === label,
  );
}

function input(selector: string) {
  return target.querySelector<HTMLInputElement>(selector)!;
}

function type(selector: string, value: string) {
  const el = input(selector);
  el.value = value;
  el.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  return el;
}

/** The `.steps` breadcrumb's current step, as a reader sees it. */
function step() {
  return target.querySelector(".steps span.on")?.textContent?.trim();
}

function text() {
  return target.textContent ?? "";
}

/** Walk to the auth step with a valid draft, which most tests start from. */
async function toAuth() {
  render();
  await settle();
  button("Jira Data Center")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  type("#add-url", "https://jira.tidewater.example");
  button("Next")!.click();
  flushSync();
}

beforeEach(() => {
  calls.listAdapters = 0;
  calls.testSource = [];
  calls.addSource = [];
  adapters = [descriptor()];
  adaptersFail = null;
  addFails = null;
  report = {
    ok: true,
    account: "mara.oyelaran",
    server_version: "9.12.4",
    secret_expires_at: null,
    error: null,
    code: null,
    elapsed_ms: 214,
    discovered: {},
  };
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

test("step 1 lists adapters from listAdapters, not from a hardcoded table", async () => {
  adapters = [
    descriptor(),
    descriptor({
      id: "quokka",
      adapter_kind: "quokka",
      name: "Quokka Tracker",
      config_schema: GITEA_SCHEMA,
      auth_methods: ["ApiToken"],
    }),
  ];
  render();
  await settle();

  expect(calls.listAdapters).toBe(1);
  // An adapter this file has never heard of is offered by name — which is the
  // point of the registry and the thing a hardcoded list cannot do.
  expect(button("Quokka Tracker")).toBeTruthy();
  expect(button("Jira Data Center")).toBeTruthy();
});

test("listAdapters failing says so rather than showing an empty picker", async () => {
  adaptersFail = { code: "internal", message: "the registry is not built", source_id: null };
  render();
  await settle();
  expect(text()).toContain("the registry is not built");
  expect(button("Next")?.disabled).toBe(true);
});

test("the instance id defaults to the adapter kind and is validated", async () => {
  render();
  await settle();
  button("Jira Data Center")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();

  expect(input("#add-id").value).toBe("jira");

  // Contract §4.1: `[a-z][a-z0-9-]{0,31}`. The id is the entity namespace and
  // is baked into every entity id, link and activity row, so it is immutable
  // (P10) — which makes this the only moment it can be got right.
  type("#add-id", "Jira EU");
  expect(text()).toMatch(/lower-?case/i);
  expect(button("Next")!.disabled).toBe(true);

  type("#add-id", "9lives");
  expect(button("Next")!.disabled).toBe(true);

  type("#add-id", "jira-eu");
  type("#add-url", "https://jira.tidewater.example");
  expect(button("Next")!.disabled).toBe(false);
});

test("the base URL is required before the flow can go on", async () => {
  render();
  await settle();
  button("Jira Data Center")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  expect(button("Next")!.disabled).toBe(true);
  type("#add-url", "https://jira.tidewater.example");
  expect(button("Next")!.disabled).toBe(false);
});

test("step 2 renders the adapter's own config schema, generated", async () => {
  render();
  await settle();
  button("Jira Data Center")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  // Straight from `JIRA_SCHEMA` — the dialog has no per-adapter branch.
  expect(target.querySelector('[data-field="flavor"] select')).toBeTruthy();
  expect(target.querySelector('[data-field="projects"] textarea')).toBeTruthy();
});

test("auth options come from the descriptor's auth_methods", async () => {
  adapters = [descriptor({ auth_methods: ["Pat"] })];
  await toAuth();
  expect(step()).toBe("Auth");
  const radios = [...target.querySelectorAll<HTMLInputElement>('input[type="radio"]')];
  expect(radios.map((r) => r.value)).toEqual(["Pat"]);
  // A single method is preselected: there is nothing to choose, and an unset
  // radio would block the flow on a decision with one answer.
  expect(radios[0]!.checked).toBe(true);
});

test("Test connection shows account, server version and elapsed time on success", async () => {
  await toAuth();
  type("#add-secret", "s3cret");
  button("Next")!.click();
  flushSync();
  expect(step()).toBe("Test");

  button("Test connection")!.click();
  await settle();

  expect(calls.testSource.length).toBe(1);
  const draft = calls.testSource[0]!;
  // An unsaved draft: `source_id` is null, so the backend tests what is being
  // typed rather than something already stored.
  expect(draft.source_id).toBeNull();
  expect(draft.base_url).toBe("https://jira.tidewater.example");
  expect(draft.secret).toEqual({ value: "s3cret" });

  const result = target.querySelector(".test-res")!;
  expect(result.textContent).toContain("mara.oyelaran");
  expect(result.textContent).toContain("9.12.4");
  expect(result.textContent).toContain("214");
});

test("the three optional readings are simply absent when the server does not say", async () => {
  report = { ok: true, account: null, server_version: null, secret_expires_at: null, error: null, code: null, elapsed_ms: 88, discovered: {} };
  await toAuth();
  type("#add-secret", "s3cret");
  button("Next")!.click();
  flushSync();
  button("Test connection")!.click();
  await settle();

  const result = target.querySelector(".test-res")!;
  expect(result.textContent).toContain("Connected");
  expect(result.textContent).toContain("88");
  // No "null", no "unknown", no empty separator left dangling.
  expect(result.textContent).not.toContain("null");
  expect(result.textContent).not.toContain("undefined");
  expect(button("Next")!.disabled).toBe(false);
});

test("a failed test shows the error and leaves Next disabled", async () => {
  report = {
    ok: false,
    account: null,
    server_version: null,
    secret_expires_at: null,
    error: "401 Unauthorized from /rest/api/2/myself",
    code: "unauthorized",
    elapsed_ms: 190,
    discovered: {},
  };
  await toAuth();
  type("#add-secret", "wrong");
  button("Next")!.click();
  flushSync();
  button("Test connection")!.click();
  await settle();

  expect(target.querySelector(".test-res")!.textContent).toContain(
    "401 Unauthorized from /rest/api/2/myself",
  );
  // Saving a source whose credential has just been refused would put a row in
  // the database that the scheduler will never be able to run.
  expect(button("Next")!.disabled).toBe(true);
});

test("a test error from a source system is text, never markup", async () => {
  report = {
    ok: false,
    account: null,
    server_version: null,
    secret_expires_at: null,
    error: '<img src=x onerror="alert(1)">',
    code: "internal",
    elapsed_ms: 12,
    discovered: {},
  };
  await toAuth();
  type("#add-secret", "x");
  button("Next")!.click();
  flushSync();
  button("Test connection")!.click();
  await settle();
  const result = target.querySelector(".test-res")!;
  expect(result.textContent).toContain('<img src=x onerror="alert(1)">');
  expect(result.querySelector("img")).toBeNull();
});

test("Save sends the config from the generated form and the secret exactly once", async () => {
  await toAuth();
  type("#add-secret", "s3cret");
  button("Next")!.click();
  flushSync();
  button("Test connection")!.click();
  await settle();
  button("Next")!.click();
  flushSync();
  expect(step()).toBe("Schedule");

  button("Save")!.click();
  await settle();

  expect(calls.addSource.length).toBe(1);
  const sent = calls.addSource[0]!;
  expect(sent).toMatchObject({
    id: "jira",
    adapter_kind: "jira",
    base_url: "https://jira.tidewater.example",
    auth_kind: "UserPassword",
    secret: { value: "s3cret" },
    enabled: true,
  });
  // From the generated form's declared default, not from a hand-written table
  // — plus the username the green test above filled in (#82).
  expect(sent.config).toEqual({ flavor: "datacenter", username: "mara.oyelaran" });
  expect(saved.length).toBe(1);
});

test("the config carries what was typed into the generated form", async () => {
  render();
  await settle();
  button("Jira Data Center")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  type("#add-url", "https://jira.tidewater.example");
  const projects = target.querySelector<HTMLTextAreaElement>('[data-field="projects"] textarea')!;
  projects.value = "PAY\nOPS";
  projects.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  button("Next")!.click();
  flushSync();
  type("#add-secret", "s3cret");
  button("Next")!.click();
  flushSync();
  button("Test connection")!.click();
  await settle();
  button("Next")!.click();
  flushSync();
  button("Save")!.click();
  await settle();

  expect(calls.addSource[0]!.config).toEqual({
    flavor: "datacenter",
    projects: ["PAY", "OPS"],
    username: "mara.oyelaran",
  });
});

/**
 * Driven through a bound a real adapter really declares.
 *
 * It used to empty Jira's `flavor`, "the one required field in the Jira
 * schema" — which the adapter does not declare and the fixture had invented,
 * so the step this test watched being blocked is one the real dialog lets
 * through (#124). TeamCity's `builds_per_config` is a constraint the source
 * itself enforces (`MAX_BUILDS_PER_CONFIG`), so a form that refuses it is a
 * form agreeing with the adapter rather than with a fixture.
 */
test("a config the validator rejects blocks the step it was typed on", async () => {
  adapters = [
    descriptor({
      id: "teamcity",
      adapter_kind: "teamcity",
      name: "TeamCity",
      config_schema: TEAMCITY_SCHEMA,
    }),
  ];
  render();
  await settle();
  button("TeamCity")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  type("#add-url", "https://teamcity.tidewater.example");
  const budget = target.querySelector<HTMLInputElement>('[data-field="builds_per_config"] input')!;
  budget.value = "10001";
  budget.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();

  expect(button("Next")!.disabled).toBe(true);
  expect(text()).toMatch(/at most 10000/);
});

test("the secret is never written anywhere but the request", async () => {
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
  for (const name of ["localStorage", "sessionStorage"] as const) {
    Object.defineProperty(window, name, { value: storage, configurable: true, writable: true });
  }
  // The control: this dump can see a value that *is* stored, so a clean dump
  // below is a fact about the secret and not about the dump.
  storage.setItem("canary", "s3cret");
  expect(JSON.stringify([...stored])).toContain("s3cret");
  storage.clear();

  await toAuth();
  type("#add-secret", "s3cret");
  // While it is on screen: the value lives in the input's property, which is
  // where typing lives, and not in an attribute a serializer would carry.
  expect(target.innerHTML).not.toContain("s3cret");
  button("Next")!.click();
  flushSync();
  button("Test connection")!.click();
  await settle();
  button("Next")!.click();
  flushSync();
  button("Save")!.click();
  await settle();

  expect(JSON.stringify([...stored])).not.toContain("s3cret");
  expect(target.innerHTML).not.toContain("s3cret");
  // Exactly once in the request, and once in the test — never a second
  // "confirm the secret" round trip.
  expect(calls.addSource.length).toBe(1);
  expect(calls.testSource.length).toBe(1);
});

test("the secret field is a password field, and there is no way to read it back", async () => {
  await toAuth();
  expect(input("#add-secret").type).toBe("password");
  expect(input("#add-secret").autocomplete).toBe("off");
});

test("a failed save keeps the dialog open and says why", async () => {
  addFails = { code: "conflict", message: "a source with id jira already exists", source_id: "jira" };
  await toAuth();
  type("#add-secret", "s3cret");
  button("Next")!.click();
  flushSync();
  button("Test connection")!.click();
  await settle();
  button("Next")!.click();
  flushSync();
  button("Save")!.click();
  await settle();

  expect(text()).toContain("a source with id jira already exists");
  expect(saved.length).toBe(0);
  // Still on the schedule step, with everything typed still there — a dialog
  // that closed on failure would throw the whole draft away.
  expect(step()).toBe("Schedule");
});

test("Back returns to the previous step with the draft intact", async () => {
  await toAuth();
  expect(step()).toBe("Auth");
  button("Back")!.click();
  flushSync();
  expect(step()).toBe("Connection");
  expect(input("#add-url").value).toBe("https://jira.tidewater.example");
});

test("changing the adapter resets the config, so no field of the old one survives", async () => {
  adapters = [
    descriptor(),
    descriptor({ id: "gitea", adapter_kind: "gitea", name: "Gitea", config_schema: GITEA_SCHEMA }),
  ];
  render();
  await settle();
  button("Jira Data Center")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  expect(target.querySelector('[data-field="flavor"]')).toBeTruthy();

  button("Back")!.click();
  flushSync();
  button("Gitea")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  // Gitea's schema, and nothing of Jira's: a config carrying `flavor` would be
  // written to a Gitea source's config column, where it means nothing.
  expect(target.querySelector('[data-field="flavor"]')).toBeNull();
  expect(target.querySelector('[data-field="owners"]')).toBeTruthy();
  expect(input("#add-id").value).toBe("gitea");
});

test("an adapter whose schema declares a secret refuses the step instead of drawing it", async () => {
  adapters = [
    descriptor({
      id: "broken",
      adapter_kind: "broken",
      name: "Broken Adapter",
      config_schema: { type: "object", properties: { api_token: { type: "string" } } },
    }),
  ];
  render();
  await settle();
  button("Broken Adapter")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();

  // Contract §3: a credential in the config column would be written to
  // Postgres. Saying so beats drawing a field that quietly does it.
  expect(text()).toMatch(/secret/i);
  expect(button("Next")!.disabled).toBe(true);
});

/*
 * The identity, filled from what *Test connection* just reported (#82).
 *
 * `username` is the only source of the identity `@me`, *My items* and *Mine,
 * untouched* resolve against, and author matching is case-sensitive — so a
 * reader retyping by hand the account the dialog has just printed on screen is
 * one slip away from an identity that matches nothing. The dialog cannot reach
 * Save without a green test, so the account is in hand for every source that
 * is ever saved.
 *
 * The fill is keyed on the property **name**, which all three adapters spell
 * `username`; it is a convention, not per-adapter knowledge, and the dialog
 * stays generated from `config_schema`.
 */

/** Walk a green test to the point where the report is on screen. */
async function toTested() {
  await toAuth();
  type("#add-secret", "s3cret");
  button("Next")!.click();
  flushSync();
  button("Test connection")!.click();
  await settle();
}

/** Step 5 → Save, from the test step. */
async function saveFromTest() {
  button("Next")!.click();
  flushSync();
  button("Save")!.click();
  await settle();
}

test("a successful test fills the username the reader never typed", async () => {
  await toTested();
  await saveFromTest();
  expect(calls.addSource[0]!.config).toMatchObject({ username: "mara.oyelaran" });
});

test("the filled username is on screen and editable, not a hidden value", async () => {
  await toTested();
  button("Back")!.click();
  flushSync();
  button("Back")!.click();
  flushSync();
  expect(step()).toBe("Connection");
  const field = input("#add-cfg-username");
  expect(field.value).toBe("mara.oyelaran");
  // Editable: the account the server reports is a starting point, not a lock.
  type("#add-cfg-username", "mara");
  expect(input("#add-cfg-username").value).toBe("mara");
});

test("the account is stored in the server's own spelling, never case-folded", async () => {
  report = { ...report, account: "Mara.Oyelaran" };
  await toTested();
  await saveFromTest();
  // `sync.item.author` is compared case-sensitively by construction, so the
  // one spelling that can match is the one the source itself uses.
  expect(calls.addSource[0]!.config).toMatchObject({ username: "Mara.Oyelaran" });
});

test("a username the reader typed is never overwritten by a later test", async () => {
  render();
  await settle();
  button("Jira Data Center")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  type("#add-url", "https://jira.tidewater.example");
  type("#add-cfg-username", "m.lindqvist");
  button("Next")!.click();
  flushSync();
  type("#add-secret", "s3cret");
  button("Next")!.click();
  flushSync();
  button("Test connection")!.click();
  await settle();
  await saveFromTest();

  expect(calls.addSource[0]!.config).toMatchObject({ username: "m.lindqvist" });
});

test("a source whose test reports no account saves exactly as it does today", async () => {
  // Contract: `ConnectionInfo.account` is optional by design — a source whose
  // API has no "who am I" endpoint is not a broken source.
  report = { ...report, account: null };
  await toTested();
  await saveFromTest();

  expect(calls.addSource.length).toBe(1);
  // Not filled, and not sent as an empty string either: the adapter's own
  // default has to be able to apply.
  expect(calls.addSource[0]!.config).not.toHaveProperty("username");
  expect(saved.length).toBe(1);
});

test("a report with no account leaves a typed username where it is", async () => {
  // The absent-account path must not be a write of `null` dressed up as a
  // no-op: that would empty a field the reader had filled in themselves.
  report = { ...report, account: null };
  render();
  await settle();
  button("Jira Data Center")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  type("#add-url", "https://jira.tidewater.example");
  type("#add-cfg-username", "m.lindqvist");
  button("Next")!.click();
  flushSync();
  type("#add-secret", "s3cret");
  button("Next")!.click();
  flushSync();
  button("Test connection")!.click();
  await settle();
  await saveFromTest();

  expect(calls.addSource[0]!.config).toMatchObject({ username: "m.lindqvist" });
});

test("the fill is the property name, so an adapter this file never heard of gets it", async () => {
  adapters = [
    descriptor({
      id: "quokka",
      adapter_kind: "quokka",
      name: "Quokka Tracker",
      auth_methods: ["ApiToken"],
      config_schema: {
        type: "object",
        properties: { username: { type: "string", title: "Username" } },
      },
    }),
  ];
  render();
  await settle();
  button("Quokka Tracker")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  type("#add-url", "https://quokka.tidewater.example");
  button("Next")!.click();
  flushSync();
  type("#add-secret", "s3cret");
  button("Next")!.click();
  flushSync();
  button("Test connection")!.click();
  await settle();
  await saveFromTest();

  expect(calls.addSource[0]!.config).toMatchObject({ username: "mara.oyelaran" });
});

/**
 * Every shipped adapter's *own* schema, driven through the fill.
 *
 * The tests above exercise Jira's fixture and a synthetic `quokka`. These two
 * drive the other real transcriptions, which is what makes `fixtures.ts`
 * load-bearing rather than decorative: the three adapters spell `username`
 * two ways -- Jira and Gitea `["string", "null"]`, TeamCity a plain
 * `"string"` -- and a fixture that drifts back to a shape the form draws as a
 * JSON textarea takes `fillIdentity`'s `control.kind !== "text"` branch and
 * fails here rather than in the window. That drift is why nobody noticed the
 * textarea in the first place (#82).
 */
for (const [name, kind, schema] of [
  ["Gitea", "gitea", GITEA_SCHEMA],
  ["TeamCity", "teamcity", TEAMCITY_SCHEMA],
] as const) {
  test(`${name}'s own schema is filled from the account its test reported`, async () => {
    adapters = [descriptor({ id: kind, adapter_kind: kind, name, config_schema: schema })];
    render();
    await settle();
    button(name)!.click();
    flushSync();
    button("Next")!.click();
    flushSync();
    // A literal, not an interpolation: the house rule scans for a reserved
    // hostname and `${kind}` is not one it can read.
    type("#add-url", "https://source.tidewater.example");
    button("Next")!.click();
    flushSync();
    type("#add-secret", "s3cret");
    button("Next")!.click();
    flushSync();
    button("Test connection")!.click();
    await settle();
    await saveFromTest();

    expect(calls.addSource[0]!.config).toMatchObject({ username: "mara.oyelaran" });
  });
}

test("an adapter with no username field has none invented for it", async () => {
  adapters = [descriptor({ config_schema: { type: "object", properties: {} } })];
  await toTested();
  await saveFromTest();
  expect(calls.addSource[0]!.config).toEqual({});
});

test("a failed test fills nothing, even if the report carries an account", async () => {
  report = { ...report, ok: false, error: "401 Unauthorized" };
  await toAuth();
  type("#add-secret", "wrong");
  button("Next")!.click();
  flushSync();
  button("Test connection")!.click();
  await settle();
  button("Back")!.click();
  flushSync();
  button("Back")!.click();
  flushSync();
  expect(input("#add-cfg-username").value).toBe("");
});

/*
 * The per-instance ids a source discovers about *itself* (#297).
 *
 * Same rule as the identity fill above and the same reason: a value the far
 * end owns and the reader cannot know. Jira's Epic Link custom field id is
 * minted per instance — three seeds of one script gave `customfield_10101`,
 * `customfield_10109` and `customfield_10101`, and on one of them
 * `customfield_10102` was *Epic Status* — so an id copied off another server
 * does not fail, it silently syncs the wrong field. A classic Jira project
 * keeps epic membership nowhere else, so a source saved without it mirrors
 * none and nothing says why.
 *
 * The fill is keyed on the property name the report itself supplies, so this
 * dialog holds no per-adapter table and the next adapter that discovers
 * something needs no change here.
 */

const EPIC = { epic_link_field: "customfield_10101" };

test("a successful test fills the epic link field the reader could not know", async () => {
  report = { ...report, discovered: EPIC };
  await toTested();
  await saveFromTest();
  expect(calls.addSource[0]!.config).toMatchObject({ epic_link_field: "customfield_10101" });
});

test("the discovered id is on screen and editable, not a hidden value", async () => {
  report = { ...report, discovered: EPIC };
  await toTested();
  button("Back")!.click();
  flushSync();
  button("Back")!.click();
  flushSync();
  expect(step()).toBe("Connection");
  expect(input("#add-cfg-epic_link_field").value).toBe("customfield_10101");
  type("#add-cfg-epic_link_field", "customfield_10999");
  expect(input("#add-cfg-epic_link_field").value).toBe("customfield_10999");
});

test("an id the reader typed is never overwritten by a later test", async () => {
  // The one direction that costs data: a reader who pasted the id off their
  // own instance meant it, and a probe that disagreed must not win.
  report = { ...report, discovered: EPIC };
  render();
  await settle();
  button("Jira Data Center")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  type("#add-url", "https://jira.tidewater.example");
  type("#add-cfg-epic_link_field", "customfield_10008");
  button("Next")!.click();
  flushSync();
  type("#add-secret", "s3cret");
  button("Next")!.click();
  flushSync();
  button("Test connection")!.click();
  await settle();
  await saveFromTest();

  expect(calls.addSource[0]!.config).toMatchObject({ epic_link_field: "customfield_10008" });
});

test("a source that discovers nothing saves exactly as it does today", async () => {
  // A Jira with no Epic Link field — no Jira Software, or the field table
  // behind a proxy — is a working source, and the empty map must not reach
  // the config as an empty string either: the adapter's own default has to be
  // able to apply.
  report = { ...report, discovered: {} };
  await toTested();
  await saveFromTest();
  expect(calls.addSource[0]!.config).not.toHaveProperty("epic_link_field");
  expect(saved.length).toBe(1);
});

test("a failed test fills nothing, so one server's ids cannot land on another", async () => {
  report = {
    ok: false,
    account: null,
    server_version: null,
    secret_expires_at: null,
    error: "the credential was refused",
    code: "unauthorized",
    elapsed_ms: 12,
    // A backend that answered this on a failure would be wrong; the dialog
    // does not depend on it being right.
    discovered: EPIC,
  };
  await toTested();
  button("Back")!.click();
  flushSync();
  button("Back")!.click();
  flushSync();
  expect(input("#add-cfg-epic_link_field").value).toBe("");
});

test("a discovered key naming a property this adapter does not declare never reaches the config", async () => {
  // The report is the adapter's claim about its own schema, and a key that
  // names nothing must not invent a config value the form never drew — the
  // rest of the map still lands.
  //
  // What enforces it is `validate`, which builds the config from the schema's
  // fields; the `!field` guard inside `fillDiscovered` is a second layer and
  // is **not** witnessed by this test. Mutating that guard away leaves the
  // saved config identical, because the stray key is dropped downstream
  // either way, and nothing on screen renders `configValues` for a key no
  // field drew. Recorded rather than pretended: the direction this cannot see
  // is "the dialog held a value it never showed".
  report = { ...report, discovered: { not_a_property: "x", ...EPIC } };
  await toTested();
  await saveFromTest();
  expect(calls.addSource[0]!.config).toMatchObject({ epic_link_field: "customfield_10101" });
  expect(calls.addSource[0]!.config).not.toHaveProperty("not_a_property");
});
