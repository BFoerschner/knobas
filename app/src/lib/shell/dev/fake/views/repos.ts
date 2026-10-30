/**
 * `?fake-ipc`: the repository page's reads (#838), the source view of a
 * repository, a branch and a commit, as `knobas-source-gitea` draws them:
 * a repository's *Branches* (the default first, `default` chip), the tree
 * a directory at a time (`files`, `files:<dir>`, `file:<path>`), *Commits*
 * (the default branch's latest; a branch's, what it carries over the default),
 * and a commit's *Diff*. A branch's whole view (#839, #840) as
 * `view/branch.rs` draws it: *Overview* (the open pull request, then *Base*,
 * *Ahead*, *Behind*, the last commit), *Commits*, the deferred *Diff* against
 * the default branch, and the toolbar's actions. A file carries a blob sha
 * (#883), so *Edit* (#845) opens under `?fake-ipc`.
 *
 * Answers no command of its own, so it sits below the feature glob:
 * `entity_view` is `entity.ts`'s, which asks {@link repoView} first. The files are prototype-6's `repoFiles` for
 * `payout-service` and its `genericFiles` for every other repository. A
 * divergence: the mock adapter draws no view for a repository at all.
 */
import type { DiffFile, RefusedWrite, SourceView, TreeEntry, ViewAction, ViewRow, WriteAsk } from "../../../../ipc/entity";
import { lineDiff } from "../../../../tf/code/line-diff";
import { CODE_FACTS, CORPUS, FAKE_IDENTITY } from "../corpus";
import { fakePdf, HOSTILE_PDF, SPEC_PDF, type FakePage } from "../../../../render/pdf-fixture";

const RETRY_MAIN = `use std::time::Duration;

use crate::psp::errors::PspError;

pub const RETRY_DELAY: Duration = Duration::from_secs(60);

/// Sends the batch once more after a fixed delay.
pub async fn send_with_retry<F, Fut>(mut send: F) -> Result<(), PspError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(), PspError>>,
{
    match send().await {
        Ok(()) => Ok(()),
        Err(_) => {
            tokio::time::sleep(RETRY_DELAY).await;
            send().await
        }
    }
}`;

const RETRY_A41 = `use std::time::Duration;
use rand::Rng;

use crate::psp::errors::PspError;

pub const BASE: Duration = Duration::from_secs(30);
pub const FACTOR: u32 = 2;
pub const MAX_ATTEMPTS: u32 = 10;
pub const JITTER: f64 = 0.2;

/// Delay before the given attempt, with jitter.
pub fn next_delay(attempt: u32) -> Duration {
    let mut rng = rand::thread_rng();
    let jitter = rng.gen_range(-JITTER..JITTER);
    let base = BASE * FACTOR.pow(attempt);
    base.mul_f64(1.0 + jitter)
}

pub fn is_transient(err: &PspError) -> bool {
    matches!(err, PspError::Http(503) | PspError::Code(ref c) if c == "TEMP_UNAVAILABLE")
}

pub async fn send_with_retry<F, Fut>(mut send: F) -> Result<(), PspError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(), PspError>>,
{
    let mut attempt = 0;
    while attempt <= MAX_ATTEMPTS {
        match send().await {
            Ok(()) => return Ok(()),
            Err(e) if is_transient(&e) => {
                tokio::time::sleep(next_delay(attempt)).await;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
    Err(PspError::GaveUp { attempts: attempt })
}`;

const RETRY_HEAD = RETRY_A41.replace("MAX_ATTEMPTS: u32 = 10", "MAX_ATTEMPTS: u32 = 5")
  .replace("JITTER: f64 = 0.2", "JITTER: f64 = 0.1")
  .replace("with jitter.", "with ±10 % jitter.");

const TEST_HEAD = `use payout_service::sepa::retry::{send_with_retry, MAX_ATTEMPTS};
use payout_service::psp::errors::PspError;
use std::sync::atomic::{AtomicU32, Ordering};

#[tokio::test(start_paused = true)]
async fn gives_up_after_max_attempts() {
    let attempts = AtomicU32::new(0);
    let result = send_with_retry(|| async {
        attempts.fetch_add(1, Ordering::SeqCst);
        Err(PspError::Http(503))
    })
    .await;
    assert!(result.is_err());
    assert_eq!(attempts.load(Ordering::SeqCst), MAX_ATTEMPTS);
}`;

/** `payout-service`'s README: GitHub-flavoured Markdown for the Markdown component (#2255). */
const PAYOUT_README = [
  "# payout-service",
  "",
  "![payout-service](docs/img/logo.png)",
  "",
  "Sends payout batches to the PSP and books them in the ledger. The retry policy is in [src/sepa/retry.rs](src/sepa/retry.rs);",
  "the PSP's own docs are on [their portal](https://psp.example.com/docs).",
  "",
  "## Run locally",
  "",
  "```sh",
  "cargo run -- --env dev",
  "```",
  "",
  "## Status",
  "",
  "| Area | State | Owner |",
  "|---|---|---|",
  "| SEPA batches | live | @mara |",
  "| Instant payouts | beta | @jonas |",
  "| Refunds | ~~manual~~ automated | @priya |",
  "",
  "- [x] Retry transient PSP errors",
  "- [x] Book each batch in the ledger",
  "- [ ] Dead-letter queue with replay",
  "",
  "## How a delivery flows",
  "",
  "```mermaid",
  "flowchart LR",
  "  pay[Payment] --> outbox[(outbox)]",
  "  outbox --> worker[retry worker]",
  "  worker -->|5xx: backoff| worker",
  "  worker --> psp[PSP]",
  "```",
  "",
  "```mermaid",
  "sequenceDiagram",
  "  participant S as Scheduler",
  "  participant P as PSP",
  "  S->>P: POST /batches",
  "  P-->>S: 202 Accepted",
  "  Note over S,P: retried with backoff on 5xx",
  "```",
  "",
  "```rust",
  "pub async fn run() -> anyhow::Result<()> {",
  "    sepa::batch::schedule().await",
  "}",
  "```",
  "",
  "## Architecture",
  "",
  "![How a batch flows](docs/img/flow.svg)",
].join("\n");

/** `payout-service`'s README images (#2256), read through the byte read: a PNG, and an SVG with a script the sanitizer drops. */
const PAYOUT_IMAGES: Record<string, () => Uint8Array> = {
  "docs/img/logo.png": () =>
    Uint8Array.from(
      atob(
        "iVBORw0KGgoAAAANSUhEUgAAAGAAAAAgCAIAAABiouoDAAAAW0lEQVR42u3YsQ0AIAwDsF7CicxczMLCCTxQcUBrKWMmT1Fi35NmrJmmWz8AAQIECBAgQGWBQPz7gAABAgQIEKC6QCAsaUCAAAECBMgfZCha0oAAAQIECFCr/gP3NZ9LUPv2+wAAAABJRU5ErkJggg==",
      ),
      (c) => c.charCodeAt(0),
    ),
  "docs/img/flow.svg": () =>
    new TextEncoder().encode(
      [
        '<svg width="360" height="60" viewBox="0 0 360 60" onload="window.__pwned = \'svg-image\'">',
        "<script>window.__pwned = 'svg-image-script'</script>",
        '<rect x="2" y="12" width="96" height="36" rx="6" fill="#e6f0eb" stroke="#1e786e"/>',
        '<text x="50" y="35" font-family="sans-serif" font-size="13" text-anchor="middle" fill="#1e786e">batch</text>',
        '<path d="M100 30 H128" stroke="#1e786e" stroke-width="2"/>',
        '<rect x="132" y="12" width="96" height="36" rx="6" fill="#e6f0eb" stroke="#1e786e"/>',
        '<text x="180" y="35" font-family="sans-serif" font-size="13" text-anchor="middle" fill="#1e786e">PSP</text>',
        '<path d="M230 30 H258" stroke="#1e786e" stroke-width="2"/>',
        '<rect x="262" y="12" width="96" height="36" rx="6" fill="#e6f0eb" stroke="#1e786e"/>',
        '<text x="310" y="35" font-family="sans-serif" font-size="13" text-anchor="middle" fill="#1e786e">ledger</text>',
        "</svg>",
      ].join(""),
    ),
};

/** Diagram files (#2258, #2259): one that draws, with a click directive that must not run, and one that does not parse. */
const DIAGRAMS = {
  "docs/architecture.mmd": [
    "flowchart TB",
    "  %% payments-api and what it talks to",
    '  shop["shop frontend"] -->|POST /pay| api["payments-api"]',
    '  api --> psp["card PSP"]',
    '  api --> sepa["SEPA bank"]',
    '  api --> db[("postgres · payments")]',
    '  click api href "javascript:window.__pwned = \'click\'"',
    "  click psp call evil()",
  ].join("\n"),
  "docs/broken.mermaid": ["flowchart TB", '  shop["shop frontend"] --> api', "  api --> |outbox worker"].join("\n"),
  // D2 and Graphviz (#2259): a README with one fence of each, a file of each that draws, and one of each that does not parse.
  "docs/README.md": [
    "# Diagrams",
    "",
    "```d2",
    "shop -> api: POST /pay",
    "api -> outbox: enqueue",
    'outbox -> psp: "retry worker" {style.stroke-dash: 3}',
    'api.link: "javascript:window.__pwned = \'d2\'"',
    "```",
    "",
    "```dot",
    "digraph deps {",
    "  rankdir=LR;",
    '  "payments-api" -> "sepa-batch" -> ledger;',
    '  "payments-api" -> psp [label="card"];',
    "}",
    "```",
  ].join("\n"),
  "docs/flow.d2": ["direction: right", "shop -> api: POST /pay", "api -> db: payments {shape: cylinder}", "api -> psp: authorise"].join("\n"),
  "docs/deps.gv": ["graph services {", '  api -- db [label="sql"];', "  api -- psp;", "  api -- sepa;", "}"].join("\n"),
  "docs/broken.d2": ["shop -> api", "api -> {"].join("\n"),
  "docs/broken.dot": ["digraph {", "  api -> ", "}"].join("\n"),
};

/** `?fake-md-hostile`'s file (#2255): raw HTML that must not run, for the sanitizer's headless Chrome witness. */
const HOSTILE_MD = [
  "# Hostile",
  "",
  "<script>window.__pwned = 'script'</script>",
  '<img src="nope.png" onerror="window.__pwned = \'onerror\'">',
  '<p onclick="window.__pwned = \'onclick\'" style="color: red">styled paragraph</p>',
  '<a href="javascript:window.__pwned=\'href\'">javascript link</a>',
  '<a href="JaVaScRiPt:window.__pwned=\'mixed\'">mixed-case link</a>',
  '<svg><script>window.__pwned = \'svg\'</script></svg>',
  '<iframe srcdoc="<script>parent.__pwned = \'iframe\'</script>"></iframe>',
  "",
  "[markdown link](javascript:window.__pwned='md')",
].join("\n");

/** `payout-service` on its default branch, as prototype-6's `repoFiles` (with `main`'s retry). */
const PAYOUT_MAIN: Record<string, string> = {
  "Cargo.toml": `[package]\nname = "payout-service"\nversion = "1.9.0-rc1"\nedition = "2021"\n\n[dependencies]\ntokio = { version = "1.40", features = ["full"] }\nreqwest = "0.12"\nrand = "0.8"\nthiserror = "1"`,
  "README.md": PAYOUT_README,
  ...DIAGRAMS,
  "src/main.rs": `use payout_service::run;\n\n#[tokio::main]\nasync fn main() {\n    run().await.expect("payout-service failed");\n}`,
  "src/lib.rs": `pub mod psp;\npub mod sepa;\n\npub async fn run() -> anyhow::Result<()> {\n    sepa::batch::schedule().await\n}`,
  "src/psp/client.rs": `use crate::psp::errors::PspError;\n\npub struct PspClient { base_url: String }\n\nimpl PspClient {\n    pub async fn send_batch(&self, body: &[u8]) -> Result<(), PspError> {\n        todo!("POST {}/batches", self.base_url)\n    }\n}`,
  "src/psp/errors.rs": `#[derive(Debug, thiserror::Error)]\npub enum PspError {\n    #[error("http {0}")]\n    Http(u16),\n    #[error("psp code {0}")]\n    Code(String),\n    #[error("gave up after {attempts} attempts")]\n    GaveUp { attempts: u32 },\n}`,
  "src/sepa/mod.rs": `pub mod batch;\npub mod retry;`,
  "src/sepa/batch.rs": `pub async fn schedule() -> anyhow::Result<()> {\n    // 13:00 daily batch, see PAY-231\n    Ok(())\n}`,
  "src/sepa/retry.rs": RETRY_MAIN,
  ".teamcity/settings.kts": `import jetbrains.buildServer.configs.kotlin.*\n\nproject {\n    buildType(Payout_Build)\n    buildType(Payout_IntegrationTests)\n}`,
};

/** The job's branch: the retry rewritten, its test and the design note added. */
const PAYOUT_FEATURE: Record<string, string> = {
  ...PAYOUT_MAIN,
  "src/sepa/retry.rs": RETRY_HEAD,
  "tests/sepa_retry.rs": TEST_HEAD,
  "docs/backoff.md": `# Backoff for SEPA payouts\n\nTransient PSP errors (HTTP 503, TEMP_UNAVAILABLE) are retried.\n\n- base delay 30 s, factor 2\n- at most 5 attempts\n- jitter ±10 %`,
};

const PAYOUT = "mock:tidewater/payout-service";
const FEATURE = "feat/PAY-231-idempotent-retry";
const FEATURE_NAME = FEATURE;

/** The three commits the job's branch carries, newest first: id, subject, author, when, the change. */
export const PAYOUT_COMMITS: [sha: string, subject: string, author: string, at: string, path: string, before: string, after: string][] = [
  ["c90d11e4a7b2", "PAY-231: jitter in backoff, cap at 5 attempts", "mara", "2026-08-22T09:42:00Z", "src/sepa/retry.rs", RETRY_A41, RETRY_HEAD],
  ["a41f2c08d3e9", "PAY-231 backoff jitter", "mara", "2026-08-22T08:02:00Z", "src/sepa/retry.rs", RETRY_MAIN, RETRY_A41],
  ["7be0e4f15c60", "PAY-231: retry SEPA payouts on transient PSP errors", "mara", "2026-08-21T15:30:00Z", "tests/sepa_retry.rs", "", TEST_HEAD],
];

/**
 * `?fake-rendered-diff` (#2279): the newest commit also changes a README, a Mermaid diagram and a PNG, adds a
 * Markdown file and deletes one, so its diff offers *Rendered*. Each changed file reads whole at that commit
 * (after) and at its parent (before); the PNG's bytes the same way.
 */
const RENDERED_DIFF: Record<string, [before: string | null, after: string | null]> = {
  "README.md": [
    "# payout-service\n\nPays out **SEPA** batches once a day.\n\n- retries: 3\n- backoff: none",
    "# payout-service\n\nPays out **SEPA** batches once a day, retrying transient PSP errors.\n\n- retries: 5\n- backoff: exponential with jitter\n\n> See `docs/backoff.md`.",
  ],
  "docs/retry-flow.mmd": ["flowchart LR\n  batch --> PSP --> ledger", "flowchart LR\n  batch --> PSP\n  PSP -->|503| retry\n  retry --> PSP\n  PSP --> ledger"],
  "docs/rollout.md": [null, "# Rollout\n\n1. Ship behind `retry.enabled`.\n2. Turn it on for one PSP.\n3. Watch the **retry rate** for a day."],
  "docs/old-retry.md": ["# Old retry notes\n\nRetries were manual: an operator re-sent the batch.", null],
};
const LOGO_AFTER =
  "iVBORw0KGgoAAAANSUhEUgAAAGAAAAAgCAIAAABiouoDAAAAW0lEQVR42u3YIREAIBBE0UtCRDTByITBIAhAAYYA3Jv5ctWTG7uHHiEABAgQIECA/gUqrV4ba17LtgcECBAgQIAAAQIECBAgQIAAAfoNyCXmUQQECBAgQICSdgCzYkA8nT/7cAAAAABJRU5ErkJggg==";
const renderedDiffOn = () => new URLSearchParams(globalThis.location?.search).has("fake-rendered-diff");
const bytesOf = (base64: string) => Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));

/** `?fake-rendered-diff`'s files on the newest commit's diff. */
function renderedDiffFiles(): DiffFile[] {
  const text = Object.entries(RENDERED_DIFF).map(([path, [before, after]]) => {
    const made = lineDiff(path, before ?? "", after ?? "");
    return { ...made, change: before === null ? "added" : after === null ? "deleted" : made.change } as DiffFile;
  });
  const png: DiffFile = { path: "docs/img/logo.png", old_path: null, change: "binary", additions: 0, deletions: 0, hunks: [], truncated: false, threads: [] };
  return [...text, png];
}

/**
 * `?fake-pdf-diff` (#2282): the newest commit also changes three PDFs, read as bytes at it (after) and its parent
 * (before). `docs/spec.pdf` changes the text of page 2 of 4; `docs/handbook.pdf` gains a page between pages 1
 * and 2 of 3; `docs/brand.pdf` changes only the colour of the image on page 2 of 3 (page 3 keeps its image).
 */
const pdfPage = (title: string, line: string, image?: string): FakePage => ({ lines: [title, line], ...(image ? { image } : {}) });
const SPEC_PAGES = [
  pdfPage("Payout service: design", "Pays out SEPA batches once a day."),
  pdfPage("Retries", "Transient PSP errors are retried three times."),
  pdfPage("Backoff", "None: a retry goes out at once."),
  pdfPage("Ledger", "Every payout is booked once it settles."),
];
const HANDBOOK_PAGES = [pdfPage("On-call handbook", "Who is paged, and when."), pdfPage("Escalation", "Page the PSP's desk after 30 minutes."), pdfPage("Hand-over", "Write up the shift in the channel.")];
const BRAND_PAGES = (colour: string) => [pdfPage("Brand", "The logo and its colours."), pdfPage("Logo", "The mark, 2 by 2.", colour), pdfPage("Type", "Helvetica throughout.", "8a5600")];
const PDF_DIFF: Record<string, [before: () => Uint8Array, after: () => Uint8Array]> = {
  "docs/spec.pdf": [
    () => fakePdf({ pages: SPEC_PAGES }),
    () => fakePdf({ pages: SPEC_PAGES.map((page, i) => (i === 1 ? pdfPage("Retries", "Transient PSP errors are retried five times, with jitter.") : page)) }),
  ],
  "docs/handbook.pdf": [
    () => fakePdf({ pages: HANDBOOK_PAGES }),
    () => fakePdf({ pages: [HANDBOOK_PAGES[0]!, pdfPage("Runbooks", "Each alert links its runbook."), ...HANDBOOK_PAGES.slice(1)] }),
  ],
  "docs/brand.pdf": [() => fakePdf({ pages: BRAND_PAGES("1f7444") }), () => fakePdf({ pages: BRAND_PAGES("2f55d4") })],
};
const pdfDiffOn = () => new URLSearchParams(globalThis.location?.search).has("fake-pdf-diff");
const pdfDiffFiles = (): DiffFile[] =>
  Object.keys(PDF_DIFF).map((path) => ({ path, old_path: null, change: "binary", additions: 0, deletions: 0, hunks: [], truncated: false, threads: [] }));

/** A `?fake-rendered-diff` file at the newest commit (`at` 0, after) or its parent (`at` 1, before); `undefined` for none. */
function renderedDiffSide(at: number, path: string): string | null | undefined {
  if (!renderedDiffOn() || (at !== 0 && at !== 1)) return undefined;
  const sides = RENDERED_DIFF[path];
  return sides === undefined ? undefined : sides[at === 0 ? 1 : 0];
}

/** A commit's entity id on `payout-service`, in the Gitea key grammar (`owner/repo@sha`). */
export const payoutCommitId = (sha: string) => `${PAYOUT}@${sha}`;

/** The default branch's latest, none of them in the mirror: drawn plain, opening nothing. */
const MAIN_COMMITS: [string, string, string, string][] = [
  ["5d77e1a2c3b4", "Bump tokio to 1.40", "jonas", "2026-08-21T10:15:00Z"],
  ["3a9c40b1e2f7", "Settlement export: one file per currency", "priya", "2026-08-20T14:02:00Z"],
  ["e81f2d6c0a95", "Ledger booking for partial refunds", "tomasz", "2026-08-19T09:30:00Z"],
  ["b07c93e4d1a8", "README: how to run locally", "jonas", "2026-08-18T16:44:00Z"],
];

/** `?fake-file-kinds`'s files (#2254). */
const FILE_KINDS: Record<string, string> = {
  "web/client.ts": `import type { Payout } from "./types";\n\n/** Sends one payout batch; retried by the service, not here. */\nexport async function sendBatch(base: string, batch: Payout[]): Promise<number> {\n  const response = await fetch(\`\${base}/batches\`, { method: "POST", body: JSON.stringify(batch) });\n  return response.status; // 202 when queued\n}`,
  "notes/plan.zzq": "fn main() { this is no language knobas knows }",
};

/**
 * `?fake-binary`'s files (#2260): binary files in payout-service's tree, read through the byte read. A three-page
 * PDF with an external link and a link to a page, a hostile PDF (JavaScript on open, script and launch links, a
 * scripted field) and the README's PNG.
 */
const BINARY: Record<string, () => Uint8Array> = {
  "docs/spec.pdf": SPEC_PDF,
  "docs/hostile.pdf": HOSTILE_PDF,
  "docs/img/logo.png": () => PAYOUT_IMAGES["docs/img/logo.png"]!(),
};
const binaryOf = (path: string) => (new URLSearchParams(globalThis.location?.search).has("fake-binary") ? BINARY[path] : undefined);

function filesOf(repoId: string, branch: string | null): Record<string, string> | null {
  const repo = CORPUS.find((entry) => entry.entity_id === repoId && entry.kind === "repo");
  if (!repo) return null;
  if (repoId === PAYOUT) {
    const files = branch === FEATURE ? PAYOUT_FEATURE : PAYOUT_MAIN;
    const query = new URLSearchParams(globalThis.location?.search);
    // `?fake-file-kinds` (#2254): a TypeScript file and a file in no known language, for highlighting.
    const kinds = {
      ...(query.has("fake-file-kinds") ? FILE_KINDS : {}),
      // `?fake-md-hostile` (#2255): a Markdown file of raw HTML that tries to run script.
      ...(query.has("fake-md-hostile") ? { "docs/hostile.md": HOSTILE_MD } : {}),
      // `?fake-binary` (#2260): binary files; the text here is a stand-in, `binaryOf` has their bytes.
      ...(query.has("fake-binary") ? Object.fromEntries(Object.keys(BINARY).map((path) => [path, ""])) : {}),
    };
    // `?fake-file-lines=<n>` (perf audit, #2200's class): one file of `n` lines.
    const n = Number(query.get("fake-file-lines") ?? 0) || 0;
    if (n === 0) return { ...files, ...kinds };
    return { ...files, ...kinds, "BULK.rs": Array.from({ length: n }, (_, i) => `    let value_${i} = payout.amount * ${i};`).join("\n") };
  }
  const name = repo.title.slice(repo.title.indexOf("/") + 1);
  return {
    "README.md": `# ${name}\n\nOwned by the payments team.`,
    "src/main.rs": `// ${name}\n`,
    ".teamcity/settings.kts": "project { }",
  };
}

const section = (id: string, title: string, blocks: unknown[], count: number | null = null) => ({
  id,
  title,
  count,
  deferred: false,
  blocks,
});

/** One directory's listing: its directories first, then its files, each by name. */
function listing(files: Record<string, string>, dir: string): TreeEntry[] {
  const prefix = dir === "" ? "" : `${dir}/`;
  const dirs = new Set<string>();
  const entries: TreeEntry[] = [];
  for (const [path, text] of Object.entries(files)) {
    if (!path.startsWith(prefix)) continue;
    const rest = path.slice(prefix.length);
    if (rest.includes("/")) dirs.add(rest.slice(0, rest.indexOf("/")));
    else entries.push({ name: rest, kind: "file", size: binaryOf(path)?.().length ?? text.length, section: `file:${path}` });
  }
  return [
    ...[...dirs].sort().map((name): TreeEntry => ({ name, kind: "dir", size: null, section: `files:${prefix}${name}` })),
    ...entries.sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase())),
  ];
}

const age = (at: string) => {
  const days = Math.max(0, Math.round((Date.parse("2026-08-22T14:30:00Z") - Date.parse(at)) / 86_400_000));
  return days === 0 ? "today" : days === 1 ? "1 day ago" : `${days} days ago`;
};

function commitRow(entityId: string | null, sha: string, subject: string, author: string, at: string): ViewRow {
  return { entity_id: entityId, title: subject, meta: `${author} · ${age(at)}`, tone: null, chip: sha.slice(0, 10), url: null };
}

/** Prototype-6's `lineDiff` and `hunks`, shared with the commit dialog (#845). */
const diffOf = (path: string, before: string, after: string): DiffFile => lineDiff(path, before, after);

/**
 * PR #142's change (#842), as prototype-6's `prFiles(142)`: the retry
 * rewritten, its test and the design note added, against `main`.
 */
export function payoutPrFiles(): DiffFile[] {
  return [
    diffOf("src/sepa/retry.rs", RETRY_MAIN, RETRY_HEAD),
    diffOf("tests/sepa_retry.rs", "", TEST_HEAD),
    diffOf("docs/backoff.md", "", PAYOUT_FEATURE["docs/backoff.md"]!),
  ];
}

/** A blob sha for a file's text: the same text, the same sha, as git's is. */
function blobOf(text: string): string {
  let hash = 0x811c9dc5;
  for (let at = 0; at < text.length; at += 1) hash = Math.imul(hash ^ text.charCodeAt(at), 0x01000193) >>> 0;
  return hash.toString(16).padStart(8, "0").repeat(5);
}

/**
 * Prototype-6's branches of `payout-service` (`branches`, `branchMeta`), the
 * job's branch under the fixture's own name: ahead and behind `main`, the
 * pull request open from it, its tip, and what it carries.
 */
const PAYOUT_BRANCHES: {
  name: string;
  ahead: number;
  behind: number;
  pr: string | null;
  tip: [sha: string, subject: string, author: string, at: string];
}[] = [
  { name: FEATURE_NAME, ahead: 3, behind: 0, pr: "mock:payout-service#142", tip: ["c90d11e4a7b2", "PAY-231: jitter in backoff, cap at 5 attempts", "mara", "2026-08-22T09:42:00Z"] },
  { name: "feature/csv-export", ahead: 4, behind: 1, pr: "mock:payout-service#150", tip: ["d41e0a9b7c21", "CSV export: one row per payout", "jonas", "2026-08-22T11:40:00Z"] },
  { name: "spike/sepa-instant", ahead: 2, behind: 9, pr: null, tip: ["9a1b2c3d4e5f", "Spike: SEPA Instant through the PSP sandbox", "mara", "2026-08-13T15:10:00Z"] },
  { name: "chore/bump-tokio", ahead: 0, behind: 6, pr: null, tip: ["5d77e1a2c3b4", "Bump tokio to 1.40", "jonas", "2026-08-17T10:15:00Z"] },
];

// `code_facts` for the branches above and `main` (#986): the mirror holds every
// branch, its author its tip's.
CODE_FACTS.push((ids) =>
  [
    { name: "main", author: "jonas" },
    ...PAYOUT_BRANCHES.map(({ name, tip }) => ({ name, author: tip[2] })),
  ]
    .map(({ name, author }) => ({ entity_id: `${PAYOUT}@refs/heads/${name}`, author }))
    .filter(({ entity_id }) => ids.has(entity_id))
    .map(({ entity_id, author }) => ({ entity_id, author, mine: author === FAKE_IDENTITY, language: null, teams: [] })),
);

const branchAction = (write_op: string, label: string, over: Partial<ViewAction> = {}): ViewAction => ({
  write_op,
  label,
  fields: [],
  confirm: null,
  primary: false,
  danger: false,
  disabled: null,
  with_settings: false,
  values: {},
  entity: PAYOUT,
  ...over,
}) as ViewAction;

/** What a branch other than the job's changes against `main`: one file. */
function sideDiff(name: string): DiffFile[] {
  if (name === "feature/csv-export")
    return [diffOf("src/export/csv.rs", "", "pub fn header() -> &'static str {\n    \"payout_id,amount,currency,status\"\n}")];
  if (name === "spike/sepa-instant")
    return [diffOf("src/sepa/batch.rs", PAYOUT_MAIN["src/sepa/batch.rs"]!, PAYOUT_MAIN["src/sepa/batch.rs"]!.replace("// 13:00 daily batch, see PAY-231", "// 13:00 daily batch, see PAY-231\n    // spike: instant payouts skip the batch"))];
  return [];
}

/**
 * `?fake-branches=<n>` (perf audit, #2200's class): `payout-service` has `n`
 * more branches, `bulk/branch-0000` on, each one commit ahead and no PR.
 */
const BULK_BRANCHES: (typeof PAYOUT_BRANCHES)[number][] = Array.from(
  { length: Number(new URLSearchParams(globalThis.location?.search).get("fake-branches") ?? 0) || 0 },
  (_, i) => ({
    name: `bulk/branch-${String(i).padStart(4, "0")}`,
    ahead: 1,
    behind: i % 7,
    pr: null,
    tip: [`b${String(i).padStart(11, "0")}`, `Bulk change ${i}`, i % 3 ? "jonas" : "mara", "2026-08-20T09:00:00Z"],
  }),
);

/** A branch of `payout-service`'s whole view, or one of its sections. */
function branchView(name: string, sectionId: string | null): SourceView | undefined {
  const branch = PAYOUT_BRANCHES.find((candidate) => candidate.name === name) ?? BULK_BRANCHES.find((candidate) => candidate.name === name);
  if (branch === undefined) return undefined;
  const isJob = name === FEATURE_NAME;
  const carried = isJob
    ? PAYOUT_COMMITS.map(([sha, subject, author, at]) => commitRow(payoutCommitId(sha), sha, subject, author, at))
    : branch.ahead === 0
      ? []
      : [commitRow(null, branch.tip[0], branch.tip[1], branch.tip[2], branch.tip[3])];
  const files = isJob
    ? [
        diffOf("docs/backoff.md", "", PAYOUT_FEATURE["docs/backoff.md"]!),
        diffOf("src/sepa/retry.rs", RETRY_MAIN, RETRY_HEAD),
        diffOf("tests/sepa_retry.rs", "", TEST_HEAD),
      ]
    : sideDiff(name);
  if (sectionId === "diff")
    return { sections: [section("diff", "Diff", [{ type: "diff", files, truncated: false, commentable: null, gone: [] }], files.length)], actions: [] } as unknown as SourceView;
  const pr = branch.pr === null ? null : CORPUS.find((row) => row.entity_id === branch.pr);
  const overview = section("overview", "Overview", [
    ...(pr
      ? [{ type: "entities", title: "Pull request", rows: [{ entity_id: pr.entity_id, title: pr.title, meta: `#${pr.entity_id.split("#")[1]} · ${name} → main`, tone: "info", chip: "open", url: null }] }]
      : []),
    {
      type: "facts",
      facts: [
        { label: "Base", value: "main", tone: null },
        { label: "Ahead", value: String(branch.ahead), tone: null },
        { label: "Behind", value: String(branch.behind), tone: null },
        ...(pr ? [] : [{ label: "Pull request", value: "none", tone: "muted" }]),
      ],
    },
    { type: "entities", title: "Last commit", rows: [commitRow(null, ...branch.tip)] },
  ]);
  const commits = section("commits", "Commits", [{ type: "entities", title: null, rows: carried }], carried.length);
  const actions: ViewAction[] = [
    ...(pr ? [] : [branchAction("create_pull_request", "Open pull request…", { primary: true, disabled: branch.ahead === 0 ? "No commits ahead of main" : null })]),
    branchAction("create_branch", "New branch…"),
    branchAction("delete_branch", "Delete branch…", { danger: true, entity: null }),
  ];
  const sections = [overview, commits, { ...section("diff", "Diff", [], files.length), deferred: true }];
  // One section asked for is that section alone, without the toolbar, as the adapter answers it.
  if (sectionId !== null) return { sections: sections.filter((candidate) => candidate.id === sectionId), actions: [] } as unknown as SourceView;
  return { sections, actions } as unknown as SourceView;
}

/** The tree's three kinds of section, at a ref. */
function treeSection(files: Record<string, string>, id: string, ref: string) {
  if (id === "files" || id.startsWith("files:")) {
    const dir = id === "files" ? "" : id.slice("files:".length);
    return section(id, "Files", [{ type: "tree", trail: [], entries: listing(files, dir), truncated: false, url: null }]);
  }
  const path = id.slice("file:".length);
  const text = files[path];
  if (text === undefined) throw { code: "not_found", message: `gitea: no file ${path} at ${ref}`, source_id: null };
  const bytes = binaryOf(path)?.();
  if (bytes !== undefined)
    return section(id, "Files", [
      { type: "file", trail: [], path, text: "", language: null, size: bytes.length, truncated: false, binary: true, url: null, sha: blobOf(path), commit: ref },
    ]);
  // No action beside the file (#2241): *Commit change…*'s standing is `refused_writes`' ({@link repoRefusals}).
  return section(id, "Files", [
    { type: "file", trail: [], path, text, language: null, size: text.length, truncated: false, binary: false, url: null, sha: blobOf(text), commit: ref },
  ]);
}

/**
 * `entity_file_bytes` for the repositories this module draws (#2256): a README image, or any file's text as bytes,
 * at the entity's branch (the fake's commit is the branch name it was read at). `undefined` for another entity.
 */
export function repoFileBytes(entityId: string, path: string): Uint8Array | undefined {
  // `?fake-rendered-diff`'s PNG at the newest commit (after) and its parent (before).
  const commitAt = PAYOUT_COMMITS.findIndex(([sha]) => payoutCommitId(sha) === entityId);
  const pdf = PDF_DIFF[path];
  if (commitAt !== -1 && commitAt <= 1 && pdf !== undefined && pdfDiffOn()) return pdf[commitAt === 0 ? 1 : 0]();
  if (commitAt !== -1 && renderedDiffOn()) {
    if (path === "docs/img/logo.png" && commitAt <= 1) return commitAt === 0 ? bytesOf(LOGO_AFTER) : PAYOUT_IMAGES[path]!();
    throw { code: "not_found", message: `gitea: no file ${path} at this commit`, source_id: null };
  }
  const branchAt = entityId.indexOf("@refs/heads/");
  const repoId = branchAt === -1 ? entityId : entityId.slice(0, branchAt);
  const branch = branchAt === -1 ? null : entityId.slice(branchAt + "@refs/heads/".length);
  const files = filesOf(repoId, branch);
  if (files === null) return undefined;
  const image = repoId === PAYOUT ? (binaryOf(path) ?? PAYOUT_IMAGES[path]) : undefined;
  const text = files[path];
  if (image === undefined && text === undefined) throw { code: "not_found", message: `gitea: no file ${path}`, source_id: null };
  return image !== undefined ? image() : new TextEncoder().encode(text);
}

/** Why `main` takes no commit straight onto it: protected, as prototype-6 has it (#985). */
export const MAIN_PROTECTED =
  "main is a protected branch that this account may not push to: commit to a new branch and open a pull request instead";

/**
 * `refused_writes` for the repositories this module draws (#2241): a commit
 * onto `main` (the repository itself, or its `main` branch) is refused, as
 * Gitea's answer for a protected branch says it; every other ask is taken.
 */
export function repoRefusals(asks: readonly WriteAsk[]): RefusedWrite[] {
  return asks
    .filter((ask) => {
      if (ask.op !== "commit_file") return false;
      const branchAt = ask.entity.indexOf("@refs/heads/");
      const repoId = branchAt === -1 ? ask.entity : ask.entity.slice(0, branchAt);
      const branch = branchAt === -1 ? "main" : ask.entity.slice(branchAt + "@refs/heads/".length);
      return branch === "main" && filesOf(repoId, null) !== null;
    })
    .map((ask) => ({ ...ask, reason: MAIN_PROTECTED }));
}

/**
 * The view of a repository, one of its branches or one of `payout-service`'s
 * commits, or `undefined` for an entity this module does not draw.
 */
export function repoView(entityId: string, sectionId: string | null): SourceView | undefined {
  const at = PAYOUT_COMMITS.findIndex(([sha]) => payoutCommitId(sha) === entityId);
  const commit = PAYOUT_COMMITS[at];
  if (commit) {
    const [, , , , path, before, after] = commit;
    if (sectionId?.startsWith("file:")) {
      const wanted = sectionId.slice("file:".length);
      const side = renderedDiffSide(at, wanted);
      if (side !== undefined) {
        if (side === null) throw { code: "not_found", message: `gitea: no file ${wanted}`, source_id: null };
        return { sections: [section(sectionId, "Files", [{ type: "file", trail: [], path: wanted, text: side, language: null, size: side.length, truncated: false, binary: false, url: null, sha: blobOf(side), commit: null }])], actions: [] } as unknown as SourceView;
      }
    }
    // Its file whole at the commit, as a commit at a sha answers it (#2267's expanded context).
    if (sectionId === `file:${path}`)
      return { sections: [section(sectionId, "Files", [{ type: "file", trail: [], path, text: after, language: null, size: after.length, truncated: false, binary: false, url: null, sha: blobOf(after) }])], actions: [] } as unknown as SourceView;
    // The parent: the next commit down, where it changed the same file to this one's `before`.
    const next = PAYOUT_COMMITS[at + 1];
    const parent = next && next[4] === path && next[6] === before ? payoutCommitId(next[0]) : null;
    const overview = section("overview", "Overview", [{ type: "entities", title: "Parent", rows: [{ entity_id: parent, title: next?.[0].slice(0, 10) ?? "", meta: null, tone: null, chip: null, url: null }] }]);
    // `?fake-diff-lines=<n>` (#2269): forty more files of n/40 added lines each.
    const bulk = Number(new URLSearchParams(globalThis.location?.search).get("fake-diff-lines") ?? 0) || 0;
    const more = bulk === 0 ? [] : Array.from({ length: 40 }, (_, f) => diffOf(`bulk/f${String(f).padStart(2, "0")}.rs`, "", Array.from({ length: Math.ceil(bulk / 40) }, (_, i) => `let v${f}_${i} = ${i};`).join("\n")));
    const diff = section("diff", "Diff", [{ type: "diff", files: [diffOf(path, before, after), ...(at === 0 && renderedDiffOn() ? renderedDiffFiles() : []), ...(at === 0 && pdfDiffOn() ? pdfDiffFiles() : []), ...more], truncated: false, commentable: null, gone: [] }], 1 + more.length);
    // The whole view's toolbar (#707): a branch from this commit, its whole sha the value.
    const from = { value: commit[0], label: commit[0].slice(0, 10) };
    const actions =
      sectionId === null
        ? [
            branchAction("create_branch", "New branch…", {
              fields: [
                { name: "name", label: "Name", kind: { type: "text", suggestions: [] }, value: null, required: true, placeholder: "feature/…" },
                { name: "from_ref", label: "From", kind: { type: "choice", options: [from] }, value: null, required: true, placeholder: null },
              ],
            }),
          ]
        : [];
    return { sections: sectionId === null ? [overview, diff] : [diff], actions } as SourceView;
  }
  const branchAt = entityId.indexOf("@refs/heads/");
  const repoId = branchAt === -1 ? entityId : entityId.slice(0, branchAt);
  const branch = branchAt === -1 ? null : entityId.slice(branchAt + "@refs/heads/".length);
  const files = filesOf(repoId, branch);
  if (files === null) return undefined;
  const ref = branch ?? "main";
  const one = (built: ReturnType<typeof section>) => ({ sections: [built], actions: [] }) as unknown as SourceView;
  // Any two of the branch's commits (#2271): `compare:<base>...<head>`, each end
  // a sha (the base maybe `<sha>^`) or a branch, as the Gitea adapter reads it.
  if (repoId === PAYOUT && branch === null && sectionId?.startsWith("compare:")) {
    const [base = "", head = ""] = sectionId.slice("compare:".length).split("...");
    const at = (end: string) => PAYOUT_COMMITS.findIndex(([sha]) => sha === end || sha.startsWith(end));
    const newest = Math.max(at(head), 0);
    const parent = base.endsWith("^");
    const below = at(parent ? base.slice(0, -1) : base);
    const oldest = below === -1 ? PAYOUT_COMMITS.length - 1 : parent ? below : below - 1;
    // One file per path, as a compare has it: from its text before the oldest commit to after the newest.
    const sides = new Map<string, [before: string, after: string]>();
    for (const [, , , , path, before, after] of PAYOUT_COMMITS.slice(newest, oldest + 1).reverse()) {
      sides.set(path, [sides.get(path)?.[0] ?? before, after]);
    }
    const diffs = [...sides].map(([path, [before, after]]) => diffOf(path, before, after));
    return one(section(sectionId, "Diff", [{ type: "diff", files: diffs, truncated: false, commentable: null, gone: [] }], diffs.length));
  }
  if (repoId === PAYOUT && branch !== null && branch !== "main" && (sectionId === null || sectionId === "overview" || sectionId === "commits" || sectionId === "diff")) {
    return branchView(branch, sectionId);
  }
  if (sectionId === "branches") {
    const rows: ViewRow[] = [
      { entity_id: `${repoId}@refs/heads/main`, title: "main", meta: "5d77e1a2c3 · 1 day ago", tone: "muted", chip: "default", url: null },
      ...(repoId === PAYOUT
        ? [...PAYOUT_BRANCHES, ...BULK_BRANCHES].map(({ name, tip }) => ({
            entity_id: `${PAYOUT}@refs/heads/${name}`,
            title: name,
            meta: `${tip[0].slice(0, 10)} · ${age(tip[3])}`,
            tone: null,
            chip: null,
            url: null,
          }))
        : []),
    ];
    return one(section("branches", "Branches", [{ type: "entities", title: null, rows }], rows.length));
  }
  if (sectionId === "commits") {
    const rows =
      repoId === PAYOUT && branch === FEATURE
        ? PAYOUT_COMMITS.map(([sha, subject, author, at]) => commitRow(payoutCommitId(sha), sha, subject, author, at))
        : branch !== null && branch !== "main"
          ? []
          : MAIN_COMMITS.map(([sha, subject, author, at]) => commitRow(null, sha, subject, author, at));
    return one(section("commits", "Commits", [{ type: "entities", title: null, rows }]));
  }
  if (sectionId !== null && (sectionId === "files" || sectionId.startsWith("files:") || sectionId.startsWith("file:"))) {
    return one(treeSection(files, sectionId, ref));
  }
  return { sections: [], actions: [] };
}
