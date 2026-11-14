// A Gitea-shaped stand-in for the forge-waits pass (2026-11-13), for the Mac while the live products are someone
// else's. It answers, after the delay asked:
//
// - Gitea's compare route, the diff (`?output=diff`) and the commits (`?files=false`), for any two refs: `--files`
//   added files of three lines (the live Gitea took 0.65 to 2.7 s for a first compare without a clone);
// - with `--git <dir>`, a repository's record, a branch's tip, its tree (`git/trees`, a level or recursive), a file's
//   own entry (`contents-ext`) and its bytes (`raw`), for every repository, from that one local repository: what
//   *Files* reads, so a file read from the forge and one read from a clone can be timed against each other;
//
// and every other route 404 at once.
//
//   node testenv/perf/standin-forge.mjs --listen 3911 [--delay 650] [--files 300] [--git <dir>] [--control 3912]
//                                       [--distinct] [--tips <file.json>]
//
// `--distinct` (the stored-trees pass, 2026-11-14) gives each repository objects of its own: every sha the stand-in
// answers for repository `o/r` is `alias("o/r", sha)`, the first 40 hex digits of SHA-256 of `o/r:<sha>`, so two
// repositories served from the one local repository share no tree, as two real ones do not. A commit is asked for by
// its alias (the profile's mirror rewritten to name them). `--tips` names each branch's tip, `{"o/r/branch": alias}`,
// for `branches/{name}`.
//
// Point a scratch profile's Gitea source at it (`base_url` http://127.0.0.1:3911). `--control` takes
// `POST /delay?ms=<n>` and `POST /files?n=<n>`, and answers `GET /count` with the requests answered so far, by kind,
// and `GET /tree?sha=<sha>` whether that commit's tree has been answered.
// Nothing is written anywhere.
import http from "node:http";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";

const args = process.argv.slice(2);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const listen = Number(opt("--listen"));
let delay = Number(opt("--delay", "650"));
let filesPerCompare = Number(opt("--files", "300"));
const control = opt("--control");
const gitDir = opt("--git");
const distinct = args.includes("--distinct");
const tips = opt("--tips") ? JSON.parse(readFileSync(opt("--tips"), "utf8")) : {};
/** Each alias the stand-in answered or can be asked by, to its object in the local repository. */
const back = new Map();
const aliased = new Set();
const alias = (repo, object) => (distinct ? createHash("sha256").update(`${repo}:${object}`).digest("hex").slice(0, 40) : object);
/** The local object an alias of `repo` names: its commits are known before any is asked for. */
function real(repo, object) {
  if (!distinct) return object;
  if (!aliased.has(repo)) {
    aliased.add(repo);
    for (const commit of git("rev-list", "--all").toString().split("\n").filter(Boolean)) back.set(alias(repo, commit), commit);
  }
  return back.get(object) ?? object;
}
const counts = {};
/** The trees answered, by sha: a read behind the app's kept top has landed once its commit's tree has been answered. */
const treesAnswered = new Set();
if (!listen) { console.log("compare-forge: --listen <port> is required"); process.exit(2); }

const sha = (...parts) => createHash("sha1").update(parts.join("\u0000")).digest("hex");
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** `n` added files of three lines each, in twenty folders, as `git diff` prints them. */
function diff(head, n) {
  const out = [];
  for (let i = 0; i < n; i += 1) {
    const path = `src/part-${i % 20}/${head.replace(/[^a-z0-9]+/gi, "-")}-${i}.txt`;
    out.push(`diff --git a/${path} b/${path}`, "new file mode 100644", "index 0000000..1111111", "--- /dev/null", `+++ b/${path}`, "@@ -0,0 +1,3 @@", `+one of ${i}`, `+two of ${i}`, `+three of ${i}`);
  }
  return `${out.join("\n")}\n`;
}

/** Three commits on `head` over the merge base, newest first, as Gitea lists a compare's. */
function commits(base, head) {
  const merge = sha("base", base);
  const shas = [1, 2, 3].map((n) => sha("commit", base, head, n));
  const at = (n) => new Date(Date.UTC(2026, 10, 13, 9, n)).toISOString();
  const listed = [2, 1, 0].map((n) => ({
    sha: shas[n],
    html_url: null,
    created: at(n),
    commit: { message: `Work on ${head}, part ${n + 1}`, author: { name: "Stand-in", date: at(n) } },
    parents: [{ sha: n === 0 ? merge : shas[n - 1] }],
  }));
  return { total_commits: 3, commits: listed };
}

const git = (...argv) => execFileSync("git", ["-C", gitDir, ...argv], { maxBuffer: 64 << 20 });
const json = (res, value) => {
  res.writeHead(200, { "content-type": "application/json" });
  res.end(JSON.stringify(value));
};
const missing = (res) => {
  res.writeHead(404, { "content-type": "application/json" });
  res.end('{"message":"not on the stand-in"}');
};

/** `git ls-tree -l` rows as Gitea's tree entries: a level's by name, a recursive one's by whole path. */
function treeEntries(sha, recursive) {
  const out = git("ls-tree", "-z", "-l", ...(recursive ? ["-r", "-t"] : []), sha).toString("utf8");
  return out.split("\0").filter(Boolean).map((row) => {
    const [meta, path] = row.split("\t");
    const [mode, type, object, size] = meta.split(/\s+/);
    return { path, mode, type, sha: object, size: size === "-" ? 0 : Number(size) };
  });
}

/** What *Files* reads, from the one local repository; `null` for a route it does not answer. */
async function files(url, res) {
  const repo = url.pathname.match(/^\/api\/v1\/repos\/([^/]+)\/([^/]+)(\/.*)?$/);
  if (repo === null) return false;
  const [, owner, name, rest = ""] = repo;
  const ref = url.searchParams.get("ref");
  let m;
  if (rest === "") {
    count("repo");
    await sleep(delay);
    json(res, { full_name: `${owner}/${name}`, html_url: null, default_branch: "main", empty: false });
  } else if ((m = rest.match(/^\/branches\/(.+)$/))) {
    const branch = decodeURIComponent(m[1]);
    count("branch");
    await sleep(delay);
    const tip = tips[`${owner}/${name}/${branch}`];
    const sha = tip ?? git("rev-parse", "--verify", "-q", `refs/heads/${branch}`).toString().trim();
    json(res, { name: branch, commit: { id: sha, message: git("log", "-1", "--format=%s", real(`${owner}/${name}`, sha)).toString().trim() } });
  } else if ((m = rest.match(/^\/git\/trees\/([0-9a-f]+)$/))) {
    const recursive = url.searchParams.get("recursive") === "true";
    count(recursive ? "tree" : "level");
    await sleep(delay);
    const repoName = `${owner}/${name}`;
    const all = treeEntries(real(repoName, m[1]), recursive).map((entry) => {
      if (!distinct) return entry;
      const named = alias(repoName, entry.sha);
      back.set(named, entry.sha);
      return { ...entry, sha: named };
    });
    const page = Number(url.searchParams.get("page") ?? "1");
    const per = Number(url.searchParams.get("per_page") ?? "1000");
    json(res, { sha: m[1], tree: all.slice((page - 1) * per, page * per), total_count: all.length, truncated: false });
    treesAnswered.add(m[1]);
  } else if ((m = rest.match(/^\/contents-ext\/(.+)$/)) && ref !== null) {
    const path = decodeURIComponent(m[1]);
    count("entry");
    await sleep(delay);
    const [row] = treeEntries(`${real(`${owner}/${name}`, ref)}:${path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : ""}`, false).filter((e) => e.path === path.slice(path.lastIndexOf("/") + 1));
    if (row === undefined) return missing(res), true;
    json(res, { file_contents: { name: row.path, type: "file", size: row.size, sha: alias(`${owner}/${name}`, row.sha), html_url: null } });
  } else if ((m = rest.match(/^\/raw\/(.+)$/)) && ref !== null) {
    const path = decodeURIComponent(m[1]);
    count("raw");
    await sleep(delay);
    const bytes = git("cat-file", "blob", `${real(`${owner}/${name}`, ref)}:${path}`);
    res.writeHead(200, { "content-type": "application/octet-stream", "content-length": bytes.length });
    res.end(bytes);
  } else return false;
  return true;
}

const count = (kind) => (counts[kind] = (counts[kind] ?? 0) + 1);

http.createServer(async (req, res) => {
  const url = new URL(req.url, "http://x");
  const compare = url.pathname.match(/^\/api\/v1\/repos\/[^/]+\/[^/]+\/compare\/(.+)$/);
  if (req.method === "GET" && compare === null && gitDir !== null) {
    try {
      if (await files(url, res)) return;
    } catch {
      return missing(res);
    }
  }
  if (req.method !== "GET" || compare === null) return missing(res);
  const ends = decodeURIComponent(compare[1]);
  const cut = ends.indexOf("...");
  const [base, head] = cut < 0 ? [ends, ends] : [ends.slice(0, cut), ends.slice(cut + 3)];
  count("compare");
  await sleep(delay);
  if (url.searchParams.get("output") === "diff") {
    res.writeHead(200, { "content-type": "text/plain" });
    res.end(diff(head, filesPerCompare));
  } else {
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify(commits(base, head)));
  }
}).listen(listen, "127.0.0.1", () => console.log(`standin-forge: 127.0.0.1:${listen}, ${delay} ms a request, ${filesPerCompare} files a compare${gitDir ? `, files from ${gitDir}` : ""}`));

if (control) {
  http.createServer((req, res) => {
    const url = new URL(req.url, "http://x");
    if (req.method === "POST" && url.pathname === "/delay") { delay = Number(url.searchParams.get("ms") ?? "0"); res.end(`${delay}\n`); return; }
    if (req.method === "POST" && url.pathname === "/files") { filesPerCompare = Number(url.searchParams.get("n") ?? "300"); res.end(`${filesPerCompare}\n`); return; }
    if (url.pathname === "/tree") { res.end(treesAnswered.has(url.searchParams.get("sha") ?? "") ? "yes\n" : "no\n"); return; }
    if (url.pathname === "/count") { res.end(`${JSON.stringify(counts)}\n`); return; }
    res.statusCode = 404;
    res.end();
  }).listen(Number(control), "127.0.0.1");
}
