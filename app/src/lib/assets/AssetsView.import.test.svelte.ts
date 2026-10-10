/**
 * The Import, from the room bar to the third group and out the other side
 * (issue #439, spec #427 stories 21–24).
 *
 * A file of its own, the way #435's linking is: this one drives an
 * `<input type="file">` and a two-step dialog, and the files beside it are
 * about a surface that reads one asset at a time.
 *
 * ## What is asserted here, and what is asserted elsewhere
 *
 * Which properties survive an import is the **backend's** rule and is pinned
 * at its own seam (`crates/knobas-app/tests/it/assets_ipc.rs`): a second copy of
 * *"a hand edit is any activity line by the user on that property"* in the
 * frontend would be the copy that goes stale. What is here is the part only
 * this view can get wrong — that choosing a file previews rather than
 * imports, that the three groups draw what the preview answered, that a kept
 * property says so *before* the write, that Import sends the file the reader
 * chose, and that the Tree re-reads afterwards rather than patching itself.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type {
  AssetDetail,
  AssetRow,
  ImportOutcome,
  ImportPreview,
  Landing,
  Produced,
} from "../ipc/assets";
import type { IpcError } from "../ipc";
import { createRouter } from "../shell/router.svelte";

const { default: AssetsView } = await import("./AssetsView.svelte");

const NOW = new Date(2026, 8, 6, 12, 0, 0, 0);

/** The one asset this fixture's estate already holds. */
const SITE: AssetRow = {
  id: "asset:knobas-estate",
  parent_id: null,
  type_id: "site",
  type_label: "Site",
  monogram: "SI",
  name: "knobas test estate",
  status: "none",
  environment: "dev",
  owner: "Björn Förschner",
  has_children: false,
  health: "none",
  inside: "none",
  problems_inside: 0,
  linked_work: 0,
};

/**
 * A preview with something in every group.
 *
 * The shape of the real answer over the real estate file, at the size a test
 * can read: one asset that is new, one already in the tree with two properties
 * changed — one the file wins and one the reader does — and one monitor to
 * attach. A preview with an empty group in it could not tell a view that draws
 * the group from one that drops it.
 */
const PREVIEW: ImportPreview = {
  name: "knobas test estate",
  known: [
    {
      id: "asset:knobas-estate",
      kind: "asset",
      name: "knobas test estate",
      type_label: "Site",
      parent_id: null,
    },
  ],
  new: [
    {
      id: "asset:hetzner-teamcity",
      kind: "asset",
      name: "knobas-teamcity",
      type_label: "VM",
      parent_id: "asset:knobas-estate",
    },
    {
      id: "route:tunnel-jira",
      kind: "route",
      name: "Jira (tunnel)",
      type_label: null,
      parent_id: "asset:notebook",
    },
  ],
  changes: [
    {
      id: "asset:knobas-estate",
      name: "knobas test estate",
      properties: [
        {
          key: "location",
          label: "Location",
          from: { kind: "text", value: "nbg1" },
          to: { kind: "text", value: "hel1" },
          plan: "set",
        },
        {
          key: "provider",
          label: "Provider",
          from: { kind: "text", value: "Hetzner, and my desk" },
          to: { kind: "text", value: "Hetzner Cloud" },
          plan: "kept",
        },
      ],
      monitors: ["knobas-jira"],
    },
  ],
  monitor_links: [
    {
      asset_id: "asset:knobas-estate",
      asset_name: "knobas test estate",
      monitor_name: "gitea",
      monitor_id: "monitor:kuma:3",
    },
  ],
  // The other half of the same announcement (#445): a name the mirror does
  // not hold. Populated beside a link that *did* resolve, because a preview
  // that put every name in one list or the other would satisfy either
  // assertion alone.
  unresolved: [
    {
      asset_id: "asset:knobas-estate",
      asset_name: "knobas test estate",
      monitor_name: "jira (tunnel)",
    },
  ],
};

const OUTCOME: ImportOutcome = {
  assets_created: 1,
  routes_created: 1,
  properties_set: 1,
  properties_kept: 1,
  monitors_kept: 1,
  monitors_linked: 1,
};

function detailOf(assetId: string): AssetDetail {
  if (assetId !== SITE.id) throw { code: "not_found", message: `no asset ${assetId}` };
  return {
    asset: SITE,
    properties: [],
    effective_environment: null,
    effective_owner: null,
    held_by: [],
    holds: [],
    exposes: [],
    reachable_via: [],
    history: [],
    links: [],
    // The names an import kept (#439). Populated here because the pane draws
    // them, and a fixture with none could not tell a pane that draws the list
    // from one that does not.
    monitors: ["gitea", "jira (tunnel)"],
    // None of them has found a monitor: this fixture is an estate whose Kuma
    // has not synced, which is what makes both names still a *queue*. The
    // resolved case is `AssetsView.test.svelte.ts`'.
    monitoring: [],
    monitor_targets: [],
  };
}

/** What the scripted importer hands back, and what the preview is asked about. */
const FILE = '{"version":1,"name":"Hetzner Cloud","assets":[],"routes":[]}';

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

/** What the three Import commands were handed, in order. */
interface Calls {
  previewed: string[];
  applied: string[];
  /** The producer sent with each call, preview and apply alike (#508). */
  producers: string[];
  reads: number;
  /**
   * `(producer, token, landUnder)` per `produce_estate_file` call (#509).
   *
   * `landUnder` is a {@link Landing} or `null`, and the difference between
   * `null` and `{ parent: null }` is the whole of what the second ruling of
   * 2026-09-08 was about — so it is recorded verbatim rather than flattened to
   * an id, which would have made the two indistinguishable here too.
   */
  produces: [string, string | null, Landing | null][];
}

/**
 * What the importer answers, one per call, in order.
 *
 * A *script* and not one canned answer, because the two questions #509's
 * criterion is about are two round trips: `token_needed` then `ready`, or
 * `landing_needed` then `ready`. A stub that answered the same thing twice
 * could not tell a dialog that asks once from one that asks every time.
 */
let script: (Produced | IpcError)[] = [];

/**
 * The next scripted answer is a **rejection** rather than an answer.
 *
 * A knob beside the script, the shape `render`'s `refusal` has for the preview:
 * the produce port's refusals are what put the token field back, and a port
 * that could only resolve could not drive that branch at all.
 */
let rejectNextProduce = false;

function render(refusal: unknown = null, hash = "#/assets/tree") {
  const calls: Calls = {
    previewed: [],
    applied: [],
    producers: [],
    reads: 0,
    produces: [],
  };
  location.hash = hash;
  const router = createRouter();
  app = mount(AssetsView, {
    target,
    props: {
      router,
      now: () => NOW,
      ports: {
        assetTypes: () => Promise.resolve([]),
        // #505's panel: read on every selection, so a port left out falls
        // through to the real `invoke`. Empty is honest -- no test here is
        // about it.
        dependsOnThis: () => Promise.resolve({ assets: [], routes: [] }),
        assetTree: (parentId?: string | null) => {
          calls.reads += 1;
          return Promise.resolve((parentId ?? null) === null ? [SITE] : []);
        },
        getAsset: (assetId: string) => {
          try {
            return Promise.resolve(detailOf(assetId));
          } catch (cause) {
            return Promise.reject(cause);
          }
        },
        previewEstateImport: (file: string, producer: string) => {
          calls.previewed.push(file);
          calls.producers.push(producer);
          return refusal === null ? Promise.resolve(PREVIEW) : Promise.reject(refusal);
        },
        applyEstateImport: (file: string, producer: string) => {
          calls.applied.push(file);
          calls.producers.push(producer);
          return Promise.resolve(OUTCOME);
        },
        produceEstateFile: (
          producer: string,
          token: string | null,
          landUnder: Landing | null,
        ) => {
          calls.produces.push([producer, token, landUnder]);
          const answer = script.shift();
          if (answer === undefined) {
            return Promise.reject(new Error("the importer was run more times than the test scripted"));
          }
          if (rejectNextProduce) {
            rejectNextProduce = false;
            return Promise.reject(answer);
          }
          return Promise.resolve(answer as Produced);
        },
      },
    },
  });
  flushSync();
  return calls;
}

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
  script = [];
  rejectNextProduce = false;
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
  location.hash = "";
});

/**
 * Wait for the reads in flight, then redraw.
 *
 * A macrotask as well as the microtask turns every other suite here uses:
 * `File.text()` is the platform's own read and does not resolve on the
 * microtask queue, so a `Promise.resolve()` ladder alone lands before the
 * dialog has the file's text.
 */
async function settle() {
  for (let turn = 0; turn < 4; turn += 1) await Promise.resolve();
  await new Promise((done) => setTimeout(done, 0));
  for (let turn = 0; turn < 4; turn += 1) await Promise.resolve();
  flushSync();
}

function button(label: string): HTMLButtonElement | undefined {
  return [...target.querySelectorAll("button")].find(
    (candidate) => candidate.textContent?.trim() === label,
  );
}

/** Choose `text` as the estate file, the way a reader chooses one. */
async function choose(text: string) {
  const input = target.querySelector<HTMLInputElement>('input[type="file"]');
  if (!input) throw new Error("the dialog draws no file input");
  Object.defineProperty(input, "files", {
    configurable: true,
    value: [new File([text], "estate.json", { type: "application/json" })],
  });
  // `bubbles`, and not decoration: Svelte 5 delegates `change` to the mount
  // root, so an event that does not bubble reaches no listener at all and the
  // dialog sits there having been handed a file it never heard about.
  input.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
}

/** The headings of the preview's groups, in the order they are drawn. */
function groups(): string[] {
  return [...target.querySelectorAll(".grp h3")].map(
    (heading) => heading.textContent?.replace(/\s+/g, " ").trim() ?? "",
  );
}

/**
 * The monitor names an import kept are on the asset's pane (#439's fourth
 * criterion, at the surface a reader can see it on).
 *
 * The section is drawn only when there are names: *Exposes* and *Reachable
 * via* answer a question about every asset, and this one is the estate file's
 * own note. So the assertion is both — the names on an asset that has them,
 * and no heading at all on an estate nobody imported.
 */
test("the pane lists the monitor names an import kept", async () => {
  render(null, `#/asset/${SITE.id}`);
  await settle();

  const section = target.querySelector(".named");
  expect(section?.querySelector("h3")?.textContent?.trim()).toBe(
    "Named by the import, not in Kuma yet",
  );
  expect(
    [...(section?.querySelectorAll("li") ?? [])].map((row) => row.textContent?.trim()),
  ).toEqual(["gitea", "jira (tunnel)"]);
  // Nothing is attached, so there is no *Monitoring* section over it (#445).
  expect(target.querySelector(".watch")).toBeNull();
});

/**
 * The acceptance criterion, at the dialog: choosing a file **previews** — it
 * does not import — and the answer is drawn in its three groups.
 *
 * The count in every heading is the preview's own, so a view that drew the
 * headings and dropped the lists would fail here, and so would one that put
 * a group's entries under another group's name.
 */
test("choosing a file previews it and draws the three groups", async () => {
  const calls = render();
  await settle();

  button("Import")?.click();
  flushSync();
  expect(target.querySelector('input[type="file"]')).not.toBeNull();
  expect(calls.previewed).toEqual([]);

  await choose('{"name":"knobas test estate"}');

  expect(calls.previewed).toEqual(['{"name":"knobas test estate"}']);
  expect(calls.applied).toEqual([]);
  expect(groups()).toEqual([
    "New — 2",
    "Would change — 1",
    "Monitors to attach — 1",
    // Issue #445: a name the mirror lacks is kept **and reported**. Reported
    // on every preview, not only on the one that first kept it — an unchanged
    // file previewed a second time has nothing in the two groups above and
    // still has this one.
    "Named but not in Kuma — 1",
    "Already in the tree — 1",
  ]);

  const listed = [...target.querySelectorAll(".grp .lst .nm")].map((cell) =>
    cell.textContent?.trim(),
  );
  expect(listed).toEqual([
    "knobas-teamcity",
    "Jira (tunnel)",
    "knobas test estate",
    "knobas test estate",
    // The unresolved group's own row (#445), which names the asset waiting on
    // the name — the fourth `.nm` in the dialog and the fifth entry here.
    "knobas test estate",
  ]);

  // The unresolved row names the asset and the name that found nothing, so a
  // reader can go and look at either.
  const waiting = [...target.querySelectorAll(".grp")].find(
    (group) => group.querySelector("h3")?.textContent?.includes("Named but not in Kuma") ?? false,
  );
  expect(
    [...(waiting?.querySelectorAll("li") ?? [])].map((row) =>
      (row.textContent ?? "").replace(/\s+/g, " ").trim(),
    ),
  ).toEqual(["knobas test estate jira (tunnel)"]);
});

/**
 * Story 24, said **before** the write: the property the reader edited is drawn
 * as staying, and the one nobody touched is drawn as changing.
 *
 * The two lines are asserted together because a dialog that drew every change
 * the same way would satisfy either one alone — and the whole promise is that
 * a reader can tell, from the preview, which of their own values a file is
 * about to leave alone.
 */
test("the preview says which value wins on each changed property", async () => {
  render();
  await settle();
  button("Import")?.click();
  flushSync();
  await choose("{}");

  const lines = [...target.querySelectorAll(".props > li")].map(
    (row) => row.textContent?.replace(/\s+/g, " ").trim() ?? "",
  );
  expect(lines).toEqual([
    "Location nbg1 → hel1",
    "Provider Hetzner, and my desk stays — you edited it here Hetzner Cloud",
    "monitors + knobas-jira",
  ]);
});

/**
 * Import sends the file the reader chose, and the Tree re-reads afterwards.
 *
 * The re-read is the assertion that matters: an import creates assets under
 * an address that has not changed, so a view that patched its own columns
 * would draw an estate nobody answered with. The count is *more* reads after
 * the import than before it, rather than an exact number, because how many
 * columns a walk opens is `AssetsView.test.svelte.ts`' subject.
 */
test("Import applies the chosen file and the Tree re-reads", async () => {
  const calls = render();
  await settle();
  button("Import")?.click();
  flushSync();
  await choose('{"assets":[]}');

  const before = calls.reads;
  const apply = [...target.querySelectorAll("button")].find(
    (candidate) => candidate.textContent?.trim() === "Import" && candidate.closest(".dlg") !== null,
  );
  expect(apply?.disabled).toBe(false);
  apply?.click();
  await settle();

  expect(calls.applied).toEqual(['{"assets":[]}']);
  expect(target.querySelector('input[type="file"]')).toBeNull();
  expect(calls.reads).toBeGreaterThan(before);
});

/**
 * The chooser (#508): **where the estate file comes from**, and its id on both
 * calls.
 *
 * Two entries since #509 — the file a person picks off the disk, and the hcloud
 * importer — and the Docker one (spec #491, story 67) adds the third. What is
 * asserted is the option's *value* as well as its label, because the value is
 * what crosses the bridge and a chooser drawing the right words over the wrong
 * id would be a preview matched by the wrong rule.
 *
 * And on **both** calls, because the producer is half of what decides the
 * plan: an apply that dropped it would write a plan the reader was never
 * shown. `assets::preview_import` and `assets::apply_import` refuse a producer
 * they do not know, so the id itself is checked at its own seam
 * (`crates/knobas-app/tests/it/assets_ipc.rs`) and not spelled out twice here.
 */
test("the chooser offers the estate file and sends it with both calls", async () => {
  const calls = render();
  await settle();
  button("Import")?.click();
  flushSync();

  const chooser = target.querySelector<HTMLSelectElement>(".dlg select");
  expect(chooser).not.toBeNull();
  expect([...(chooser?.options ?? [])].map((option) => [option.value, option.text])).toEqual([
    ["estate_file", "Estate file"],
    ["hcloud", "Hetzner Cloud"],
    ["docker", "Docker host"],
  ]);
  expect(chooser?.value).toBe("estate_file");

  await choose('{"assets":[]}');
  const apply = [...target.querySelectorAll("button")].find(
    (candidate) => candidate.textContent?.trim() === "Import" && candidate.closest(".dlg") !== null,
  );
  apply?.click();
  await settle();

  expect(calls.previewed.length).toBe(1);
  expect(calls.applied.length).toBe(1);
  expect(calls.producers).toEqual(["estate_file", "estate_file"]);
});

/**
 * A file the backend refuses is refused **in the dialog**, in the backend's
 * own words, and the Import button stays out of reach.
 *
 * The refusal is the one the backend actually sends — a file that is not an
 * estate file — because the frontend has no rule of its own about what an
 * estate file is and must not grow one.
 */
test("a file the backend refuses says so in the dialog and cannot be applied", async () => {
  const calls = render({
    code: "invalid",
    message: "this is not an estate file: expected value at line 1 column 1",
  });
  await settle();
  button("Import")?.click();
  flushSync();
  await choose("{ this is not json");

  expect(target.querySelector('[role="alert"]')?.textContent).toContain("not an estate file");
  expect(groups()).toEqual([]);
  const apply = [...target.querySelectorAll("button")].find(
    (candidate) => candidate.textContent?.trim() === "Import" && candidate.closest(".dlg") !== null,
  );
  expect(apply?.disabled).toBe(true);
  expect(calls.applied).toEqual([]);
});

// ---------------------------------------------------------------------------
// The hcloud importer's half of the dialog (#509).
//
// The producer itself, the file it builds and the origin-key match are the
// backend's and are pinned at their own seams
// (`crates/knobas-app/tests/it/assets_ipc.rs` against a recording,
// `just estate-live` against the real Hetzner). What is here is what only this
// surface can get wrong: that the token is asked for **once**, that *land
// under* is asked **only when a new server exists**, and that what came back is
// offered for download.
// ---------------------------------------------------------------------------

/** Pick one of the chooser's importer entries, the way a reader picks it. */
function chooseImporter(id = "hcloud") {
  const chooser = target.querySelector<HTMLSelectElement>(".dlg select");
  if (!chooser) throw new Error("the dialog draws no chooser");
  chooser.value = id;
  chooser.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
}

/** The token field, or `null` when the dialog is not asking for one. */
function tokenField(): HTMLInputElement | null {
  return target.querySelector<HTMLInputElement>('.dlg input[type="password"]');
}

/**
 * **The token is asked for once.**
 *
 * The first run comes back `token_needed` and no field was on screen before it:
 * the dialog does not offer a credential box on the chance that one is wanted,
 * because a token the keychain already holds is one nobody should be asked to
 * type. The reader types one, the run succeeds, and the field is gone — which
 * is the half a stub answering the same thing twice could not tell.
 *
 * And the token is sent **only on the call the reader typed it for**: the
 * second call carries `null`, because after that the keychain is where it
 * lives.
 */
test("an importer asks for the token once, and not before it is owed", async () => {
  script = [{ state: "token_needed" }, { state: "ready", file: FILE, new_servers: [], skipped: [] }];
  const calls = render();
  await settle();
  button("Import")?.click();
  flushSync();

  chooseImporter();
  expect(tokenField()).toBeNull();
  expect(target.querySelector('input[type="file"]')).toBeNull();

  button("Read Hetzner Cloud")?.click();
  await settle();
  expect(calls.produces).toEqual([["hcloud", null, null]]);
  const field = tokenField();
  expect(field).not.toBeNull();

  field!.value = "a-hetzner-token";
  field!.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  button("Read Hetzner Cloud")?.click();
  await settle();

  expect(calls.produces).toEqual([
    ["hcloud", null, null],
    ["hcloud", "a-hetzner-token", null],
  ]);
  expect(tokenField()).toBeNull();
  expect(calls.previewed).toEqual([FILE]);
  expect(calls.producers).toEqual(["hcloud"]);
});

/**
 * **Land under is asked only when a new server exists.**
 *
 * Both halves, because *only* is the word under test: the run whose answer is
 * `ready` draws no picker at all — which is the run `just estate-live` makes
 * against the real estate — and the run whose answer is `landing_needed` draws
 * one, names the servers it is about, and sends back where the reader walked
 * to.
 */
test("land under is asked only when the importer found a server the estate lacks", async () => {
  script = [{ state: "ready", file: FILE, new_servers: [], skipped: [] }];
  const quiet = render();
  await settle();
  button("Import")?.click();
  flushSync();
  chooseImporter();
  button("Read Hetzner Cloud")?.click();
  await settle();

  expect(quiet.produces).toEqual([["hcloud", null, null]]);
  expect(target.querySelector(".dlg .pick")).toBeNull();
  expect(target.textContent).not.toContain("Land under");

  unmount(app!);
  app = undefined;
  script = [
    { state: "token_needed" },
    { state: "landing_needed", servers: ["knobas-scratch"] },
    { state: "ready", file: FILE, new_servers: ["knobas-scratch"], skipped: [] },
    { state: "landing_needed", servers: ["knobas-scratch"] },
    { state: "ready", file: FILE, new_servers: ["knobas-scratch"], skipped: [] },
  ];
  const asked = render();
  await settle();
  button("Import")?.click();
  flushSync();
  chooseImporter();
  button("Read Hetzner Cloud")?.click();
  await settle();
  tokenField()!.value = "a-hetzner-token";
  tokenField()!.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  button("Read Hetzner Cloud")?.click();
  await settle();

  // The token went with the run that reached the far end, so the field is
  // gone even though no file has come back yet: `landing_needed` is an answer
  // from a credential that worked, and a box still holding it through the next
  // step would ask again for something the keychain now has. Found by the
  // `?fake-ipc` walk, which is why it is asserted here.
  expect(tokenField()).toBeNull();
  expect(target.textContent).toContain("knobas-scratch");
  expect(target.querySelector(".dlg .pick")).not.toBeNull();

  // **The picker opens at the top of the estate, and the top is an answer.**
  // This is the half that was missing until the second ruling of 2026-09-08:
  // the walk below steps into an asset first, so nothing ever pressed the
  // button the picker opens on. Standing here sends `{ parent: null }` — the
  // spelling that means *the top* — and the run reaches `ready`. A dialog
  // sending a bare `null` would be asking the same question again, which on an
  // estate with no asset to walk into is the only thing its only button can do.
  button("Put them in the top of the estate")?.click();
  await settle();
  expect(asked.produces.at(-1)).toEqual(["hcloud", null, { parent: null }]);
  expect(target.querySelector(".dlg .pick")).toBeNull();

  // …and then the walk-in, which is the ordinary case.
  button("Read Hetzner Cloud")?.click();
  await settle();
  // The picker walks the estate: the top level holds the one asset this
  // fixture has, and standing on it is what says where the servers land.
  const into = [...target.querySelectorAll<HTMLButtonElement>(".dlg .into")];
  expect(into.map((row) => row.textContent?.trim())).toEqual(["knobas test estate"]);
  into[0]?.click();
  await settle();

  button("Put them in knobas test estate")?.click();
  await settle();
  expect(asked.produces).toEqual([
    ["hcloud", null, null],
    ["hcloud", "a-hetzner-token", null],
    ["hcloud", null, { parent: null }],
    ["hcloud", null, null],
    ["hcloud", null, { parent: "asset:knobas-estate" }],
  ]);
});

/**
 * **What was produced is offered for download**, under a name that says which
 * importer made it, and with the file's own text behind the link.
 *
 * The href is a blob URL, so what is asserted is that there is one and that the
 * anchor really is a download rather than a navigation — the bytes behind it
 * are the string the dialog was handed, which is the same string it sent to the
 * preview and `calls.previewed` already pins.
 */
test("the produced file is offered for download", async () => {
  script = [{ state: "ready", file: FILE, new_servers: ["knobas-scratch"], skipped: [] }];
  const calls = render();
  await settle();
  button("Import")?.click();
  flushSync();
  chooseImporter();
  button("Read Hetzner Cloud")?.click();
  await settle();

  const link = target.querySelector<HTMLAnchorElement>(".dlg a.dl");
  expect(link).not.toBeNull();
  expect(link?.getAttribute("download")).toBe("hcloud-estate.json");
  expect(link?.getAttribute("href")).toMatch(/^blob:/);
  expect(link?.textContent?.trim()).toBe("Download hcloud-estate.json");
  expect(target.textContent).toContain("1 is new");
  expect(calls.previewed).toEqual([FILE]);
});

/**
 * **The Docker importer asks nothing** (#510): one press, one file.
 *
 * The two questions the hcloud half is built around are the two this producer
 * structurally cannot ask — it has no credential, and a container lands under
 * the engine whose context found it (spec #491, story 68) — so what this test
 * is about is that neither is drawn: no token field, no picker, and the token
 * argument goes over as `null` rather than as an empty string the backend would
 * have to trim.
 *
 * And the **skipped** engines are drawn. That group is this producer's alone: a
 * container engine in the tree carrying no `docker_context` is one the run
 * could not read, and the produced file cannot tell that apart from an engine
 * holding no containers. A reader hunting for a container that is in no group
 * has nowhere else to find out why.
 */
test("the Docker importer asks for no token and no landing, and names what it skipped", async () => {
  script = [
    {
      state: "ready",
      file: FILE,
      new_servers: [],
      skipped: ["Docker engine (OrbStack)"],
    },
  ];
  const calls = render();
  await settle();
  button("Import")?.click();
  flushSync();

  chooseImporter("docker");
  expect(tokenField()).toBeNull();
  expect(target.querySelector('input[type="file"]')).toBeNull();

  button("Read Docker host")?.click();
  await settle();

  expect(calls.produces).toEqual([["docker", null, null]]);
  expect(tokenField()).toBeNull();
  expect(target.querySelector(".dlg .pick")).toBeNull();
  expect(target.textContent).not.toContain("Land under");
  expect(calls.previewed).toEqual([FILE]);
  expect(calls.producers).toEqual(["docker"]);

  expect(target.textContent).toContain("1 container engine was");
  expect(target.textContent).toContain("Docker engine (OrbStack)");
  expect(target.querySelector<HTMLAnchorElement>(".dlg a.dl")?.getAttribute("download")).toBe(
    "docker-estate.json",
  );
});

/**
 * **Changing the chooser clears what the previous producer got to.**
 *
 * A preview is a plan drawn under one producer's **origin key** — the second
 * matching rule (#508) — so a preview carried across the chooser would offer
 * *Import* on a plan the backend is not about to run. The Import button going
 * back to disabled is the part a reader can act on, and the groups going with
 * it is what says the plan was dropped rather than hidden.
 *
 * Written because the `?fake-ipc` walk could see it and no test could: the
 * header claimed this and a mutant deleting the handler survived all nine
 * cases before it.
 */
test("choosing another producer drops the preview the last one drew", async () => {
  const calls = render();
  await settle();
  button("Import")?.click();
  flushSync();

  await choose('{"assets":[]}');
  expect(calls.previewed).toEqual(['{"assets":[]}']);
  expect(groups().length).toBeGreaterThan(0);
  const importButton = () =>
    [...target.querySelectorAll<HTMLButtonElement>(".dlg button")].find(
      (candidate) => candidate.textContent?.trim() === "Import",
    );
  expect(importButton()?.disabled).toBe(false);

  chooseImporter();
  await settle();

  expect(groups()).toEqual([]);
  expect(importButton()?.disabled).toBe(true);
  expect(target.querySelector(".dlg a.dl")).toBeNull();
  // And nothing was sent on the way past: the chooser is a choice, not a run.
  expect(calls.previewed).toEqual(['{"assets":[]}']);
  expect(calls.produces).toEqual([]);
});

/**
 * **A token the far end refuses puts the field back**, which is the second half
 * of *asks for a token once* (story 62).
 *
 * The first half is above: once a token works, the backend keeps it and nobody
 * is asked again. This is what happens when a token that used to work stops —
 * revoked in Hetzner, expired, the project moved. There is nowhere else to
 * re-enter it: an importer is not a source (ADR-0015), so it has no row in the
 * sources view and no *Re-enter* strip, and a dialog with no way back would
 * leave the reader holding a keychain item they cannot replace.
 *
 * The refusal is `unauthorized` specifically and not any failure: the dialog
 * reads `IpcError.code`, and putting the field up for an unreachable Hetzner or
 * a database that is down would ask for a credential that was never the
 * problem. Both directions are asserted here, which is what makes the branch a
 * branch rather than a `catch`.
 */
test("a token the far end refuses puts the field back, and another fault does not", async () => {
  script = [
    { state: "token_needed" },
    { state: "ready", file: FILE, new_servers: [], skipped: [] },
    { state: "ready", file: FILE, new_servers: [], skipped: [] },
  ];
  const calls = render();
  await settle();
  button("Import")?.click();
  flushSync();
  chooseImporter();

  button("Read Hetzner Cloud")?.click();
  await settle();
  tokenField()!.value = "a-token-that-worked";
  tokenField()!.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  button("Read Hetzner Cloud")?.click();
  await settle();
  expect(tokenField()).toBeNull();

  // Hetzner stops accepting it. The refusal is in the dialog, in the backend's
  // own words, and the field is back with it.
  script = [{ code: "unauthorized", message: "unable to authenticate", source_id: null }];
  rejectNextProduce = true;
  button("Read Hetzner Cloud")?.click();
  await settle();
  expect(target.textContent).toContain("unable to authenticate");
  expect(tokenField()).not.toBeNull();

  // …and a fault that is not about the credential leaves it alone: the reader
  // is told what went wrong, and not asked for a token that was never at fault.
  script = [{ code: "unreachable", message: "hetzner did not answer", source_id: null }];
  rejectNextProduce = true;
  // The field is on screen from the refusal above, so this is asserted from a
  // run that starts *without* one: choosing the producer again clears it.
  chooseImporter();
  await settle();
  expect(tokenField()).toBeNull();
  button("Read Hetzner Cloud")?.click();
  await settle();
  expect(target.textContent).toContain("hetzner did not answer");
  expect(tokenField()).toBeNull();
  expect(calls.produces.length).toBe(4);
});
