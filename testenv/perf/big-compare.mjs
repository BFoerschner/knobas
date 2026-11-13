// Big compares for `open.mjs --set inner` and `compare.mjs` (the compare pass of PR #2479, 2026-11-13; README, *Big
// compares*): two scratch repositories on the live Gitea, one cloned by knobas and one read from the forge only, each a
// `main` of 10,000 files and branches off it that change 300, 3,000 and 10,000 of them, read through the profile's Gitea
// source on the windowless backend. `up` makes them for the run and `down` takes every piece away again, so the live
// products are left as they were.
//
// The repositories are made with `git fast-import` at fixed dates, so every run pushes the same commits.
//
//   node testenv/perf/big-compare.mjs up --port <backend> --dir <scratch dir>   # make, sync, clone; leaves a state file
//   node testenv/perf/big-compare.mjs down --dir <scratch dir>                  # put everything back
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

export const ORG = "knobas-perf-big-compare";
/** The profile's Gitea source the repositories are read through (`live-gitea` in the scale profile), set by `up`. */
let source = process.env.KNOBAS_PERF_GITEA_SOURCE ?? "live-gitea";
export const cloned = () => `${source}:${ORG}/cloned`;
export const forge = () => `${source}:${ORG}/forge`;
/** `main`'s files: 100 folders of 100. */
const DIRS = 100;
const PER_DIR = 100;
/** The branches: each changes the first `n` files of `main`, in three commits; the `-b` twins give the 4x run reads of its own. */
export const BRANCHES = [
  ["files-300", 300],
  ["files-3000", 3000],
  ["files-10000", 10000],
  ["files-300-b", 300],
  ["files-3000-b", 3000],
  ["files-10000-b", 10000],
];
/** Another base for *Into*: `main` and one commit beside it. */
export const OTHER_BASE = "base-b";

const here = new URL(".", import.meta.url).pathname;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** The live Gitea and this worktree's seed token (`./seed` writes it; README, *One environment, one owner at a time*). */
function gitea() {
  let state = {};
  try { state = JSON.parse(readFileSync(join(here, "..", "seed-state.json"), "utf8")).gitea ?? {}; } catch {}
  const token = process.env.KNOBAS_GITEA_TOKEN ?? state.token;
  const url = process.env.KNOBAS_GITEA_URL ?? state.url ?? "http://127.0.0.1:3000";
  if (!token) throw new Error("big-compare: no Gitea token (testenv/seed-state.json or KNOBAS_GITEA_TOKEN); run ./seed first");
  return { token, url, user: state.admin_user ?? "knobas" };
}

async function api(g, method, path, body) {
  return fetch(`${g.url}/api/v1${path}`, { method, headers: { Authorization: `token ${g.token}`, "Content-Type": "application/json" }, body: body ? JSON.stringify(body) : undefined });
}

const pathOf = (i) => `pkg/m${String(Math.floor(i / PER_DIR)).padStart(2, "0")}/file${String(i).padStart(5, "0")}.ts`;
const original = (i) =>
  Array.from({ length: 12 }, (_, l) => (l === 5 ? `export const value${i} = ${i};` : `// ${pathOf(i)}, line ${l + 1}`)).join("\n") + "\n";
const changed = (i, branch, step) =>
  original(i).replace(`export const value${i} = ${i};`, `export const value${i} = "${branch}";`) + `export const step${i} = ${step}; // ${branch}\n`;

/** The repository as a fast-import stream: `main`, each branch off it in three commits, and the other base. */
function stream() {
  const out = [];
  const file = (path, text) => out.push(`M 100644 inline ${path}\ndata ${Buffer.byteLength(text)}\n${text}\n`);
  const commit = (ref, at, message, from) => {
    out.push(`commit ${ref}\ncommitter knobas <knobas@example.invalid> ${at} +0000\ndata ${Buffer.byteLength(message)}\n${message}`);
    if (from) out.push(`from ${from}\n`);
  };
  const T = 1794528000;
  commit("refs/heads/main", T, "Ten thousand files\n");
  for (let i = 0; i < DIRS * PER_DIR; i++) file(pathOf(i), original(i));
  file("README.md", "# big-compare\n\nA `main` of 10,000 files and branches that change 300, 3,000 and 10,000 of them (testenv/perf/big-compare.mjs).\n");
  out.push("\n");
  out.push(`reset refs/tags/knobas-main\nfrom refs/heads/main\n\n`);
  BRANCHES.forEach(([name, n], b) => {
    const third = Math.ceil(n / 3);
    for (let step = 0; step < 3; step++) {
      commit(`refs/heads/${name}`, T + 3600 * (b + 1) + step * 60, `${name}: change files ${step * third + 1} to ${Math.min(n, (step + 1) * third)}\n`, step === 0 ? "refs/tags/knobas-main" : null);
      // A file of the branch's own, first in the file list (root files first): what tells one branch's list from another's.
      if (step === 0) file(`A-${name}.md`, `# ${name}\n\nChanges the first ${n} files of main.\n`);
      for (let i = step * third; i < Math.min(n, (step + 1) * third); i++) file(pathOf(i), changed(i, name, step + 1));
      out.push("\n");
    }
  });
  commit(`refs/heads/${OTHER_BASE}`, T + 86400, "Another base\n", "refs/tags/knobas-main");
  file("README.md", "# big-compare\n\nAnother base for Into.\n");
  out.push("\n");
  return out.join("");
}

async function call(port, cmd, args = {}) {
  const res = await fetch(`http://127.0.0.1:${port}/invoke`, { method: "POST", body: JSON.stringify({ cmd, args }) });
  const body = await res.json();
  if ("err" in body) throw new Error(`${cmd}: ${JSON.stringify(body.err)}`);
  return body.ok;
}

let made = null;
const stateFile = (dir) => join(dir, "big-compare-state.json");

/**
 * Makes the two repositories, the source that syncs them on the backend at `port`, and the clone (in `dir`/clones,
 * the clones root for the run). Each step that can be left half done by a killed run is undone first. With `clone:
 * false` nothing is cloned (a run that measures the forge alone).
 */
export async function up({ port, dir, clone = true }) {
  const g = gitea();
  made = { port, dir, g, root: undefined };
  mkdirSync(dir, { recursive: true });
  const src = join(dir, "big-compare-src");
  rmSync(src, { recursive: true, force: true });
  mkdirSync(src, { recursive: true });
  execFileSync("git", ["init", "-q", "--bare", src]);
  execFileSync("git", ["-C", src, "fast-import", "--quiet"], { input: stream(), maxBuffer: 1 << 30 });
  const org = await api(g, "POST", "/orgs", { username: ORG, visibility: "public" });
  if (!org.ok && org.status !== 422) throw new Error(`big-compare: Gitea refused the org (${org.status})`);
  const refs = ["main", OTHER_BASE, ...BRANCHES.map(([name]) => name)];
  for (const repo of ["cloned", "forge"]) {
    const r = await api(g, "POST", `/orgs/${ORG}/repos`, { name: repo, default_branch: "main" });
    if (!r.ok && r.status !== 409) throw new Error(`big-compare: Gitea refused ${repo} (${r.status})`);
    const at = new URL(g.url);
    const remote = `${at.protocol}//${g.user}:${g.token}@${at.host}/${ORG}/${repo}.git`;
    execFileSync("git", ["-C", src, "-c", "credential.helper=", "push", "-q", "--force", remote, ...refs.map((ref) => `${ref}:${ref}`)], { stdio: "ignore" });
  }
  const sources = await call(port, "list_sources");
  const id = process.env.KNOBAS_PERF_GITEA_SOURCE ?? "live-gitea";
  const live = sources.find((s) => s.id === id && Array.isArray(s.config?.owners));
  if (!live) throw new Error(`big-compare: the profile has no Gitea source ${id} with owners (KNOBAS_PERF_GITEA_SOURCE names another)`);
  const owners = live.config.owners.filter((o) => o !== ORG);
  made.source = { id: live.id, config: { ...live.config, owners } };
  source = live.id;
  save(dir);
  await call(port, "update_source", { id: live.id, patch: { config: { ...live.config, owners: [...owners, ORG] } } });
  await call(port, "sync_now", { sourceId: live.id }).catch(() => {});
  for (let i = 0; i < 300; i++) {
    // Each repository and every branch: Compare predicts at the clone from the tips the mirror holds for its two ends.
    const ids = [cloned(), forge(), ...[cloned(), forge()].flatMap((repo) => refs.map((ref) => `${repo}@refs/heads/${ref}`))];
    const briefs = await call(port, "entity_briefs", { entityIds: ids }).catch(() => []);
    const status = (await call(port, "sync_status")).find((s) => s.source_id === live.id);
    if (briefs.length === ids.length && status && !status.running) break;
    if (i === 299) throw new Error(`big-compare: the source never synced the two repositories and their branches (${briefs.length} of ${ids.length})`);
    if (status && !status.running && i % 10 === 9) await call(port, "sync_now", { sourceId: live.id }).catch(() => {});
    await sleep(1000);
  }
  if (!clone) return;
  made.root = (await call(port, "entity_checkout", { entityId: cloned() })).clones_root ?? null;
  save(dir);
  await call(port, "set_clones_root", { path: join(dir, "clones") });
  await call(port, "clone_repo", { entityId: cloned() });
  for (let i = 0; i < 300; i++) {
    if (await call(port, "entity_local_tree", { entityId: cloned() }).catch(() => null)) return;
    await sleep(1000);
  }
  throw new Error("big-compare: the clone never answered its tree");
}

function save(dir) {
  const { port, source: src, root } = made;
  writeFileSync(stateFile(dir), JSON.stringify({ port, source: src, root }));
}

/**
 * Takes away what `up` made: the org from the Gitea source's owners, the run's clones root, the repositories and the
 * org. The source's next sync sweeps the two repositories out of the mirror; the profile is a scratch copy anyway.
 */
export async function down() {
  if (made === null) return;
  const { port, g, root, source: src, dir } = made;
  made = null;
  if (src) await call(port, "update_source", { id: src.id, patch: { config: src.config } }).catch((e) => console.log(`big-compare: ${e.message}`));
  if (root !== undefined) await call(port, "set_clones_root", { path: root }).catch((e) => console.log(`big-compare: ${e.message}`));
  for (const repo of ["cloned", "forge"]) await api(g, "DELETE", `/repos/${ORG}/${repo}`).catch(() => {});
  await api(g, "DELETE", `/orgs/${ORG}`).catch(() => {});
  rmSync(join(dir, "big-compare-src"), { recursive: true, force: true });
  rmSync(join(dir, "clones"), { recursive: true, force: true });
  rmSync(stateFile(dir), { force: true });
  console.log("big-compare: removed the repositories and the org, and the Gitea source's owners are back");
}

/**
 * The places, at one throttle: each branch's compare opened in the cloned repository (`compare-*`, read at the clone)
 * and in the forge's (`forge-compare-*`), the 4x run its own twins, so each first open is a read knobas never made. The
 * page is useful once its changed files are listed and the first file's lines drawn.
 */
export function places(rate, shown) {
  const enc = encodeURIComponent;
  const R = (repo) => `#/repo/${enc(repo)}`;
  const twin = rate === 1 ? "" : "-b";
  const place = (name, repo, branch) => ({
    name,
    start: R(repo),
    go: `${R(repo)}/compare/${enc(branch)}`,
    hash: `${R(repo)}/compare/`,
    useful: [[`${shown} .tf-cdiff .tf-flist button[title="A-${branch}.md"]`, 1], [`${shown} .tf-cdiff .tf-diff[data-file="A-${branch}.md"] .tf-dbody`, 1]],
    marks: { files: [[`${shown} .tf-cdiff .tf-flist button[title="A-${branch}.md"]`, 1]] },
    awayGo: R(repo),
    inner: true,
  });
  return [
    place("compare-300", cloned(), `files-300${twin}`),
    place("compare-3000", cloned(), `files-3000${twin}`),
    place("compare-10000", cloned(), `files-10000${twin}`),
    place("forge-compare-300", forge(), `files-300${twin}`),
    place("forge-compare-3000", forge(), `files-3000${twin}`),
    place("forge-compare-10000", forge(), `files-10000${twin}`),
  ];
}

// The command line: `up` and `down` for a run of `compare.mjs` or a pass by hand.
if (process.argv[1] && new URL(import.meta.url).pathname === process.argv[1]) {
  const args = process.argv.slice(2);
  const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
  const dir = opt("--dir");
  if (!dir) { console.log("big-compare.mjs up|down --dir <scratch dir> [--port <backend>] [--no-clone]"); process.exit(2); }
  if (args[0] === "up") {
    try {
      await up({ port: Number(opt("--port")), dir, clone: !args.includes("--no-clone") });
      console.log(`big-compare: ${cloned()} (cloned) and ${forge()} are synced`);
    } catch (e) {
      console.log(String(e.message || e));
      await down();
      process.exit(1);
    }
  } else if (args[0] === "down") {
    const saved = existsSync(stateFile(dir)) ? JSON.parse(readFileSync(stateFile(dir), "utf8")) : {};
    made = { port: Number(opt("--port") ?? saved.port), dir, g: gitea(), root: saved.root, source: saved.source };
    await down();
  } else {
    console.log("big-compare.mjs up|down --dir <scratch dir>");
    process.exit(2);
  }
}
