/**
 * `?fake-ipc`: The estate Import (#439): produce, preview and apply, as a replay of the
 * file over `estate.ts`'s estate.
 */
import type { Produced } from "../../../ipc/assets";
import type { Handlers } from "../fake-tauri";
import { onFakeReset } from "../fake-seed";
import {
  type EstateFileAsset,
  type EstateFileRoute,
  ASSET_HISTORY,
  ESTATE,
  FIXTURE_ESTATE,
  FIXTURE_ROUTES,
  assetFromFile,
  routeFromFile,
  typeOf,
} from "./estate";

/**
 * The file an import was handed, or a refusal.
 *
 * **Two files and no others** since #509: the estate this fixture was built
 * from, and whatever its own importer last produced ({@link PRODUCED_FILE},
 * matched as text). The parse is `JSON.parse` and a shape check and nothing
 * else, where `assets::plan` is a versioned parser with a closed key
 * vocabulary, an id namespace check, a cycle check and a property-kind rule.
 * Re-implementing any of that here would be a second answer to *is this file
 * legal*, and the second answer is the one that goes stale -- so anything else
 * is refused by name.
 */
function estateFileOf(args: Record<string, unknown>) {
  let parsed: unknown;
  try {
    parsed = JSON.parse(String(args.file ?? ""));
  } catch {
    throw { code: "invalid", message: "that file is not JSON", source_id: null };
  }
  const file = parsed as { name?: string; assets?: EstateFileAsset[]; routes?: EstateFileRoute[] };
  const ours = file.name === ESTATE.name || String(args.file ?? "") === PRODUCED_FILE;
  if (!ours || !Array.isArray(file.assets) || !Array.isArray(file.routes)) {
    throw {
      code: "invalid",
      message:
        `this harness replays two imports — "${ESTATE.name}", the estate file it ` +
        `draws its own Tree from, and the one its own importer just produced. ` +
        `Any other file is the real command's to parse.`,
      source_id: null,
    };
  }
  return { assets: file.assets, routes: file.routes };
}

/** The file's entries as the dialog's rows, assets before routes. */
function importEntries(file: { assets: EstateFileAsset[]; routes: EstateFileRoute[] }) {
  return [
    ...file.assets.map((entry) => ({
      id: entry.id,
      kind: "asset",
      name: entry.name,
      type_label: typeOf(entry.type).label,
      parent_id: entry.parent ?? null,
    })),
    ...file.routes.map((route) => ({
      id: route.id,
      kind: "route",
      name: route.name,
      type_label: null,
      parent_id: route.asset,
    })),
  ];
}

/**
 * `produce_estate_file`: the hcloud importer's three answers, without hcloud
 * (#509).
 *
 * **A state machine and not a recording**, because what a walk through this
 * harness can certify is the *dialog*: that the token is asked for once, that
 * *land under* is asked only when a new server exists, and that the file comes
 * back and is offered. So this fixture holds one flag -- whether a token has
 * been given -- and one server the estate does not hold, which is enough to
 * make every branch reachable by hand and none of them reachable twice.
 *
 * **The Docker producer is answered by {@link dockerProduce}, not by this**
 * (#510): it asks for no token and no landing, and answering it here would draw
 * hcloud's two questions over a producer that has neither, which is a walk
 * certifying a dialog nobody ships.
 *
 * What it says nothing about is Hetzner: there is no API here, no origin key
 * and no match. `crates/knobas-app/tests/it/assets_ipc.rs` witnesses the shape
 * against a recording and `just estate-live` witnesses the real system, which
 * is the split stated in `assets::hcloud`'s own header.
 */
let IMPORTER_TOKEN: string | null = null;

/** The server this fixture's hcloud holds and the estate does not. */
const IMPORTER_NEW_SERVER = "knobas-scratch";

/**
 * The exact text the last produce answered with.
 *
 * Held so that {@link estateFileOf} can accept it: that function's rule is
 * *one file and no other*, and since #509 this harness has two of its own --
 * the estate it draws its Tree from, and the file its own importer just made.
 * Compared as **text** rather than by a name or a shape, so widening the rule
 * did not turn it into a second parser.
 */
let PRODUCED_FILE: string | null = null;

/**
 * `produce_estate_file` for the **Docker host** producer (#510).
 *
 * One press, one answer: no token, no *land under*, and a container under an
 * engine this fixture's estate already holds. It also answers with one
 * **skipped** engine, so that the branch of the dialog that state exists for is
 * reachable by hand -- a branch nothing here could reach is a branch nobody
 * would see before it shipped. The name it skips is
 * `asset:orbstack-docker`'s, because a reader recognises it from the Tree; it
 * is **not** a claim about that engine, which since #510 carries a
 * `docker_context` like every other engine in `testenv/estate.json`.
 * No engine in this fixture's estate can honestly be skipped, which is the
 * whole reason this state has to be invented here.
 *
 * As above, it says nothing about docker: there is no CLI here and no match.
 * `crates/knobas-app/tests/it/assets_ipc.rs` drives a stub docker and
 * `just estate-live` drives the real context.
 */
function dockerProduce(): Produced {
  PRODUCED_FILE = JSON.stringify(
    {
      version: 1,
      name: "Docker hosts",
      assets: [
        {
          id: "asset:docker-orbstack/knobas-gitea",
          type: "container",
          name: "knobas-gitea",
          properties: { docker_context: "orbstack", container_name: "knobas-gitea" },
        },
      ],
      routes: [],
    },
    null,
    2,
  );
  return {
    state: "ready",
    file: PRODUCED_FILE,
    new_servers: [],
    skipped: ["Docker engine (OrbStack)"],
  };
}

function estateProduce(args: Record<string, unknown>): Produced {
  if (args.producer === "docker") return dockerProduce();
  const token = typeof args.token === "string" ? args.token.trim() : "";
  if (token !== "") IMPORTER_TOKEN = token;
  if (IMPORTER_TOKEN === null) return { state: "token_needed" };

  // **The argument being absent is the question; a `Landing` is an answer, and
  // `{ parent: null }` is the top of the estate** (`assets::Landing`, #509).
  // Read the way the backend reads it, because reading `{ parent: null }` as
  // *nothing said yet* is exactly the defect the second ruling of 2026-09-08
  // was raised on, and a fixture that repeated it would let the walk go green
  // over it.
  const landing = (args.landUnder ?? null) as { parent?: string | null } | null;
  const held = FIXTURE_ESTATE.some((asset) => asset.name === IMPORTER_NEW_SERVER);
  if (!held && landing === null) {
    return { state: "landing_needed", servers: [IMPORTER_NEW_SERVER] };
  }
  const entry: Record<string, unknown> = {
    id: "asset:hcloud-164750999",
    type: "vm",
    name: IMPORTER_NEW_SERVER,
    properties: {
      hcloud_id: "164750999",
      server_type: "cx23",
      os: "ubuntu-24.04",
      location: "fsn1",
      ip: "203.0.113.9",
    },
  };
  // No `parent` key at all for the top, which is what `FileAsset::parent` reads
  // as an asset at the top of the estate.
  const parent = landing?.parent ?? null;
  if (!held && parent !== null) entry.parent = parent;
  PRODUCED_FILE = JSON.stringify(
    { version: 1, name: "Hetzner Cloud", assets: [entry], routes: [] },
    null,
    2,
  );
  return {
    state: "ready",
    file: PRODUCED_FILE,
    new_servers: held ? [] : [IMPORTER_NEW_SERVER],
    // hcloud never skips: one token either sees a server or does not know it
    // exists.
    skipped: [],
  };
}

/**
 * `preview_estate_import`: what an import of this file would do here.
 *
 * **A replay of one recorded answer, and it says so.** Against the estate as
 * loaded, every one of the file's thirty-two entries is already in the tree —
 * which is criterion 3 of #440 and the state a demo profile is in after its
 * first start. Delete an asset in the harness and the entry moves to *new*,
 * because that much is a set membership and not a rule.
 *
 * **`changes` is always empty, and that is the honest answer here rather than
 * a shortcut.** What belongs in it is decided by `assets::HAND_EDITED` —
 * *"a hand edit is any activity line by the user on that property"* — over
 * `knobas.activity`. This fixture has no activity table and no actor on a
 * property, so it cannot tell a value the file wrote from a value a person
 * typed, and a group composed by guessing would put the dialog's most
 * consequential sentence in front of a reader with nothing behind it. What
 * answers that group is `crates/knobas-app/tests/it/assets_ipc.rs`, over a real
 * PostgreSQL, and the dialog's rendering of all three groups is
 * `AssetsView.import.test.svelte.ts`.
 *
 * **`args.producer` is read by nothing here, and that is deliberate** (#508).
 * The producer decides the **origin key** — the second matching rule, used
 * when a file entry's id is not one the tree holds — and honouring it would be
 * a second implementation of `assets::matched_by_origin_key` living in a
 * fixture, which is the copy that goes stale. So this replay matches by id
 * alone, whichever producer the chooser sent, and what witnesses the rule is
 * the IPC-seam suite over a real PostgreSQL. A walk through this harness
 * certifies the chooser's layout and never the rule behind it.
 */
function estatePreview(args: Record<string, unknown>) {
  const file = estateFileOf(args);
  const held = (id: string) =>
    FIXTURE_ESTATE.some((asset) => asset.id === id) ||
    FIXTURE_ROUTES.some((route) => route.id === id);
  const entries = importEntries(file);
  return {
    name: ESTATE.name,
    known: entries.filter((entry) => held(entry.id)),
    new: entries.filter((entry) => !held(entry.id)),
    changes: [],
    // No monitor in this fixture answers to any name, because there is no
    // mirror here and no Kuma: every name the file gives is therefore
    // unresolved, and the dialog draws that group rather than an empty
    // *Monitors to attach* (#445). Composed from the file rather than
    // recorded, so deleting an asset in the harness moves its names with it.
    monitor_links: [],
    unresolved: file.assets.flatMap((entry) =>
      (entry.monitors ?? []).map((monitor_name) => ({
        asset_id: entry.id,
        asset_name: entry.name,
        monitor_name,
      })),
    ),
  };
}

/**
 * `apply_estate_import`: create what the preview called new, and nothing else.
 *
 * The plan is the preview's, which is the property `assets::apply_import`
 * makes structural -- an entry the preview did not list cannot be written. On
 * an untouched estate that is nothing at all, and the dialog reports six
 * zeroes; after a delete it is the row coming back, with the origin line the
 * real import writes.
 */
function estateApply(args: Record<string, unknown>) {
  const file = estateFileOf(args);
  const wanted = new Set(estatePreview(args).new.map((entry) => entry.id));
  const outcome = {
    assets_created: 0,
    routes_created: 0,
    properties_set: 0,
    properties_kept: 0,
    monitors_kept: 0,
    monitors_linked: 0,
  };
  for (const entry of file.assets) {
    if (!wanted.has(entry.id)) continue;
    FIXTURE_ESTATE.push(assetFromFile(entry));
    ASSET_HISTORY.push({
      entity_id: entry.id,
      verb: "imported",
      detail: { estate: ESTATE.name },
      actor: "import",
    });
    outcome.assets_created += 1;
    outcome.monitors_kept += (entry.monitors ?? []).length;
  }
  for (const route of file.routes) {
    if (!wanted.has(route.id)) continue;
    FIXTURE_ROUTES.push(routeFromFile(route));
    outcome.routes_created += 1;
  }
  return outcome;
}

/** This feature's commands. */
export function handlers(): Handlers {
  return {
    // The Import (#439), as a **replay** and not as a second implementation of
    // the merge rule -- see `estatePreview`. It exists because the estate this
    // fixture draws *is* the file `--demo` imports (#440), so pressing Import
    // and choosing that file is the one gesture a reader will actually make
    // here, and until now it answered "command not found" in red.
    preview_estate_import: (args) => estatePreview(args),
    apply_estate_import: (args) => estateApply(args),
    produce_estate_file: (args) => estateProduce(args),
  };
}

onFakeReset(() => {
  IMPORTER_TOKEN = null;
  PRODUCED_FILE = null;
});
