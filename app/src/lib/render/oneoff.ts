/**
 * Commands (ADR-0060), the pure half the window keeps: how a run's line is drawn in pieces, what its ending says, its
 * findings grouped by file, what *Copy* copies, and the TypeScript twins of `knobas_core::checkout::oneoff`'s
 * `kind_of`, `check` and `fill` that the menus and *It will run* read (held to the Rust by `oneoff-twin.json`).
 */
import { secondsLabel } from "../time/format";
import type { OneOff, OneOffEnded, OneOffFinding, OneOffKind, OneOffLine, OneOffPlace, OneOffStdin, OneOffStyle, OneOffTarget, OneOffUrl } from "../ipc/oneoff";

/** One piece of a line: its text, its classes, and the place or address it is a link to. */
export interface Piece {
  text: string;
  cls: string;
  place: OneOffPlace | null;
  url: OneOffUrl | null;
}

/** A style as the `tf-oo-` classes that draw it: a theme colours names, never a value a program printed. */
export function classOf(style: OneOffStyle): string {
  const out: string[] = [];
  if (style.fg) out.push(`tf-oo-${style.fg}`);
  if (style.bg) out.push(`tf-oo-on-${style.bg}`);
  if (style.bold) out.push("tf-oo-bold");
  if (style.dim) out.push("tf-oo-dim");
  if (style.italic) out.push("tf-oo-it");
  if (style.underline) out.push("tf-oo-ul");
  return out.join(" ");
}

/** Line `line`'s characters `from` to `to` (UTF-16 units) as pieces, cut where a run, a place or an address starts or ends. */
export function pieces(line: OneOffLine, from = 0, to = line.text.length): Piece[] {
  if (to <= from) return [];
  const runs = line.runs ?? [];
  const places = line.places ?? [];
  const urls = line.urls ?? [];
  const cuts = new Set([from, to]);
  const cut = (a: number, b: number) => {
    if (a > from && a < to) cuts.add(a);
    if (b > from && b < to) cuts.add(b);
  };
  for (const r of runs) cut(r.from, r.to);
  for (const p of places) cut(p.from, p.to);
  for (const u of urls) cut(u.from, u.to);
  const at = [...cuts].sort((x, y) => x - y);
  const out: Piece[] = [];
  for (let k = 0; k < at.length - 1; k++) {
    const a = at[k] as number;
    const b = at[k + 1] as number;
    const cls = runs
      .filter((r) => r.from <= a && r.to >= b)
      .map((r) => classOf(r.style))
      .filter((c) => c !== "")
      .join(" ");
    out.push({
      text: line.text.slice(a, b),
      cls,
      place: places.find((p) => p.from <= a && p.to >= b) ?? null,
      url: urls.find((u) => u.from <= a && u.to >= b) ?? null,
    });
  }
  return out;
}

/** What the run line says about how a run stands. */
export interface State {
  tone: "ok" | "err" | "warn" | "info" | "muted";
  /** The words before the exit code, and the code as a figure where there is one. */
  words: string;
  code: number | null;
}

/** How `ended` reads on the run line; `null` is a run still going. */
export function stateOf(ended: OneOffEnded | null): State {
  if (ended === null) return { tone: "info", words: "Running", code: null };
  switch (ended.how) {
    case "exit":
      return { tone: ended.code === 0 ? "ok" : "err", words: "Exited with", code: ended.code };
    case "signal":
      return { tone: "err", words: "Ended by signal", code: ended.signal };
    case "timed_out":
      return { tone: "warn", words: "Stopped at its timeout", code: null };
    case "stopped":
      return { tone: "muted", words: "Stopped by you", code: null };
    case "not_started":
      return { tone: "err", words: "Did not start", code: null };
  }
}

/** How a run ended in a few words, for a menu row, a toast and *Last runs*: `exit 1`, `stopped`. */
export function endedShort(ended: OneOffEnded | null | undefined): string {
  if (ended === null || ended === undefined) return "running";
  switch (ended.how) {
    case "exit":
      return `exit ${ended.code}`;
    case "signal":
      return `signal ${ended.signal}`;
    case "timed_out":
      return "timed out";
    case "stopped":
      return "stopped";
    case "not_started":
      return "did not start";
  }
}

/** `0:07`, a running clock. */
export function clock(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/** `2.4s`, `1m 12s`: how long a run took, in the app's one spelling of a length, tenths under ten seconds. */
export function took(ms: number): string {
  return secondsLabel(ms < 10_000 ? Math.round(ms / 100) / 10 : Math.round(ms / 1000));
}

/** `767 B`, `1.2 MB`. */
export function size(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** `1 line`, `10 lines`. */
export function many(n: number, one: string, more: string): string {
  return `${n.toLocaleString("en")} ${n === 1 ? one : more}`;
}

/** `14 min ago`, `yesterday`: how long ago a run started. */
export function ago(at: number, now: number): string {
  const s = Math.max(0, Math.round((now - at) / 1000));
  if (s < 45) return "just now";
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h} h ago`;
  const d = Math.round(h / 24);
  return d === 1 ? "yesterday" : `${d} days ago`;
}

/** An argument vector as one line a person can read and paste: a word with a space or a quote in it is quoted. */
export function shown(argv: readonly string[]): string {
  return argv.map((a) => (a === "" || /[\s"']/.test(a) ? `"${a.replace(/"/g, '\\"')}"` : a)).join(" ");
}

/** One finding with its place, for *Problems*. */
export interface Problem extends OneOffFinding {
  path: string;
  line: number;
  col: number | null;
  /** The output line it was read from. */
  n: number;
}

/** The findings in `lines`, a file a group, in the order the program printed them. */
export function problems(lines: readonly OneOffLine[]): { path: string; list: Problem[] }[] {
  const groups = new Map<string, Problem[]>();
  for (const l of lines) {
    const place = l.finding ? (l.places ?? []).find((p) => p.line !== null) : undefined;
    if (!l.finding || place === undefined || place.line === null) continue;
    const list = groups.get(place.path) ?? [];
    list.push({ ...l.finding, path: place.path, line: place.line, col: place.col, n: l.n });
    groups.set(place.path, list);
  }
  return [...groups].map(([path, list]) => ({ path, list }));
}

/** What *Copy* copies of the output: the text without its colour codes, a line each. */
export function outputText(lines: readonly OneOffLine[]): string {
  return lines.map((l) => l.text).join("\n") + (lines.length > 0 ? "\n" : "");
}

/** What *Copy* copies of *Problems*: `path:line:col: severity: words`, a line each. */
export function problemsText(groups: readonly { path: string; list: Problem[] }[]): string {
  return groups
    .flatMap((g) => g.list.map((p) => `${p.path}:${p.line}${p.col !== null ? `:${p.col}` : ""}: ${p.severity || "note"}: ${p.said}${p.rule ? ` [${p.rule}]` : ""}`))
    .join("\n");
}

/** What a target reads as in the dialog's sub line and a menu. */
export function targetLabel(target: OneOffTarget, repo: string): string {
  switch (target.kind) {
    case "file":
      return `${target.path} in ${repo}`;
    case "folder":
      return target.path === "" ? repo : `${target.path}/ in ${repo}`;
    case "repository":
      return repo;
    case "commit":
      return `commit ${target.sha.slice(0, 7)} of ${repo}`;
    case "range":
      return `${target.base}..${target.head} of ${repo}`;
    case "files":
      return `${many(target.paths.length, "file", "files")} of ${repo}`;
  }
}

// -- the twins of knobas_core::checkout::oneoff --------------------------------------------------------------------

/** Every placeholder a command may hold, in the order the page offers them. */
export const PLACEHOLDERS = ["file", "relfile", "files", "dir", "reldir", "root", "name", "line", "endline", "sha", "base", "head", "branch", "out", "outdir"] as const;
const PATHS = new Set(["file", "relfile", "files", "dir", "reldir", "root", "out", "outdir"]);
const SHELLS = new Set(["sh", "bash", "zsh", "dash", "fish", "cmd", "cmd.exe", "powershell", "powershell.exe", "pwsh", "pwsh.exe"]);
const SCRIPT_FLAGS = new Set(["-c", "-lc", "-ic", "/c", "/k", "-command", "-encodedcommand"]);

/** Quotes group words and nothing else is special (`knobas_core::checkout::open::words`); `null` for an unclosed quote. */
export function words(command: string): string[] | null {
  const out: string[] = [];
  let cur = "";
  let started = false;
  let quote: string | null = null;
  for (const ch of command) {
    if (quote !== null) {
      if (ch === quote) quote = null;
      else cur += ch;
      continue;
    }
    if (ch === '"' || ch === "'") {
      quote = ch;
      started = true;
      continue;
    }
    if (/\s/.test(ch)) {
      if (started) {
        out.push(cur);
        cur = "";
        started = false;
      }
      continue;
    }
    cur += ch;
    started = true;
  }
  if (quote !== null) return null;
  if (started) out.push(cur);
  return out;
}

function namesIn(word: string): string[] | null {
  const out: string[] = [];
  let rest = word;
  for (;;) {
    const open = rest.indexOf("{");
    if (open < 0) return out;
    const close = rest.indexOf("}", open);
    if (close < 0) return null;
    out.push(rest.slice(open + 1, close));
    rest = rest.slice(close + 1);
  }
}

const programName = (word: string) => (word.split(/[\\/]/).pop() ?? word).toLowerCase();

/** What a command is offered on, read from its placeholders; `null` where it does not split. */
export function kindOf(command: string, stdin: OneOffStdin = "none"): OneOffKind | null {
  const split = words(command);
  if (split === null) return null;
  const held = new Set<string>();
  for (const w of split) for (const n of namesIn(w) ?? []) held.add(n);
  const has = (...names: string[]) => names.some((n) => held.has(n));
  if (stdin !== "none" || has("file", "relfile", "files", "line", "endline")) return "file";
  if (has("dir", "reldir")) return "folder";
  if (has("base", "head")) return "range";
  if (has("sha")) return "commit";
  return "repository";
}

/** What Save refuses, in the app's own sentences; `null` where the command can be saved. */
export function check(command: string, stdin: OneOffStdin = "none"): string | null {
  const split = words(command);
  if (split === null) return "the command has a quote that is never closed";
  if (split.length === 0) return "the command is empty, so there is no program to run";
  let outs = 0;
  let pathHeld = false;
  for (let i = 0; i < split.length; i++) {
    const word = split[i] as string;
    const names = namesIn(word);
    if (names === null) return "the command has a '{' that is never closed by a '}'";
    for (const n of names) {
      if (n === "path") return "{path} is not one of a command's: {root} is the repository, {dir} the folder and {file} the file";
      if (!(PLACEHOLDERS as readonly string[]).includes(n)) return `{${n}} is not something knobas fills in: a command may take ${PLACEHOLDERS.map((p) => `{${p}}`).join(", ")}`;
      if (n === "files" && word !== "{files}") return "{files} becomes an argument a file, so it stands alone as a word";
      if (n === "out" || n === "outdir") outs++;
      if (PATHS.has(n)) {
        pathHeld = true;
        if (/\s/.test(word)) return `{${n}} stands inside a quoted script, where a file's name would be read as shell text. Pass it after the script and read it as "$1": sh -c 'eslint "$1" | head -40' _ {${n}}`;
        const before = split[i - 1];
        if (i > 0 && before !== undefined && SCRIPT_FLAGS.has(before.toLowerCase()) && split.slice(0, i - 1).some((w) => SHELLS.has(programName(w)))) {
          return `{${n}} is the script of ${split[0]} ${before}: a file's name would be run as shell text. Pass it after the script and read it as "$1": sh -c 'eslint "$1"' _ {${n}}`;
        }
      }
    }
  }
  if (outs > 1) return "a command writes to {out} or to {outdir}, one of them, once";
  const program = programName(split[0] as string);
  if ((program === "wsl" || program === "wsl.exe") && pathHeld && !split.some((w) => w === "-e" || w === "--exec")) return "wsl hands its words to a shell unless -e stands before the program: write wsl -e ruff check {file}";
  if (stdin !== "none" && split.length < 2 && SHELLS.has(program)) return `${split[0]} with no script of its own would run what it reads on stdin as a script`;
  return null;
}

/** Where a command is filled, as the page knows it. */
export interface FillAt {
  /** The checkout, as the page knows it; the repository's name stands in where it does not. */
  root: string;
  sep?: string;
  file?: string | null;
  files?: readonly string[];
  dir?: string | null;
  line?: number | null;
  endline?: number | null;
  sha?: string | null;
  base?: string | null;
  head?: string | null;
  branch?: string | null;
}

/** The argument vector `command` becomes at `at`, as the backend will make it, or why it runs nothing. */
export function fill(command: string, at: FillAt): { argv: string[] } | { error: string } {
  const why = check(command);
  if (why !== null) return { error: why };
  const split = words(command) ?? [];
  const sep = at.sep ?? "/";
  const abs = (rel: string) => (rel === "" || rel === "." ? at.root : `${at.root}${sep}${rel.split("/").join(sep)}`);
  const value = (n: string): string | null => {
    switch (n) {
      case "root":
        return at.root;
      case "name":
        return at.root.split(/[\\/]/).pop() ?? "";
      case "file":
        return at.file == null ? null : abs(at.file);
      case "relfile":
        return at.file ?? null;
      case "dir":
        return at.dir == null ? null : abs(at.dir);
      case "reldir":
        return at.dir == null ? null : at.dir === "" ? "." : at.dir;
      case "line":
        return String(at.line ?? 1);
      case "endline":
        return String(at.endline ?? at.line ?? 1);
      case "sha":
        return at.sha ?? null;
      case "base":
        return at.base ?? null;
      case "head":
        return at.head ?? null;
      case "branch":
        return at.branch ?? null;
      case "out":
        return `…${sep}out${sep}output`;
      case "outdir":
        return `…${sep}out`;
      default:
        return null;
    }
  };
  const argv: string[] = [];
  for (const w of split) {
    if (w === "{files}") {
      const files = at.files && at.files.length > 0 ? at.files : at.file != null ? [at.file] : [];
      if (files.length === 0) return { error: "there is no {files} here" };
      argv.push(...files.map(abs));
      continue;
    }
    let text = "";
    let rest = w;
    for (;;) {
      const open = rest.indexOf("{");
      if (open < 0) {
        text += rest;
        break;
      }
      const close = rest.indexOf("}", open);
      const n = rest.slice(open + 1, close);
      const v = value(n);
      if (v === null) return { error: `there is no {${n}} here` };
      if ((n === "relfile" || n === "reldir") && v.startsWith("-")) return { error: `{${n}} would be '${v}', which a program would read as an option: use {${n === "relfile" ? "file" : "dir"}}` };
      text += rest.slice(0, open) + v;
      rest = rest.slice(close + 1);
    }
    argv.push(text);
  }
  return { argv };
}

/** A glob of Settings, ignoring case: `*` any run of characters (`/` too), `?` one; with a `/` against the whole path. */
export function glob(pattern: string, path: string): boolean {
  const subject = pattern.includes("/") ? path : path.slice(path.lastIndexOf("/") + 1);
  const re = new RegExp(`^${[...pattern].map((c) => (c === "*" ? ".*" : c === "?" ? "." : c.replace(/[.+^${}()|[\]\\]/g, "\\$&"))).join("")}$`, "i");
  return re.test(subject);
}

/** What the page knows of the repository a command would run in. */
export interface Facts {
  repo: string;
  /** The names at its top, where the listing has been read. */
  top: readonly string[] | null;
}

function inRepository(c: OneOff, facts: Facts): boolean {
  const repo = c.repos.length === 0 || c.repos.includes(facts.repo);
  const holds = c.repo_holds.length === 0 || facts.top === null || c.repo_holds.some((p) => facts.top!.some((name) => glob(p, name)));
  return repo && holds;
}

/** Whether `c` takes the file at `path`. */
export function takesFile(c: OneOff, path: string, facts: Facts): boolean {
  return kindOf(c.command, c.stdin) === "file" && (c.files.length === 0 || c.files.some((p) => glob(p, path))) && inRepository(c, facts);
}

/** Whether `c` takes the folder at `path` (`""` the top), whose own files are `own` where they have been read. */
export function takesFolder(c: OneOff, path: string, own: readonly string[] | null, facts: Facts): boolean {
  return (
    kindOf(c.command, c.stdin) === "folder" &&
    (c.folders.length === 0 || c.folders.some((p) => glob(p, path))) &&
    (c.has.length === 0 || own === null || c.has.some((p) => own.some((name) => glob(p, name)))) &&
    inRepository(c, facts)
  );
}

/** Whether `c` takes the repository itself. */
export function takesRepository(c: OneOff, facts: Facts): boolean {
  return kindOf(c.command, c.stdin) === "repository" && inRepository(c, facts);
}
