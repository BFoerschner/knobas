// A Gitea-shaped stand-in for the forge-waits pass (2026-11-13), for the Mac while the live products are someone
// else's. It answers, after the delay asked:
//
// - Gitea's compare route, the diff (`?output=diff`) and the commits (`?files=false`), for any two refs: `--files`
//   added files of three lines (the live Gitea took 0.65 to 2.7 s for a first compare without a clone);
// - with `--git <dir>`, a repository's record, a branch's tip, its tree (`git/trees`, a level or recursive), a file's
//   own entry (`contents-ext`) and its bytes (`raw`), for every repository, from that one local repository: what
//   *Files* reads, so a file read from the forge and one read from a clone can be timed against each other;
//
// - with `--repos <dir>`, each repository `o/r` from its own `<dir>/o-r.git` where there is one (the `--git` one
//   otherwise), and what a repository's other inner pages read (the inner-6x pass, 2026-11-15): its branches, a
//   commit's record and its diff, a path's commits, a commit's statuses, and with `--pulls <file.json>` (records by
//   `o/r#n`, as Gitea answers `pulls/{n}`, e.g. the mirror's payloads) a pull request's record, commits and `.diff`
//   (`git diff <merge base>...<head>`), with no reviews or comments; the account is `knobas`;
//
// and every other route 404 at once (`--log` prints every request, its status and its time, and why a route failed).
//
//   node testenv/perf/standin-forge.mjs --listen 3911 [--delay 650] [--files 300] [--git <dir>] [--control 3912]
//                                       [--distinct] [--tips <file.json>] [--real-compare] [--repos <dir>]
//                                       [--pulls <file.json>] [--log]
//
// `--real-compare` (the stored-compare pass, 2026-11-14) answers the compare from the `--git` repository itself, as
// Gitea does: the diff is `git diff -M <base>...<head>` and the commits `git log <base>..<head>` with their parents, so
// a list knobas draws first can be checked against the one the forge settles. `--refuse-rest` answers every other route
// 503, so a profile whose Gitea source stays on (the mirror's rows of a source turned off are not read) syncs nothing:
// each run fails as the forge not answering, and sweeps nothing.
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
import { execFile, execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { AsyncLocalStorage } from "node:async_hooks";
import { createHash } from "node:crypto";

const args = process.argv.slice(2);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const listen = Number(opt("--listen"));
let delay = Number(opt("--delay", "650"));
let filesPerCompare = Number(opt("--files", "300"));
const control = opt("--control");
const gitDir = opt("--git");
const distinct = args.includes("--distinct");
const realCompare = args.includes("--real-compare");
const refuseRest = args.includes("--refuse-rest");
const tips = opt("--tips") ? JSON.parse(readFileSync(opt("--tips"), "utf8")) : {};
const reposDir = opt("--repos");
const pulls = opt("--pulls") ? JSON.parse(readFileSync(opt("--pulls"), "utf8")) : {};
const logMissing = args.includes("--log");
/** The local repository a request reads: its own under `--repos`, else `--git`'s. */
const repoDir = new AsyncLocalStorage();
const dirOf = (pathname) => {
  const m = pathname.match(/^\/api\/v1\/repos\/([^/]+)\/([^/]+)/);
  const own = m && reposDir ? `${reposDir}/${decodeURIComponent(m[1])}-${decodeURIComponent(m[2])}.git` : null;
  return own && existsSync(own) ? own : gitDir;
};
/** Each alias the stand-in answered or can be asked by, to its object in the local repository. */
const back = new Map();
const aliased = new Set();
const alias = (repo, object) => (distinct ? createHash("sha256").update(`${repo}:${object}`).digest("hex").slice(0, 40) : object);
/** The local object an alias of `repo` names: its commits are known before any is asked for. */
function real(repo, object) {
  if (!distinct) return object;
  if (!aliased.has(repo)) {
    aliased.add(repo);
    for (const commit of gitSync("rev-list", "--all").toString().split("\n").filter(Boolean)) back.set(alias(repo, commit), commit);
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

/** The commits `head` has that `base` lacks, newest first, with their parents, as Gitea lists a compare's. */
async function realCommits(base, head) {
  const listed = (await git("log", "--format=%H%x1f%P%x1f%an%x1f%aI%x1f%s%x1e", `${base}..${head}`, "--"))
    .toString("utf8")
    .split("\x1e")
    .map((r) => r.trim())
    .filter(Boolean)
    .map((record) => {
      const [sha, parents, name, date, subject] = record.split("\x1f");
      return { sha, html_url: null, created: date, commit: { message: subject, author: { name, date } }, parents: parents.split(" ").filter(Boolean).map((p) => ({ sha: p })) };
    });
  return { total_commits: listed.length, commits: listed };
}

// Off the event loop: one slow read (a branch list, a big diff) holds no other request (the inner-6x pass, 2026-11-15:
// run in line, a page's requests waited seconds behind each other here, not on knobas).
const git = (...argv) =>
  new Promise((resolve, reject) =>
    execFile("git", ["-C", repoDir.getStore() ?? gitDir, ...argv], { maxBuffer: 64 << 20, encoding: "buffer" }, (error, out) => (error ? reject(error) : resolve(out))),
  );
const gitSync = (...argv) => execFileSync("git", ["-C", repoDir.getStore() ?? gitDir, ...argv], { maxBuffer: 64 << 20 });
const json = (res, value, total = null) => {
  res.writeHead(200, { "content-type": "application/json", ...(total === null ? {} : { "x-total-count": String(total) }) });
  res.end(JSON.stringify(value));
};
const missing = (res, url = null) => {
  if (logMissing && url) console.log(`missing ${url.pathname}${url.search}`);
  // A read that failed after its answer began is cut off, not answered again: a second answer threw, and the stand-in
  // went down mid-run, so every page after it read nothing (the forge-rounds pass, 2026-11-15).
  if (res.headersSent) return void res.destroy();
  res.writeHead(refuseRest ? 503 : 404, { "content-type": "application/json" });
  res.end('{"message":"not on the stand-in"}');
};

/** `git ls-tree -l` rows as Gitea's tree entries: a level's by name, a recursive one's by whole path. */
async function treeEntries(sha, recursive) {
  const out = (await git("ls-tree", "-z", "-l", ...(recursive ? ["-r", "-t"] : []), sha)).toString("utf8");
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
    const sha = tip ?? (await git("rev-parse", "--verify", "-q", `refs/heads/${branch}`)).toString().trim();
    json(res, { name: branch, commit: { id: sha, message: (await git("log", "-1", "--format=%s", real(`${owner}/${name}`, sha))).toString().trim() } });
  } else if ((m = rest.match(/^\/git\/trees\/([0-9a-f]+)$/))) {
    const recursive = url.searchParams.get("recursive") === "true";
    count(recursive ? "tree" : "level");
    await sleep(delay);
    const repoName = `${owner}/${name}`;
    const all = (await treeEntries(real(repoName, m[1]), recursive)).map((entry) => {
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
    const [row] = (await treeEntries(`${real(`${owner}/${name}`, ref)}:${path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : ""}`, false)).filter((e) => e.path === path.slice(path.lastIndexOf("/") + 1));
    if (row === undefined) return missing(res), true;
    json(res, { file_contents: { name: row.path, type: "file", size: row.size, sha: alias(`${owner}/${name}`, row.sha), html_url: null } });
  } else if ((m = rest.match(/^\/raw\/(.+)$/)) && ref !== null) {
    const path = decodeURIComponent(m[1]);
    count("raw");
    await sleep(delay);
    const bytes = await git("cat-file", "blob", `${real(`${owner}/${name}`, ref)}:${path}`);
    res.writeHead(200, { "content-type": "application/octet-stream", "content-length": bytes.length });
    res.end(bytes);
  } else return inner(url, res, owner, name, rest);
  return true;
}

/** Commits as Gitea's `git/commits/{sha}` and its commit lists answer them, read in one `git show`, kept by repository and sha. */
const records = new Map();
async function commitRecords(shas) {
  const dir = repoDir.getStore() ?? gitDir;
  const want = [...new Set(shas)].filter((sha) => !records.has(`${dir}\n${sha}`));
  if (want.length > 0) {
    const out = (await git("show", "-s", "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%aI%x1f%cn%x1f%ce%x1f%cI%x1f%T%x1f%B%x1e", ...want)).toString("utf8");
    const rows = out.split("\x1e").map((r) => r.replace(/^\n+/, "")).filter(Boolean);
    rows.forEach((row, n) => {
      const [full, parents, an, ae, ad, cn, ce, cd, tree, ...message] = row.split("\x1f");
      const record = {
        sha: full,
        html_url: null,
        created: ad,
        commit: { message: message.join("\x1f").replace(/\n+$/, ""), author: { name: an, email: ae, date: ad }, committer: { name: cn, email: ce, date: cd }, tree: { sha: tree } },
        author: null,
        committer: null,
        parents: parents.split(" ").filter(Boolean).map((p) => ({ sha: p })),
        files: null,
      };
      records.set(`${dir}\n${want[n]}`, record);
    });
  }
  return shas.map((sha) => records.get(`${dir}\n${sha}`));
}
const commitRecord = async (sha) => (await commitRecords([sha]))[0];
const commitList = async (range, path = null) => (await git("rev-list", range, ...(path ? ["--", path] : []))).toString().split("\n").filter(Boolean);
/** A page of `shas` as Gitea pages a list (`page`, `limit`), with its total. */
async function paged(url, res, shas) {
  const page = Number(url.searchParams.get("page") ?? "1");
  const limit = Number(url.searchParams.get("limit") ?? url.searchParams.get("per_page") ?? "30");
  json(res, await commitRecords(shas.slice((page - 1) * limit, page * limit)), shas.length);
}
/** A commit's change as Gitea's `.diff` prints it: against its first parent, or whole for a root. */
async function commitDiff(sha) {
  const parents = (await git("show", "-s", "--format=%P", sha)).toString().trim().split(" ").filter(Boolean);
  return parents.length ? git("diff", "--no-color", "--no-ext-diff", "-M", `${parents[0]}`, sha, "--") : git("show", "--no-color", "--format=", sha);
}

/** What a repository's other inner pages read (`--repos`, `--pulls`); `false` for a route the stand-in does not answer. */
async function inner(url, res, owner, name, rest) {
  let m;
  const pull = (n) => pulls[`${owner}/${name}#${n}`];
  if (rest === "/branches") {
    count("branches");
    await sleep(delay);
    const rows = (await git("for-each-ref", "--format=%(refname:short)%1f%(objectname)", "refs/heads")).toString().split("\n").filter(Boolean).map((r) => r.split("\x1f"));
    const page = Number(url.searchParams.get("page") ?? "1");
    const limit = Number(url.searchParams.get("limit") ?? "50");
    const shown = rows.slice((page - 1) * limit, page * limit);
    const commits = await commitRecords(shown.map(([, sha]) => sha));
    // A branch's commit as Gitea's branch record gives it: `id`, not a commit record's `sha`.
    const branchCommit = (c) => ({ id: c.sha, message: c.commit.message, url: null, author: c.commit.author, committer: c.commit.committer, timestamp: c.commit.committer.date });
    json(res, shown.map(([branch], n) => ({ name: branch, commit: branchCommit(commits[n]), protected: false })), rows.length);
  } else if ((m = rest.match(/^\/git\/commits\/([0-9a-f]{40})\.diff$/))) {
    count("commit-diff");
    await sleep(delay);
    const text = await commitDiff(m[1]);
    res.writeHead(200, { "content-type": "text/plain" });
    res.end(text);
  } else if ((m = rest.match(/^\/git\/commits\/([0-9a-f]{40})$/))) {
    count("commit");
    await sleep(delay);
    json(res, await commitRecord(m[1]));
  } else if ((m = rest.match(/^\/commits\/([^/]+)\/status$/))) {
    count("status");
    await sleep(delay);
    json(res, { state: "", statuses: [], total_count: 0, sha: m[1] });
  } else if (rest === "/commits") {
    count("commits");
    await sleep(delay);
    await paged(url, res, await commitList(url.searchParams.get("sha") ?? "main", url.searchParams.get("path")));
  } else if ((m = rest.match(/^\/pulls\/(\d+)\.diff$/)) && pull(m[1])) {
    count("pull-diff");
    await sleep(delay);
    const p = pull(m[1]);
    const text = await git("diff", "--no-color", "--no-ext-diff", "-M", `${p.merge_base ?? p.base.sha}...${p.head.sha}`, "--");
    res.writeHead(200, { "content-type": "text/plain" });
    res.end(text);
  } else if ((m = rest.match(/^\/pulls\/(\d+)$/)) && pull(m[1])) {
    count("pull");
    await sleep(delay);
    json(res, pull(m[1]));
  } else if ((m = rest.match(/^\/pulls\/(\d+)\/commits$/)) && pull(m[1])) {
    count("pull-commits");
    await sleep(delay);
    const p = pull(m[1]);
    await paged(url, res, await commitList(`${p.merge_base ?? p.base.sha}..${p.head.sha}`));
  } else if ((m = rest.match(/^\/(pulls\/\d+\/(reviews|files)|issues\/\d+\/(comments|timeline)|reviewers|labels|pulls)$/))) {
    count("empty");
    await sleep(delay);
    json(res, [], 0);
  } else return false;
  return true;
}

const count = (kind) => (counts[kind] = (counts[kind] ?? 0) + 1);

http.createServer((req, res) => {
  // `--log`: every request, its status and its time here, so a slow page can be told from a slow stand-in.
  if (logMissing) {
    const t = performance.now();
    res.on("finish", () => console.log(`${req.method} ${req.url} ${res.statusCode} ${Math.round(performance.now() - t)} ms`));
  }
  repoDir.run(dirOf(new URL(req.url, "http://x").pathname), () => answer(req, res));
}).listen(listen, "127.0.0.1", () => console.log(`standin-forge: 127.0.0.1:${listen}, ${delay} ms a request, ${filesPerCompare} files a compare${gitDir ? `, files from ${gitDir}` : ""}${reposDir ? `, repositories from ${reposDir}` : ""}`));

async function answer(req, res) {
  const url = new URL(req.url, "http://x");
  if (req.method === "GET" && url.pathname === "/api/v1/user") return json(res, { id: 1, login: "knobas", username: "knobas", full_name: "knobas", email: "knobas@tidewater.test" });
  const compare = url.pathname.match(/^\/api\/v1\/repos\/[^/]+\/[^/]+\/compare\/(.+)$/);
  if (req.method === "GET" && compare === null && (gitDir !== null || reposDir !== null)) {
    try {
      if (await files(url, res)) return;
    } catch (e) {
      if (logMissing) console.log(`failed ${url.pathname}${url.search}: ${String(e.message ?? e).split("\n")[0]}`);
      return missing(res);
    }
  }
  if (req.method !== "GET" || compare === null) return missing(res, url);
  const ends = decodeURIComponent(compare[1]);
  const cut = ends.indexOf("...");
  const [base, head] = cut < 0 ? [ends, ends] : [ends.slice(0, cut), ends.slice(cut + 3)];
  count("compare");
  await sleep(delay);
  if (realCompare && (gitDir !== null || reposDir !== null)) {
    try {
      if (url.searchParams.get("output") === "diff") {
        // Read before answering: a git that fails is a 404, not a second answer.
        const text = await git("diff", "--no-color", "--no-ext-diff", "-M", `${base}...${head}`, "--");
        res.writeHead(200, { "content-type": "text/plain" });
        res.end(text);
      } else json(res, await realCommits(base, head));
    } catch {
      missing(res);
    }
    return;
  }
  if (url.searchParams.get("output") === "diff") {
    res.writeHead(200, { "content-type": "text/plain" });
    res.end(diff(head, filesPerCompare));
  } else {
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify(commits(base, head)));
  }
}

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
