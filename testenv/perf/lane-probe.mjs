// The felt waits a sync can hold, timed at the windowless backend (the priority lane, 2026-11-13): each probe is the
// live reads one page or press makes, sent together as the page sends them, and timed from the first send to the
// first answer that holds something (useful) and to the last live answer (settled). A view knobas answers from what
// it held (ADR-0042, `held_at` before the send) is read again behind its answer; the call is polled until its
// `held_at` moves past the send, which is when the page's live read landed. With `--source`, the probes run while
// that source is kept syncing (`syncing.mjs`), which is the case a person's read used to queue behind.
//
//   node lane-probe.mjs --port <backend port> --probes <file.json> [--source <source id> [--mode sync|backfill]]
//                       [--out <file.json>]
//
// A probes file is `[{"name": "...", "calls": [{"cmd": "entity_view", "args": {...}}, ...]}, ...]`. A call with
// `"until": "sent"` is a press (`submit_write`): its live answer is when the write left the queue, landed or refused
// (`pending_writes` no longer lists it open), which is when the page's toast says so. A probe is a
// first open only on a backend that has not read it before: start each measured backend on a fresh clone of the
// database, so its kept reads are its own.
//
// **It leaves the live products as it found them.** Each comment a press posts (`{"Comment": ...}`) carries a marker
// of this run, `[lane-probe <run>]`, and once the probes are done every comment carrying it is deleted through the
// product's own API: a Gitea pull request's or issue's (`live-gitea:`, KNOBAS_GITEA_URL and KNOBAS_GITEA_TOKEN) and
// a Jira ticket's (`live-jira:`, KNOBAS_JIRA_URL, KNOBAS_JIRA_USER and KNOBAS_JIRA_PASSWORD), the variables
// `eval "$(testenv/seed --env)"` sets. A press still in the queue is discarded first. A comment it cannot delete (no
// variables, another source, a refused delete) is named, and the run exits 1.
import { spawn } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const argv = process.argv.slice(2);
const flag = (name) => {
  const at = argv.indexOf(`--${name}`);
  return at >= 0 ? argv[at + 1] : undefined;
};
const port = Number(flag("port"));
const probes = JSON.parse(readFileSync(flag("probes") ?? "", "utf8"));
const source = flag("source");
const out = flag("out");
const mode = flag("mode") ?? "sync";
if (!port || !Array.isArray(probes)) {
  console.error("usage: node lane-probe.mjs --port <port> --probes <file.json> [--source <id>] [--out <file>]");
  process.exit(2);
}

const ask = (cmd, args) =>
  fetch(`http://127.0.0.1:${port}/invoke`, { method: "POST", body: JSON.stringify({ cmd, args }) }).then((r) =>
    r.json(),
  );

/** When `answer` says it was read, if it was answered from what knobas held. */
const heldAt = (answer) => (typeof answer?.ok?.held_at === "string" ? Date.parse(answer.ok.held_at) : null);

/** This run's mark on every comment it posts, and what it posted them on. */
const marker = `[lane-probe ${Date.now().toString(36)}]`;
const commented = new Set();
const pressed = [];

/** A press's args with this run's marker on its comment, noting where the comment goes. */
function marked(args) {
  const comment = args?.payload?.Comment;
  if (comment === undefined) return args;
  commented.add(comment.entity);
  return { ...args, payload: { ...args.payload, Comment: { ...comment, body: `${comment.body} ${marker}` } } };
}

/** Whether the write `id` is still open in the queue. */
const open = async (id) => ((await ask("pending_writes", {})).ok ?? []).some((w) => w.id === id);

async function call({ cmd, args: asked, until }) {
  const args = cmd === "submit_write" ? marked(asked) : asked;
  const sent = performance.now();
  const sentWall = Date.now();
  let ok = false;
  let holds = false;
  let held = false;
  let liveMs = null;
  let error;
  try {
    const answer = await ask(cmd, args);
    ok = "ok" in answer;
    holds = ok && answer.ok !== null && JSON.stringify(answer.ok).length > 2;
    if (!ok) error = JSON.stringify(answer.err).slice(0, 200);
    if (ok && cmd === "submit_write") pressed.push(answer.ok.id);
    const at = heldAt(answer);
    held = at !== null && at < sentWall;
    const firstMs = performance.now() - sent;
    if (!held && until !== "sent") liveMs = firstMs;
    // A press: the write is queued at once and sent behind the answer.
    while (until === "sent" && ok && liveMs === null && performance.now() - sent < 120_000) {
      if (!(await open(answer.ok.id))) liveMs = performance.now() - sent;
      else await new Promise((resolve) => setTimeout(resolve, 100));
    }
    // The read behind: polled until the view knobas holds was read after the send.
    while (held && liveMs === null && performance.now() - sent < 120_000) {
      await new Promise((resolve) => setTimeout(resolve, 100));
      const again = await ask(cmd, args);
      const now = heldAt(again);
      if (now !== null && now >= sentWall) liveMs = performance.now() - sent;
    }
    return { cmd, args, ms: firstMs, liveMs, ok, holds, held, error };
  } catch (e) {
    error = String(e);
  }
  return { cmd, args, ms: performance.now() - sent, liveMs, ok, holds, held, error };
}

let load;
if (source) {
  const here = dirname(fileURLToPath(import.meta.url));
  load = spawn("node", [join(here, "syncing.mjs"), "--port", String(port), "--source", source, "--mode", mode], {
    stdio: ["ignore", "pipe", "inherit"],
  });
  await new Promise((resolve, reject) => {
    load.stdout.on("data", (chunk) => {
      process.stderr.write(chunk);
      if (/^syncing$/m.test(String(chunk))) resolve();
    });
    load.on("exit", () => reject(new Error("syncing.mjs ended before the source was syncing")));
  });
}

const results = [];
try {
  for (const probe of probes) {
    const calls = await Promise.all(probe.calls.map(call));
    const answered = calls.filter((c) => c.holds).map((c) => c.ms);
    const result = {
      name: probe.name,
      usefulMs: answered.length ? Math.min(...answered) : null,
      settledMs: calls.some((c) => c.ok && c.liveMs === null) ? null : Math.max(...calls.map((c) => c.liveMs ?? c.ms)),
      calls,
    };
    results.push(result);
    const failed = calls.filter((c) => !c.ok).map((c) => `${c.cmd}: ${c.error}`);
    console.log(
      `${probe.name.padEnd(40)} useful ${result.usefulMs === null ? "   -" : result.usefulMs.toFixed(0).padStart(5)} ms` +
        `  settled ${result.settledMs === null ? "never" : result.settledMs.toFixed(0).padStart(5)} ms` +
        `${failed.length ? `  FAILED ${failed.join("; ")}` : ""}`,
    );
  }
} finally {
  load?.kill("SIGTERM");
}
if (out) writeFileSync(out, JSON.stringify({ port, source: source ?? null, mode, results }, null, 1));

/** Delete every comment carrying this run's marker; answers what could not be deleted. */
async function cleanUp() {
  for (const id of pressed) if (await open(id)) await ask("discard_write", { id });
  const env = process.env;
  const left = [];
  let removed = 0;
  const forge = async (url, init) => {
    const response = await fetch(url, init);
    if (!response.ok) throw new Error(`${init?.method ?? "GET"} ${url}: ${response.status}`);
    return init?.method === "DELETE" ? null : response.json();
  };
  for (const entity of commented) {
    try {
      const gitea = /^live-gitea:([^/]+)\/([^#@]+)#(\d+)$/.exec(entity);
      const jira = /^live-jira:([A-Z][A-Z0-9_]*-\d+)$/.exec(entity);
      if (gitea && env.KNOBAS_GITEA_URL && env.KNOBAS_GITEA_TOKEN) {
        const api = `${env.KNOBAS_GITEA_URL.replace(/\/$/, "")}/api/v1/repos/${gitea[1]}/${gitea[2]}/issues`;
        const headers = { Authorization: `token ${env.KNOBAS_GITEA_TOKEN}` };
        for (const c of await forge(`${api}/${gitea[3]}/comments`, { headers })) {
          if (!c.body.includes(marker)) continue;
          await forge(`${api}/comments/${c.id}`, { method: "DELETE", headers });
          removed += 1;
        }
      } else if (jira && env.KNOBAS_JIRA_URL && env.KNOBAS_JIRA_USER && env.KNOBAS_JIRA_PASSWORD) {
        const api = `${env.KNOBAS_JIRA_URL.replace(/\/$/, "")}/rest/api/2/issue/${jira[1]}/comment`;
        const basic = Buffer.from(`${env.KNOBAS_JIRA_USER}:${env.KNOBAS_JIRA_PASSWORD}`).toString("base64");
        const headers = { Authorization: `Basic ${basic}`, Accept: "application/json" };
        for (const c of (await forge(`${api}?maxResults=1000`, { headers })).comments) {
          if (!String(c.body).includes(marker)) continue;
          await forge(`${api}/${c.id}`, { method: "DELETE", headers });
          removed += 1;
        }
      } else {
        left.push(`${entity} (no way to delete it here)`);
        continue;
      }
    } catch (error) {
      left.push(`${entity} (${error.message})`);
    }
  }
  console.log(`lane-probe: deleted ${removed} comment(s) marked ${marker}`);
  return left;
}

if (commented.size > 0) {
  const left = await cleanUp();
  if (left.length > 0) {
    console.error(`lane-probe: comments marked ${marker} may be left on: ${left.join(", ")}`);
    process.exit(1);
  }
}
