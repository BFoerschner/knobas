// Keeps one source syncing for the length of a run (the priority lane, 2026-11-13): asks the windowless backend for
// a run of the source, and for another each time the last one ends, until it is stopped (SIGTERM or SIGINT): the
// case a person's read used to queue behind. Every start is a line on stdout.
//
//   node syncing.mjs --port <backend port> --source <source id> [--mode sync|backfill] [--settle <ms>]
//
// --mode    `sync` (the default): the run the schedule starts every few minutes, from the stored position, or
//           `backfill`, which re-reads the whole source. On the scale Gitea either fills the quota, ten requests a
//           second (counted through `slow-forge.mjs`, 2026-11-13); Jira's, Confluence's and TeamCity's backfills
//           send about two a second, under their quotas.
// --settle  how long to wait after the first start before printing `syncing`, so a caller that waits for that line
//           measures with the run past its set-up and into its reads (default 4000)
const args = Object.fromEntries(
  process.argv
    .slice(2)
    .join(" ")
    .split(/\s*--/)
    .filter(Boolean)
    .map((pair) => pair.split(/\s+/)),
);
const port = Number(args.port);
const source = args.source;
const settle = Number(args.settle ?? 4000);
const mode = args.mode ?? "sync";
if (!port || !source) {
  console.error("usage: node syncing.mjs --port <port> --source <source id> [--settle <ms>]");
  process.exit(2);
}

async function invoke(cmd, payload) {
  const answer = await fetch(`http://127.0.0.1:${port}/invoke`, {
    method: "POST",
    body: JSON.stringify({ cmd, args: payload }),
  }).then((r) => r.json());
  if ("err" in answer) throw new Error(`${cmd}: ${JSON.stringify(answer.err)}`);
  return answer.ok;
}

async function running() {
  const all = await invoke("sync_status", {});
  return all.find((s) => s.source_id === source)?.running === true;
}

let stopped = false;
for (const signal of ["SIGTERM", "SIGINT"]) process.on(signal, () => (stopped = true));

const started = Date.now();
let runs = 0;
let announced = false;
while (!stopped) {
  try {
    if (!(await running())) {
      const run = await invoke(mode === "backfill" ? "backfill_source" : "sync_now", { sourceId: source });
      runs += 1;
      console.log(`syncing: ${mode} run ${run} of ${source} started at ${((Date.now() - started) / 1000).toFixed(1)} s`);
    }
  } catch (error) {
    console.log(`syncing: ${error.message}`);
  }
  if (!announced && runs > 0 && Date.now() - started >= settle) {
    console.log("syncing");
    announced = true;
  }
  await new Promise((resolve) => setTimeout(resolve, 500));
}
console.log(`syncing: stopped after ${runs} run(s)`);
