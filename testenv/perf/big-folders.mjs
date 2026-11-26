// Big folders for `open.mjs --set inner` (the big-folder pass of PR #2479, 2026-11-13; README, *Big folders*): two
// scratch repositories on the live Gitea, one cloned by knobas and one read from the forge only, each with folders of
// 300, 3,000 and 12,000 entries and one ten directories down (about 31,400 entries in all, so the clone's folders past
// the 20,000th are pressed too), read through the profile's Gitea source on the windowless
// backend.
// `up` makes them for the run and `down` takes every piece away again, so the live products are left as they were.
//
// The repositories are made with `git fast-import` at a fixed date, so every run pushes the same commit.
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";

export const ORG = "knobas-perf-big-folders";
/** The profile's Gitea source the repositories are read through (`live-gitea` in the scale profile), set by `up`. */
let source = "live-gitea";
const cloned = () => `${source}:${ORG}/cloned`;
const forge = () => `${source}:${ORG}/forge`;
const DEEP = "deep/a/b/c/d/e/f/g/h/i";
/** Each folder, its entries, and what its files are called. The `-2` twins give the 4x run reads of its own. */
const FOLDERS = [
  ["hundreds", 300, "handler", "ts"],
  ["hundreds-2", 300, "handler", "ts"],
  ["thousands", 3000, "fixture", "json"],
  ["thousands-2", 3000, "fixture", "json"],
  ["huge", 12000, "snapshot", "json"],
  ["huge-2", 12000, "snapshot", "json"],
  [`${DEEP}/leaf`, 800, "leaf", "txt"],
];

const here = new URL(".", import.meta.url).pathname;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** The live Gitea and this worktree's seed token (`./seed` writes it; README, *One environment, one owner at a time*). */
function gitea() {
  let state = {};
  try { state = JSON.parse(readFileSync(join(here, "..", "seed-state.json"), "utf8")).gitea ?? {}; } catch {}
  const token = process.env.KNOBAS_GITEA_TOKEN ?? state.token;
  const url = process.env.KNOBAS_GITEA_URL ?? state.url ?? "http://127.0.0.1:3000";
  if (!token) throw new Error("big-folders: no Gitea token (testenv/seed-state.json or KNOBAS_GITEA_TOKEN); run ./seed first");
  return { token, url, user: state.admin_user ?? "knobas" };
}

async function api(g, method, path, body) {
  const res = await fetch(`${g.url}/api/v1${path}`, { method, headers: { Authorization: `token ${g.token}`, "Content-Type": "application/json" }, body: body ? JSON.stringify(body) : undefined });
  return res;
}

/** The repository as a fast-import stream: every folder above, a note at each level of the deep one, a README. */
function stream() {
  const parts = [];
  const file = (path, text) => {
    parts.push(`M 100644 inline ${path}\ndata ${Buffer.byteLength(text)}\n${text}\n`);
  };
  for (const [dir, n, prefix, ext] of FOLDERS)
    for (let i = 1; i <= n; i++) file(`${dir}/${prefix}-${String(i).padStart(5, "0")}.${ext}`, `{"id": ${i}, "folder": "${dir}"}\n`);
  const levels = DEEP.split("/");
  for (let i = 1; i <= levels.length; i++) file(`${levels.slice(0, i).join("/")}/notes.md`, `level ${i}\n`);
  file("src/main.go", "package main\n\nfunc main() {}\n");
  file("README.md", "# big-folders\n\nFolders of 300, 3,000 and 12,000 entries and one ten directories down (testenv/perf/big-folders.mjs).\n");
  const message = "Big folders\n";
  return `commit refs/heads/main\ncommitter knobas <knobas@example.invalid> 1794528000 +0000\ndata ${message.length}\n${message}${parts.join("")}\n`;
}

async function call(port, cmd, args = {}) {
  const res = await fetch(`http://127.0.0.1:${port}/invoke`, { method: "POST", body: JSON.stringify({ cmd, args }) });
  const body = await res.json();
  if ("err" in body) throw new Error(`${cmd}: ${JSON.stringify(body.err)}`);
  return body.ok;
}

let made = null;

/**
 * Makes the two repositories, the source that syncs them on the backend at `port`, and the clone (in `dir`/clones,
 * the clones root for the run). Each step that can be left half done by a killed run is undone first.
 */
export async function up({ port, dir }) {
  const g = gitea();
  made = { port, dir, g, root: undefined };
  const src = join(dir, "big-folders-src");
  rmSync(src, { recursive: true, force: true });
  mkdirSync(src, { recursive: true });
  execFileSync("git", ["init", "-q", "--bare", src]);
  execFileSync("git", ["-C", src, "fast-import", "--quiet"], { input: stream() });
  const org = await api(g, "POST", "/orgs", { username: ORG, visibility: "public" });
  if (!org.ok && org.status !== 422) throw new Error(`big-folders: Gitea refused the org (${org.status})`);
  for (const repo of ["cloned", "forge"]) {
    const made = await api(g, "POST", `/orgs/${ORG}/repos`, { name: repo, default_branch: "main" });
    if (!made.ok && made.status !== 409) throw new Error(`big-folders: Gitea refused ${repo} (${made.status})`);
    const at = new URL(g.url);
    const remote = `${at.protocol}//${g.user}:${g.token}@${at.host}/${ORG}/${repo}.git`;
    execFileSync("git", ["-C", src, "-c", "credential.helper=", "push", "-q", "--force", remote, "main"], { stdio: "ignore" });
  }
  // The scratch org joins the profile's own Gitea source for the run (its owners come back in `down`), so the
  // repositories are read at the address and with the credential the scale profile reads Gitea with.
  const sources = await call(port, "list_sources");
  // By id: Forgejo's source is a `gitea` adapter too.
  const id = process.env.KNOBAS_PERF_GITEA_SOURCE ?? "live-gitea";
  const live = sources.find((s) => s.id === id && Array.isArray(s.config?.owners));
  if (!live) throw new Error(`big-folders: the profile has no Gitea source ${id} with owners (KNOBAS_PERF_GITEA_SOURCE names another)`);
  const owners = live.config.owners.filter((o) => o !== ORG);
  made.source = { id: live.id, config: { ...live.config, owners } };
  source = live.id;
  await call(port, "update_source", { id: live.id, patch: { config: { ...live.config, owners: [...owners, ORG] } } });
  await call(port, "sync_now", { sourceId: live.id }).catch(() => {});
  // Ten minutes: a new owner makes the source's next run walk the whole scale estate, five minutes on the WSL box
  // (the WSL block, 2026-11-13; four timed out a run).
  for (let i = 0; i < 600; i++) {
    // Each repository and its `main`: the clone's tree is read at the commit the mirror names for the branch, and a
    // source with many repositories may store a new repository's branches in a later run than the repository.
    const ids = [cloned(), forge(), `${cloned()}@refs/heads/main`, `${forge()}@refs/heads/main`];
    const briefs = await call(port, "entity_briefs", { entityIds: ids }).catch(() => []);
    const status = (await call(port, "sync_status")).find((s) => s.source_id === live.id);
    if (briefs.length === ids.length && status && !status.running) break;
    if (i === 599) throw new Error("big-folders: the source never synced the two repositories and their branches");
    // A run under way when the owners changed read the old ones: ask for the next.
    if (status && !status.running && i % 10 === 9) await call(port, "sync_now", { sourceId: live.id }).catch(() => {});
    await sleep(1000);
  }
  // The clone, under a clones root of the run's own; the profile's own root comes back in `down`.
  made.root = (await call(port, "entity_checkout", { entityId: cloned() })).clones_root ?? null;
  await call(port, "set_clones_root", { path: join(dir, "clones") });
  await call(port, "clone_repo", { entityId: cloned() });
  for (let i = 0; i < 180; i++) {
    if (await call(port, "entity_local_tree", { entityId: cloned() }).catch(() => null)) {
      // Pinned as the repository's checkout, so the next rig's clones root of its own does not hide it (the WSL block,
      // 2026-11-13: the big compares' root left this clone unread, and every folder was read from the forge).
      const { path } = await call(port, "entity_checkout", { entityId: cloned() });
      await call(port, "set_checkout_override", { entityId: cloned(), path });
      made.pinned = true;
      return;
    }
    await sleep(1000);
  }
  throw new Error("big-folders: the clone never answered its tree");
}

/**
 * Takes away what `up` made: the org from the Gitea source's owners, the run's clones root, the repositories and the
 * org. The source's next sync sweeps the two repositories out of the mirror; the profile is a scratch copy anyway.
 */
export async function down() {
  if (made === null) return;
  const { port, g, root, source } = made;
  if (made.pinned) await call(port, "set_checkout_override", { entityId: cloned(), path: null }).catch((e) => console.log(`big-folders: ${e.message}`));
  made = null;
  if (source) await call(port, "update_source", { id: source.id, patch: { config: source.config } }).catch((e) => console.log(`big-folders: ${e.message}`));
  if (root !== undefined) await call(port, "set_clones_root", { path: root }).catch((e) => console.log(`big-folders: ${e.message}`));
  for (const repo of ["cloned", "forge"]) await api(g, "DELETE", `/repos/${ORG}/${repo}`).catch(() => {});
  await api(g, "DELETE", `/orgs/${ORG}`).catch(() => {});
  console.log("big-folders: removed the repositories and the org, and the Gitea source's owners are back");
}

/**
 * The places, at one throttle: each folder pressed in the cloned repository (`folder-*`: no read at all, the rows in
 * the next frame or two), and the 300, 3,000 and 12,000 ones in the forge's (`forge-folder-*`, the 4x run its own
 * twins, so its first press is a read knobas never made). A return folds the folder and presses it again; a round that comes back
 * to the tab with the folder still open folds it first (`undo`).
 */
export function places(rate, shown) {
  const enc = encodeURIComponent;
  const R = (repo) => `#/repo/${enc(repo)}`;
  const row = (path) => `${shown} .tf-ftree [data-path="${path}"]`;
  const place = (name, repo, folder, start = R(repo)) => ({
    name,
    start,
    press: row(folder),
    hash: R(repo),
    useful: [[`${shown} .tf-ftree [data-path^="${folder}/"]`, 10]],
    away: row(folder),
    undo: `${row(folder)}[aria-expanded="true"]`,
    inner: true,
  });
  const twin = rate === 1 ? "" : "-2";
  const notes = `${R(cloned())}/files/${DEEP.split("/").map(enc).join("/")}/notes.md`;
  return [
    place("folder-300", cloned(), "hundreds"),
    place("folder-3000", cloned(), "thousands"),
    place("folder-12000", cloned(), "huge"),
    place("folder-deep", cloned(), `${DEEP}/leaf`, notes),
    place("forge-folder-300", forge(), `hundreds${twin}`),
    place("forge-folder-3000", forge(), `thousands${twin}`),
    place("forge-folder-12000", forge(), `huge${twin}`),
  ];
}
