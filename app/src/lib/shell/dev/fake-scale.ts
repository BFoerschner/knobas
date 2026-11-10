/**
 * `?fake-ipc&fake-scale`: the fixture at the size of a real developer's mirror (Björn's, 2026-11-10: about 5,000
 * Confluence pages, 20,000 tickets, 4,000 TeamCity objects and 10,000 forge items), to measure every page at the size
 * it has to stay quick at. Deterministic: the same rows on every load, so two measurements read one corpus.
 *
 * - **Tickets**: 20,000 over eight projects (`SCL`'s keys are `PAY-1001`…, `OPS-1001`… after the demo's own), eleven
 *   statuses (so a board has eleven columns), 24 assignees and none, five types, 48 epics, labels, three years of
 *   updates, most of them settled the way a real tracker's are.
 * - **Docs**: 5,000 pages in three spaces, seven folds deep at most, where *Runbooks* holds one parent of 800
 *   children and *Engineering handbook* one of 300 ({@link scalePages}).
 * - **Forge**: 120 repositories, 2,400 branches, 4,400 pull requests (one in twelve open, some asking the fixture's
 *   reader for a review) and 3,000 commits.
 * - **CI**: 300 more configurations of twelve runs each on *Bulk CI* (`?fake-pipelines=300`), and the demo's own
 *   configurations reaching 3,000 runs back ({@link SCALE_HISTORY}).
 * - **Inbox** 2,000 more rows (`?fake-inbox-items`), **Monitors** 300 (`?fake-monitors`).
 *
 * An explicit `?fake-<area>=<n>` beside it still wins: {@link expandScale} only fills what the address leaves out.
 * Without the switch nothing here runs, so the default fixture, its screenshots and its trips are what they were.
 */
import { CORPUS } from "./fake/corpus";

/** What `fake-scale` turns on, as the per-area switches spell it. */
const AREAS: Record<string, string> = {
  "fake-pipelines": "300",
  "fake-inbox-items": "2000",
  "fake-monitors": "300",
};

/** How many runs back a demo configuration's history reaches under `fake-scale`. */
export const SCALE_HISTORY = 3000;

/** `params` with what `fake-scale` turns on, where the address does not say otherwise. */
export function expandScale(params: URLSearchParams): URLSearchParams {
  if (!params.has("fake-scale")) return params;
  const out = new URLSearchParams(params);
  for (const [key, value] of Object.entries(AREAS)) if (!out.has(key)) out.set(key, value);
  return out;
}

/** A small seeded generator, so the corpus is the same on every load. */
function seeded(seed: number): () => number {
  let s = seed >>> 0;
  return () => {
    s = (s + 0x6d2b79f5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const pick = <T>(random: () => number, from: readonly T[]): T => from[Math.floor(random() * from.length)]!;

const WORDS = [
  "payout", "ledger", "carrier", "customs", "booking", "retry", "invoice", "tracking", "refund", "settlement",
  "runbook", "mandate", "webhook", "timeout", "export", "import", "schema", "migration", "cache", "queue", "batch",
  "token", "session", "report", "alert", "metric", "dashboard", "search", "index", "label", "manifest", "parcel",
  "route", "address", "currency", "rate", "limit", "audit", "consent", "profile", "release", "rollback", "deploy",
  "config", "flag", "partner", "contract", "catalog", "pricing", "discount", "voucher", "shipment", "warehouse",
];
const VERBS = ["Fix", "Add", "Remove", "Retry", "Speed up", "Document", "Split", "Move", "Rename", "Handle", "Validate", "Cache", "Log", "Bound", "Paginate"];
const PEOPLE = [
  "mara", "jonas", "priya", "tomasz", "lena", "sven", "aylin", "noah", "elif", "kenji", "marta", "ravi", "sofia",
  "luca", "ines", "omar", "hana", "piotr", "zoe", "emil", "nadia", "felix", "chen", "amara",
];
const PROJECTS: [key: string, name: string][] = [
  ["PAY", "Payments Platform"], ["OPS", "Operations"], ["TRK", "Tracking"], ["BKG", "Booking"],
  ["PLAT", "Platform"], ["DATA", "Data"], ["MOB", "Mobile"], ["WEB", "Web"],
];
/** Eleven statuses, settled ones weighted the way a three-year-old tracker holds them. */
const STATUSES: [status: string, weight: number][] = [
  ["Backlog", 9], ["To Do", 8], ["Selected for Development", 4], ["In Progress", 6], ["In Review", 4], ["Blocked", 2],
  ["QA", 3], ["Ready for Release", 2], ["Done", 40], ["Closed", 15], ["Resolved", 7],
];
const TYPES: [type: string, weight: number][] = [["Story", 40], ["Bug", 30], ["Task", 22], ["Sub-task", 6], ["Spike", 2]];
const PRIORITIES = ["Highest", "High", "Medium", "Medium", "Medium", "Low", "Lowest"];
const LABELS = ["backend", "frontend", "infra", "security", "tech-debt", "customer", "sepa", "psp", "mobile", "data", "perf", "a11y"];

function weighted(random: () => number, from: readonly [string, number][]): string {
  const total = from.reduce((sum, [, weight]) => sum + weight, 0);
  let at = random() * total;
  for (const [value, weight] of from) if ((at -= weight) < 0) return value;
  return from.at(-1)![0];
}

function title(random: () => number, words: number): string {
  const rest = Array.from({ length: words }, () => pick(random, WORDS));
  return `${pick(random, VERBS)} ${rest.join(" ")}`;
}

/** When the fixture's mirror stands: the demo's own clock, three years of history behind it. */
const NOW = Date.parse("2026-08-22T12:00:00Z");
const iso = (ms: number) => new Date(ms).toISOString().replace(/\.\d{3}Z$/, "Z");

type Row = (typeof CORPUS)[number];

/** `n` tickets over eight projects, numbered after the demo's own keys so none collides. */
export function scaleTickets(n: number): Row[] {
  const random = seeded(20_000);
  const next = new Map(PROJECTS.map(([key]) => [key, 1001]));
  const out: Row[] = [];
  for (let i = 0; i < n; i++) {
    const [project, name] = PROJECTS[i % 3 === 0 ? 0 : Math.floor(random() * PROJECTS.length)]!;
    const number = next.get(project)!;
    next.set(project, number + 1);
    const key = `${project}-${number}`;
    const status = weighted(random, STATUSES);
    const assignee = random() < 0.12 ? null : pick(random, PEOPLE);
    const summary = title(random, 2 + Math.floor(random() * 4));
    const updated = NOW - Math.floor(Math.pow(random(), 2.2) * 3 * 365 * 86_400_000);
    const epic = `${project}-${100 + Math.floor(random() * 6)}`;
    const labels = random() < 0.6 ? [pick(random, LABELS), ...(random() < 0.3 ? [pick(random, LABELS)] : [])] : [];
    out.push({
      entity_id: `mock:${key}`,
      kind: "ticket",
      title: summary,
      updated_at: iso(updated),
      body_text: `${summary}\n\nAs ${pick(random, PEOPLE)} found in ${pick(random, WORDS)}: the ${pick(random, WORDS)} ${pick(random, WORDS)} needs a look.`,
      author: pick(random, PEOPLE),
      web_url: null,
      deleted_at: null,
      payload: {
        key,
        fields: { project: { key: project, name } },
        status,
        priority: pick(random, PRIORITIES),
        assignee,
        type: weighted(random, TYPES),
        epic,
        story_points: [1, 2, 3, 5, 8, 13][Math.floor(random() * 6)],
        blocked: status === "Blocked",
        labels: [...new Set(labels)],
        created: iso(updated - Math.floor(random() * 120) * 86_400_000),
      },
    });
  }
  return out;
}

const REPO_WORDS = ["payout", "ledger", "carrier", "customs", "booking", "tracking", "refund", "settlement", "pricing", "catalog", "auth", "gateway", "notify", "search", "report", "billing", "label", "route", "partner", "fx"];
const REPO_TAILS = ["service", "worker", "api", "client", "ui", "jobs", "sdk", "events", "store", "proxy", "cli"];
const LANGUAGES = ["Rust", "Go", "Kotlin", "TypeScript", "Python", "Java"];
const TEAMS = ["Payments", "Booking", "Tracking", "Platform", "Data", "Mobile", "Web", "Shared"];

/** About `n` forge rows: repositories, their branches, pull requests and commits, in a real Gitea's proportions. */
export function scaleForge(n: number): Row[] {
  const random = seeded(10_000);
  const repos = Math.round(n * 0.012);
  const out: Row[] = [];
  const names: string[] = [];
  for (let r = 0; r < repos; r++) {
    const name = `${REPO_WORDS[r % REPO_WORDS.length]}-${REPO_TAILS[Math.floor(r / REPO_WORDS.length) % REPO_TAILS.length]}${r >= REPO_WORDS.length * REPO_TAILS.length ? `-${r}` : ""}`;
    names.push(name);
    const full = `tidewater/${name}`;
    out.push({
      entity_id: `mock:${full}`,
      kind: "repo",
      title: full,
      updated_at: iso(NOW - Math.floor(random() * 90) * 86_400_000),
      body_text: full,
      author: pick(random, PEOPLE),
      web_url: null,
      deleted_at: null,
      payload: { full_name: full, default_branch: "main", description: `The ${name.replace("-", " ")}.`, teams: [pick(random, TEAMS)], language: pick(random, LANGUAGES) },
    });
  }
  // Repositories are not equally busy: a few hold most of the work.
  const busy = () => names[Math.floor(Math.pow(random(), 1.8) * names.length)]!;
  const branches = Math.round(n * 0.24);
  for (let b = 0; b < branches; b++) {
    const repo = busy();
    const project = pick(random, PROJECTS)[0];
    const name = `${pick(random, ["feat", "fix", "chore", "spike"])}/${project}-${1001 + Math.floor(random() * 2000)}-${pick(random, WORDS)}-${b}`;
    out.push({
      entity_id: `mock:tidewater/${repo}@refs/heads/${name}`,
      kind: "branch",
      title: name,
      updated_at: iso(NOW - Math.floor(Math.pow(random(), 2) * 400) * 86_400_000),
      body_text: name,
      author: pick(random, PEOPLE),
      web_url: null,
      deleted_at: null,
      payload: { name, repository: `tidewater/${repo}`, path: `tidewater/${repo}` },
    });
  }
  const pulls = Math.round(n * 0.44);
  const numbers = new Map<string, number>();
  for (let p = 0; p < pulls; p++) {
    const repo = busy();
    const num = (numbers.get(repo) ?? 0) + 1;
    numbers.set(repo, num);
    const open = random() < 1 / 12;
    const state = open ? "open" : random() < 0.88 ? "merged" : "closed";
    const author = pick(random, PEOPLE);
    const asked = open && random() < 0.25 ? ["mara"] : open && random() < 0.5 ? [pick(random, PEOPLE)] : [];
    out.push({
      entity_id: `mock:${repo}#${num}`,
      kind: "pr",
      title: title(random, 2 + Math.floor(random() * 3)),
      updated_at: iso(NOW - Math.floor(Math.pow(random(), 2) * 700) * 86_400_000 - p * 1000),
      body_text: "",
      author,
      web_url: null,
      deleted_at: null,
      payload: {
        num,
        repo,
        state,
        chip: open ? { text: "open", tone: "info" } : state === "merged" ? { text: "merged", tone: "ok" } : { text: "closed", tone: "muted" },
        path: `tidewater/${repo}`,
        ...(asked.length > 0 ? { requested_reviewers: asked } : {}),
      },
    });
  }
  const commits = n - repos - branches - pulls;
  for (let c = 0; c < commits; c++) {
    const repo = busy();
    const sha = Math.floor(random() * 0xfffffff).toString(16).padStart(7, "0") + c.toString(16);
    out.push({
      entity_id: `mock:tidewater/${repo}@${sha}`,
      kind: "commit",
      title: `${pick(random, WORDS)}: ${title(random, 2).toLowerCase()}`,
      updated_at: iso(NOW - Math.floor(Math.pow(random(), 2) * 700) * 86_400_000 - c * 1000),
      body_text: "",
      author: pick(random, PEOPLE),
      web_url: null,
      deleted_at: null,
      payload: { sha, repo, branch: "main", path: `tidewater/${repo}` },
    });
  }
  return out;
}

/** A generated Docs page, as `docs.ts` keeps its bulk pages. */
export interface ScalePage {
  id: string;
  title: string;
  space: string;
  parent: string | null;
  parentTitle: string | null;
}

/** The three spaces {@link scalePages} fills, by key. */
export const SCALE_SPACES: Record<string, string> = { HB: "Engineering handbook", RUN: "Runbooks", DSGN: "System design" };

/**
 * `n` pages over {@link SCALE_SPACES}: *Runbooks* (30% of them) has one page, *Service runbooks*, holding 800 children
 * at its first fold, *Engineering handbook* (45%) one, *Team pages*, holding 300; the rest fold six or seven deep, four
 * to twelve to a parent. Titles are unique, so a search names one page.
 */
export function scalePages(n: number): ScalePage[] {
  const random = seeded(5_000);
  const out: ScalePage[] = [];
  const shares: [space: string, share: number, wide: string, children: number][] = [
    ["HB", 0.45, "Team pages", 300],
    ["RUN", 0.3, "Service runbooks", 800],
    ["DSGN", 0.25, "", 0],
  ];
  let made = 0;
  shares.forEach(([space, share, wide, children], at) => {
    const count = at === shares.length - 1 ? n - made : Math.round(n * share);
    made += count;
    const here: ScalePage[] = [];
    const add = (pageTitle: string, parent: ScalePage | null) => {
      const page = { id: `confluence:scale-${space.toLowerCase()}-${here.length + 1}`, title: pageTitle, space, parent: parent?.id ?? null, parentTitle: parent?.title ?? null };
      here.push(page);
      return page;
    };
    const tops = Array.from({ length: 8 }, (_, t) => add(`${space} ${pick(random, WORDS)} overview ${t + 1}`, null));
    if (wide) {
      const parent = add(wide, null);
      const room = Math.min(children, count - here.length);
      for (let c = 0; c < room; c++) add(`${pick(random, REPO_WORDS)} ${pick(random, WORDS)} ${space.toLowerCase()}-${here.length + 1}`, parent);
    }
    // Every other page under one before it, near the top more often, so folds run six or seven deep.
    const depth = new Map<string, number>(tops.map((top) => [top.id, 0]));
    const folding = [...tops];
    while (here.length < count) {
      const parent = folding[Math.floor(Math.pow(random(), 1.3) * folding.length)]!;
      const kid = add(`${pick(random, VERBS)} ${pick(random, WORDS)} ${pick(random, WORDS)} ${space.toLowerCase()}-${here.length + 1}`, parent);
      const level = depth.get(parent.id)! + 1;
      depth.set(kid.id, level);
      if (level < 7) folding.push(kid);
    }
    out.push(...here);
  });
  return out;
}

/**
 * Adds the scale corpus's tickets and forge rows to `CORPUS` when the page asks for it. Outside `fake/`, which holds
 * features that answer commands: this one answers none, it grows the others' corpora, before their seeds run.
 */
export function seedScale(params: URLSearchParams): void {
  if (!params.has("fake-scale")) return;
  CORPUS.push(...scaleTickets(20_000), ...scaleForge(10_000));
}
