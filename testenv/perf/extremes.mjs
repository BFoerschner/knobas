// The extremes for `open.mjs --set extremes` (the extremes pass of PR #2479, 2026-11-13; README, *The extremes*):
// the worst shapes a real team's data has, on the live products, each under a scope of its own
// (`testenv/seed-extremes.sh`: Jira project XTRM, Confluence space XTRM, Gitea org knobas-extremes, TeamCity project
// Extremes), read through the profile's own sources on the windowless backend.
//
// `up` seeds what is missing (each product under its live lock; a rerun on a seeded product adds nothing), widens the
// sources' scopes to the extremes and syncs them; `down` puts the scopes back and, unless `--keep-extremes`, removes
// the extremes from the products again, so they are left as they were.
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const here = new URL(".", import.meta.url).pathname;
const testenv = join(here, "..");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Each product's extremes: the seed's name for it, the source that reads it, and the scope it widens. */
export const PRODUCTS = [
  { product: "jira", source: "live-jira", key: "projects", value: "XTRM" },
  { product: "confluence", source: "live-confluence", key: "spaces", value: "XTRM" },
  { product: "gitea", source: "live-gitea", key: "owners", value: "knobas-extremes" },
  { product: "teamcity", source: "live-teamcity", key: null, value: null },
];

async function call(port, cmd, args = {}) {
  const res = await fetch(`http://127.0.0.1:${port}/invoke`, { method: "POST", body: JSON.stringify({ cmd, args }) });
  const body = await res.json();
  if ("err" in body) throw new Error(`${cmd}: ${JSON.stringify(body.err)}`);
  return body.ok;
}

let made = null;

/** This worktree's seed state: the products' addresses and credentials (`./seed` writes it). */
const state = () => JSON.parse(readFileSync(process.env.KNOBAS_EXTREMES_STATE ?? join(testenv, "seed-state.json"), "utf8"));

/**
 * The items the places open: the ids the seed fixes (`extremes-items.json`), and those the products give, read from
 * them: the Confluence pages' ids by title, the TeamCity run's as the configuration's newest finished build.
 */
export async function items() {
  const spec = JSON.parse(readFileSync(join(here, "extremes-items.json"), "utf8"));
  // KNOBAS_EXTREMES_IDS (JSON) gives the ids outright: for a run on a mirror synced earlier, with a product down since.
  if (process.env.KNOBAS_EXTREMES_IDS) return { ...spec.fixed, ...JSON.parse(process.env.KNOBAS_EXTREMES_IDS) };
  const out = { ...spec.fixed };
  const s = state();
  const basic = (u, p) => `Basic ${Buffer.from(`${u}:${p}`).toString("base64")}`;
  const read = async (url, headers) => {
    try {
      const res = await fetch(url, { headers: { ...headers, Accept: "application/json" }, signal: AbortSignal.timeout(20000) });
      return res.ok ? await res.json() : null;
    } catch {
      return null;
    }
  };
  for (const [name, title] of Object.entries(spec.pages)) {
    const found = s.confluence ? await read(`${s.confluence.url}/rest/api/content?spaceKey=XTRM&title=${encodeURIComponent(title)}`, { Authorization: basic(s.confluence.user, s.confluence.password) }) : null;
    const id = found?.results?.[0]?.id;
    if (id) out[name] = `live-confluence:${id}`;
  }
  const builds = s.teamcity ? await read(`${s.teamcity.url}/app/rest/builds?locator=buildType:(id:${spec.config}),state:finished,count:1&fields=build(id)`, { Authorization: `Bearer ${s.teamcity.token}` }) : null;
  const run = builds?.build?.[0]?.id;
  if (run) out.run = `live-teamcity:build:${run}`;
  for (const name of ["page", "hugePage", "run"]) if (!out[name]) console.log(`extremes: no ${name} found (its product down?): its places are left out`);
  return out;
}

/**
 * Seeds the extremes (unless `seeded`), widens each source's scope and syncs it, until the mirror holds `wait` (the
 * entity ids the places open). Each step a killed run leaves half done is redone by the next: the seed is idempotent.
 */
export async function up({ port, seed = true, keep = false, wait = [] }) {
  // A second `up` (the sync, after the seed) adds to what the first made, so `down` undoes both.
  made ??= { port, keep, seeded: [], scopes: [] };
  if (seed)
    for (const { product } of PRODUCTS) {
      console.log(`extremes: seeding ${product}`);
      execFileSync("bash", [join(testenv, "seed-extremes.sh"), product], { stdio: "inherit" });
      made.seeded.push(product);
    }
  const sources = await call(port, "list_sources");
  for (const { source, key, value } of PRODUCTS) {
    const live = sources.find((s) => s.id === source);
    if (!live) throw new Error(`extremes: the profile has no source ${source}`);
    if (key !== null && !live.config?.[key]?.includes(value)) {
      made.scopes.push({ id: source, config: live.config });
      await call(port, "update_source", { id: source, patch: { config: { ...live.config, [key]: [...live.config[key], value] } } });
    }
    await call(port, "sync_now", { sourceId: source }).catch(() => {});
  }
  for (let i = 0; i < 1800; i++) {
    const briefs = await call(port, "entity_briefs", { entityIds: wait }).catch(() => []);
    const running = (await call(port, "sync_status")).filter((s) => PRODUCTS.some((p) => p.source === s.source_id) && s.running);
    if (briefs.length === wait.length && running.length === 0) return;
    if (i % 60 === 59) {
      console.log(`extremes: waiting for the sync (${briefs.length}/${wait.length} items, ${running.length} running)`);
      for (const { source } of PRODUCTS) await call(port, "sync_now", { sourceId: source }).catch(() => {});
    }
    await sleep(1000);
  }
  throw new Error("extremes: the sources never synced the extremes");
}

/** Puts the scopes back and, unless kept, removes what `up` seeded. */
export async function down() {
  if (made === null) return;
  const { port, keep, seeded, scopes } = made;
  made = null;
  for (const { id, config } of scopes) await call(port, "update_source", { id, patch: { config } }).catch((e) => console.log(`extremes: ${e.message}`));
  if (keep) return console.log("extremes: kept on the products (--keep-extremes); testenv/seed-extremes.sh <product> --remove takes them away");
  for (const product of seeded) {
    try {
      execFileSync("bash", [join(testenv, "seed-extremes.sh"), product, "--remove"], { stdio: "inherit" });
    } catch (e) {
      console.log(`extremes: removing ${product} failed: ${e.message}`);
    }
  }
}

/**
 * The places at the items: each opens a page of the extremes the way a reader would, and is useful once its first
 * rows are drawn with nothing waiting. A return leaves by `away` (a press) or `awayGo` (an address).
 */
export function places(i, shown) {
  const enc = encodeURIComponent;
  const R = `#/repo/${enc(i.repo)}`;
  const PR = `#/pr/${enc(i.pr)}`;
  const RUN = `#/build/${enc(i.pipeline)}/${enc(i.run)}`;
  const TICKET = `#/ticket/${enc(i.ticket)}`;
  const tab = (base, t) => `${shown} .tf-tabs a[href="${base}/${t}"]`;
  const home = (base) => `${shown} .tf-tabs a[href="${base}"]`;
  const all = [
    { name: "x-ticket", start: "#/tickets", go: TICKET, hash: TICKET, useful: [[`${shown} .tf-tkhead, ${shown} .tf-ticket-title`, 1]], marks: { comment: [[`${shown} .tf-convo .tf-msg`, 1, true]] }, awayGo: "#/tickets" },
    { name: "x-pr-overview", start: `${R}/prs`, go: PR, hash: PR, useful: [[`${shown} .tf-pr .tf-md, ${shown} .tf-pr .tf-cfail`, 1]], marks: { side: [[`${shown} .tf-pr .tf-side .tf-panel`, 2]] }, awayGo: `${R}/prs` },
    { name: "x-pr-conversation", start: PR, press: tab(PR, "conversation"), hash: `${PR}/conversation`, useful: [[`${shown} .tf-convo article.tf-msg`, 3]], away: home(PR) },
    { name: "x-pr-files", start: PR, press: tab(PR, "diff"), hash: `${PR}/diff`, useful: [[`${shown} .tf-diff .tf-dbody`, 1], [`${shown} .tf-flist button`, 10]], away: home(PR) },
    { name: "x-pr-commits", start: PR, press: tab(PR, "commits"), hash: `${PR}/commits`, useful: [[`${shown} .tf-rcommits tr`, 10]], away: home(PR) },
    { name: "x-repo-branches", start: R, press: tab(R, "branches"), hash: `${R}/branches`, useful: [[`${shown} .tf-rbranches tbody tr`, 10]], away: tab(R, "files") },
    { name: "x-repo-prs", start: R, press: tab(R, "prs"), hash: `${R}/prs`, useful: [[`${shown} .tf-rprs tr.tf-row`, 10]], away: tab(R, "files") },
    { name: "x-repo-builds", start: R, press: tab(R, "builds"), hash: `${R}/builds`, useful: [[`${shown} .tf-rbrail .tf-rbr`, 10]], away: tab(R, "files") },
    { name: "x-repo-compare", start: R, press: tab(R, "compare"), hash: `${R}/compare`, useful: [[`${shown} .tf-cform`, 1]], away: tab(R, "files") },
    { name: "x-docs-page", start: `#/docs/space/XTRM`, go: `#/page/${enc(i.page)}`, hash: `#/page/`, useful: [[`${shown} .tf-rd-main h1`, 1], [`${shown} .tf-rd-main td`, 10]], marks: { comments: [[`${shown} .tf-cmts .tf-msg`, 1, true]] }, awayGo: `#/docs/space/XTRM` },
    { name: "x-docs-huge", start: `#/docs/space/XTRM`, go: `#/page/${enc(i.hugePage)}`, hash: `#/page/`, useful: [[`${shown} .tf-rd-main h1`, 1], [`${shown} .tf-rd-main p`, 1]], awayGo: `#/docs/space/XTRM` },
    { name: "x-docs-space", start: "#/docs", go: "#/docs/space/XTRM", hash: "#/docs/space/XTRM", useful: [[`${shown} .tf-dh-row`, 10]], awayGo: "#/docs" },
    { name: "x-run", start: `#/builds/${enc(i.pipeline)}`, go: RUN, hash: RUN, useful: [[`${shown} .tf-bhead, ${shown} .tf-runbody`, 2]], awayGo: `#/builds/${enc(i.pipeline)}` },
    { name: "x-run-artifacts", start: RUN, press: tab(RUN, "artifacts"), hash: `${RUN}/artifacts`, useful: [[`${shown} .tf-atree .tf-arow`, 10]], away: home(RUN) },
    { name: "x-run-tests", start: RUN, press: tab(RUN, "tests"), hash: `${RUN}/tests`, useful: [[`${shown} .tf-trow`, 3]], away: home(RUN) },
    { name: "x-run-log", start: RUN, press: tab(RUN, "log"), hash: `${RUN}/log`, useful: [[`${shown} .tf-logv .tf-ll`, 5]], away: home(RUN) },
  ];
  // A place whose item was not found (its product down, `items`) is left out.
  const needs = { "x-docs-page": i.page, "x-docs-huge": i.hugePage, "x-run": i.run, "x-run-artifacts": i.run, "x-run-tests": i.run, "x-run-log": i.run };
  return all.filter((p) => !(p.name in needs) || needs[p.name]).map((p) => ({ ...p, inner: true }));
}
