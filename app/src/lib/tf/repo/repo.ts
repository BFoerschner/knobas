/**
 * The repository page's pure half (#838, R-8.3 to R-8.5, R-8.7, R-8.10):
 * what an address names, which branches the dropdown offers, which paths the
 * local checkout changed, and what the header and the lists say.
 *
 * The page reads a repository through the source's own view (`entity_view`,
 * ADR-0020): its branches, its tree and files at a branch, its commits and a
 * commit's diff, all read live, so a repository nobody cloned reads the same
 * as one on this disk. Section ids are the source's and never taken apart.
 */
import type {
  CheckoutView,
  ChoiceOption,
  CodeFacts,
  DiffFile,
  DiffRead,
  EntityDetail,
  EntityPage,
  EntityRow,
  SourceView,
  TreeEntry,
  ViewAction,
  ViewRow,
  ViewSection,
  RefusedWrite,
  WriteAsk,
} from "../../ipc/entity";
import type { WriteSource } from "../../shell/queued-toast";
import { clockLabel } from "../../time/format";
import { checkoutLine, prNumber } from "../code/code";
import type { DrawnFile } from "../diff/diff";
import { matchesAll } from "../filter/filter";

/**
 * The header's first line (R-8.3), prototype-6's `repoHead`: the declared
 * teams and language (#973, `code_facts`), then the last push. What is not
 * declared is left out rather than guessed.
 */
export function headLine(
  facts: Pick<CodeFacts, "teams" | "language"> | undefined,
  pushedAt: string | null,
  now: Date,
): string {
  const lead = [...(facts?.teams ?? []), ...(facts?.language ? [facts.language] : [])];
  const pushed = clockLabel(pushedAt, now);
  return lead.length > 0 ? `${lead.join(", ")}, last push ${pushed}.` : `Last push ${pushed}.`;
}

/**
 * The repository page's echoes (#839, #845, #2105; `tf/queued.svelte.ts`),
 * all on the repository's entity: a branch being made (`MADE`, its text the
 * name) or deleted (`DELETED`), drawn on *Branches* until its listing holds
 * it or no longer does; a file whose commit is on its way (`commitKind`, its
 * text the message, its base the blob it was made against), drawn on the
 * file until the source holds a newer blob.
 */
export const MADE = "branch";
export const DELETED = "delete";
export const commitKind = (branch: string, path: string) => `commit ${branch}\u0000${path}`;
/** The surfaces they are tagged with, whose re-reads settle them. */
export const REPO_BRANCHES = "repo-branches";
export const REPO_FILES = "repo-files";

/**
 * The page's tabs, in the order of the approved mockup (#2252, #2257): the
 * repository at a ref (Files, Commits, Branches, Compare), then work in
 * flight (Pull requests, Local changes); `group` is which, and the row draws
 * a rule between the two. Compare takes any base and head (#2271), so it is
 * its own tab beside the history rather than under Pull requests. A later
 * ticket that builds one replaces its `NOT_BUILT` entry in `TfRepo.svelte`
 * with an arm drawing its component; the tab row is this list.
 */
export const REPO_TABS = [
  { id: "files", label: "Files", group: "ref" },
  { id: "commits", label: "Commits", group: "ref" },
  { id: "branches", label: "Branches", group: "ref" },
  { id: "compare", label: "Compare", group: "ref" },
  { id: "prs", label: "Pull requests", group: "work" },
  { id: "local", label: "Local changes", group: "work" },
] as const;

/** A tab of the row. */
export type RepoTabId = (typeof REPO_TABS)[number]["id"];

/** What an address can open: a tab, or an edit, which lights Files without being it. */
export type RepoTab = RepoTabId | "edit";

/** Which tab of the row an address lights. */
export function litTab(tab: RepoTab): RepoTabId {
  return tab === "edit" ? "files" : tab;
}

/** What `#/repo/<repo entity id>/...` names. */
export interface RepoRoute {
  /** The repository's entity id, or `null` for a bare `#/repo`. */
  repoId: string | null;
  tab: RepoTab;
  /** Files and edit: the file's path in the tree. */
  path: string | null;
  /** Files: the line a line link (`line/<n>/<path>`, #2251) marks, from 1; `null` for none. */
  line: number | null;
  /** Commits: the commit's entity id, whose diff opens. */
  commit: string | null;
  /** Compare: the branch proposed (#837's `compare/<branch>`). */
  branch: string | null;
}

const TABS = new Set<string>([...REPO_TABS.map((tab) => tab.id), "edit"]);

/**
 * Read what follows `repo` in the address. The route has decoded every
 * segment, so a path's slashes split it and are joined back here; the entity
 * ids were encoded whole and arrive as one segment each.
 */
export function readRepoRoute(rest: readonly string[]): RepoRoute {
  const repoId = rest[0] || null;
  if (rest[1] === LINE) {
    const n = /^\d+$/.test(rest[2] ?? "") ? Number(rest[2]) : 0;
    return { repoId, tab: "files", path: rest.slice(3).join("/") || null, line: n > 0 ? n : null, commit: null, branch: null };
  }
  const tab = (TABS.has(rest[1] ?? "") ? rest[1] : "files") as RepoTab;
  const tail = rest.slice(2).join("/") || null;
  return {
    repoId,
    tab,
    path: tab === "files" || tab === "edit" ? tail : null,
    line: null,
    commit: tab === "commits" ? (rest[2] ?? null) : null,
    branch: tab === "compare" ? tail : null,
  };
}

/** The address of the repository page, a tab of it, or a place inside a tab. */
export function repoHash(repoId: string, tab?: RepoTab, ...tail: string[]): string {
  const parts = [`#/repo/${encodeURIComponent(repoId)}`];
  if (tab !== undefined) parts.push(tab, ...tail);
  return parts.join("/");
}

/** The Files tab with `path` open; each segment encoded, the slashes kept. */
export function fileHash(repoId: string, path: string): string {
  return repoHash(repoId, "files", ...path.split("/").map(encodeURIComponent));
}

/** The segment a line link starts with: not a tab, the Files tab lit. */
const LINE = "line";

/**
 * The Files tab with `path` open at `line` (from 1), marked (#2251). The line
 * comes before the path: a path's segments are any names a tree holds.
 */
export function lineHash(repoId: string, path: string, line: number): string {
  return [repoHash(repoId), LINE, String(line), ...path.split("/").map(encodeURIComponent)].join("/");
}

/** A commit's diff under the Commits tab. */
export function commitHash(repoId: string, commitId: string): string {
  return repoHash(repoId, "commits", encodeURIComponent(commitId));
}

/**
 * The repository a branch or commit is under (#1440): the repository whose
 * name is the row's declared parent, `owner/name` (ADR-0018), in the same
 * source. `null` where the row declares none (the miss, ADR-0007) or the
 * mirror holds no such repository. The id is never taken apart (ADR-0020).
 */
export function repoOfRow(row: Pick<EntityRow, "source_id" | "path">, repos: readonly EntityRow[]): EntityRow | null {
  if (row.path === null) return null;
  return repos.find((repo) => repo.kind === "repo" && repo.source_id === row.source_id && repo.title === row.path) ?? null;
}

/**
 * Every row of a room, read a page at a time until `total` is reached, or
 * until `done` says the rows so far hold what the caller wants (#1825). One
 * read answers at most `LIST_MOST` rows, so a mirror of more rows than that
 * is read on with `offset` instead of dropping its tail unsaid. An empty page
 * ends the walk, so a room that shrank mid-read cannot loop it.
 *
 * `onPage` is handed each page as it lands (#2245), with every row so far in
 * a new array, so a list can draw its first page at once and append the rest
 * behind it; whoever awaits the walk still gets the whole list in one piece.
 */
export async function readAllRows(
  read: (offset: number) => Promise<EntityPage>,
  done: (rows: readonly EntityRow[]) => boolean = () => false,
  onPage?: (rows: readonly EntityRow[], page: EntityPage) => void,
): Promise<EntityRow[]> {
  const rows: EntityRow[] = [];
  for (;;) {
    const page = await read(rows.length);
    rows.push(...page.rows);
    onPage?.([...rows], page);
    if (page.rows.length === 0 || rows.length >= page.total || done(rows)) return rows;
  }
}

/** The two kinds whose addresses send the reader to their repository page (#1440). */
export type UnderRepoKind = "branch" | "commit";

/**
 * Where a branch or commit opens under its repository (#1440): a branch the
 * Branches tab (R-8.1, "a job's branch rows open that tab"), a commit its own
 * page under Commits (R-8.7).
 */
export function underRepoHash(kind: UnderRepoKind, entityId: string, repoId: string): string {
  return kind === "branch" ? repoHash(repoId, "branches") : commitHash(repoId, entityId);
}

/** One branch the dropdown offers. */
export interface BranchOption {
  name: string;
  /**
   * The branch's entity id, which its files and commits are read at; `null`
   * only for the default branch where the mirror does not hold it, which is
   * read at the repository instead.
   */
  entityId: string | null;
  isDefault: boolean;
}

/**
 * The branches the dropdown offers, from the view's *Branches* section, in
 * its order (the default branch first). Only a branch the mirror holds has an
 * id to read at; the view cleared the others' (ADR-0020 rule 7), and the note
 * a cut list ends on has none either. The default branch is offered either
 * way: it is read at the repository.
 */
export function branchOptions(section: ViewSection | null): BranchOption[] {
  const options: BranchOption[] = [];
  for (const block of section?.blocks ?? []) {
    if (block.type !== "entities") continue;
    for (const row of block.rows) {
      const isDefault = row.chip === "default";
      if (row.entity_id === null && !isDefault) continue;
      options.push({ name: row.title, entityId: row.entity_id, isDefault });
    }
  }
  return options;
}

/**
 * The rows of every list the section holds, in the source's order: a muted
 * line a source writes beside its list (`view::muted_line`) is drawn where
 * it put it, as the note a cut list ends on is, never taken for the list
 * (#2065).
 */
export function viewRows(section: ViewSection | null | undefined): ViewRow[] {
  return (section?.blocks ?? []).flatMap((block) => (block.type === "entities" ? block.rows : []));
}

/** The rows that hold every word typed, across sha, title and meta. */
export function visibleViewRows(
  rows: readonly ViewRow[],
  text: string,
  /** Commits' *Author: Mine* (#973): the ids whose declared author is mine, or `null` with the chip off. */
  mine: ReadonlySet<string> | null = null,
): ViewRow[] {
  return rows.filter(
    (row) =>
      (mine === null || (row.entity_id !== null && mine.has(row.entity_id))) &&
      matchesAll(`${row.chip ?? ""} ${row.title} ${row.meta ?? ""}`, text),
  );
}

/**
 * The paths the local checkout changed: unstaged, staged (a rename under its
 * new path) and untracked files. An untracked directory (`scratch/`) marks no
 * file, since the tree does not list what is inside it as changed.
 */
export function changedPaths(read: DiffRead | null): Set<string> {
  const paths = new Set<string>();
  if (read === null || read.status !== "read") return paths;
  for (const file of [...read.unstaged.files, ...read.staged.files]) paths.add(file.path);
  for (const path of read.untracked) if (!path.endsWith("/")) paths.add(path);
  return paths;
}

/** How a change reads after its path on *Local changes* (#847): "modified, not staged". */
const CHANGE_WORD: Record<DiffFile["change"], string> = {
  added: "added",
  modified: "modified",
  deleted: "deleted",
  renamed: "renamed",
  binary: "modified",
};

/**
 * *Local changes*' files (#847, R-8.9): each unstaged file marked "modified,
 * not staged", each staged one "modified, staged", in that order, so the
 * one diff block draws them as a pull request's.
 */
export function localFiles(read: DiffRead | null): DrawnFile[] {
  if (read === null || read.status !== "read") return [];
  // A label, not the path: the path's name picks the highlighting's language.
  const marked = (file: DiffFile, how: string): DrawnFile => ({
    ...file,
    label: `${file.path} (${CHANGE_WORD[file.change]}, ${how})`,
  });
  return [
    ...read.unstaged.files.map((file) => marked(file, "not staged")),
    ...read.staged.files.map((file) => marked(file, "staged")),
  ];
}

/** A file's lines, without the empty one a final newline would add. */
export function fileLines(text: string): string[] {
  const lines = text.split("\n");
  if (lines.length > 1 && lines[lines.length - 1] === "") lines.pop();
  return lines;
}

/** A place in a file's text: a line (from 0) and a character in it. */
export interface TextPlace {
  line: number;
  offset: number;
}

/**
 * The text between two places of a file's lines, in either order, its lines
 * joined by newlines (#2251): what a selection across the file viewer copies,
 * whether or not the lines between are drawn.
 */
export function selectedText(lines: readonly string[], a: TextPlace, b: TextPlace): string {
  const [from, to] = a.line < b.line || (a.line === b.line && a.offset <= b.offset) ? [a, b] : [b, a];
  if (from.line === to.line) return (lines[from.line] ?? "").slice(from.offset, to.offset);
  const out = [(lines[from.line] ?? "").slice(from.offset)];
  for (let n = from.line + 1; n < to.line; n += 1) out.push(lines[n] ?? "");
  out.push((lines[to.line] ?? "").slice(0, to.offset));
  return out.join("\n");
}

/** The directories above a path, outermost first: `src`, `src/sepa`. */
export function ancestors(path: string): string[] {
  const parts = path.split("/").slice(0, -1);
  return parts.map((_, at) => parts.slice(0, at + 1).join("/"));
}

/**
 * The sections a link to `path` is likely read through, asked before the
 * listings above it have named them (#2244, spec #2234 step 2): each
 * directory above it (`""` the top) and the file. The ids are Gitea's
 * adapter's (`files`, `files:<dir>`, `file:<path>`), the one tree a source
 * draws today, and only a guess: the section a listing names is the one the
 * tab keeps, and a guess it names too is answered by the read already asked.
 * The shell never takes an id apart (ADR-0020 rule 3).
 */
export function likelySections(path: string): { dirs: Record<string, string>; file: string } {
  const dirs: Record<string, string> = { "": "files" };
  for (const dir of ancestors(path)) dirs[dir] = `files:${dir}`;
  return { dirs, file: `file:${path}` };
}

/**
 * The file that opens when the address names none: the top directory's
 * README, a Markdown one first, as Gitea's own page prefers it.
 */
export function pickDefaultFile(entries: readonly TreeEntry[]): string | null {
  let best: string | null = null;
  for (const entry of entries) {
    if (entry.kind !== "file") continue;
    const lower = entry.name.toLowerCase();
    if (!/^readme(\.[a-z]+)?$/.test(lower)) continue;
    const markdown = /\.(md|markdown|mdown|mkd)$/.test(lower);
    if (best === null || (markdown && !/\.(md|markdown|mdown|mkd)$/.test(best.toLowerCase()))) best = entry.name;
  }
  return best;
}

/**
 * The header's checkout: where it is and how it stands (Code's *Local
 * checkout* words), both `null` with no clone, `null` itself while
 * `entity_checkout` has not answered.
 */
export function checkoutSentence(view: CheckoutView | undefined): { path: string | null; state: string | null } | null {
  if (view === undefined) return null;
  if (view.path === null) return { path: null, state: null };
  const line = checkoutLine(view);
  return { path: view.path, state: line !== null && line.text !== view.path ? line.text : null };
}

/** A commit's short sha from its id's `@<sha>`, the way the source shortens it; `null` without one. */
export function shaOf(commitId: string): string | null {
  const found = /@([0-9a-f]{4,64})$/i.exec(commitId);
  return found ? found[1]!.slice(0, 10) : null;
}

/** The pull requests whose declared parent is this repository, in its source. */
export function repoPrs(rows: readonly EntityRow[], repo: Pick<EntityRow, "source_id" | "title">): EntityRow[] {
  return rows.filter((row) => row.kind === "pr" && row.source_id === repo.source_id && row.path === repo.title);
}

/** The pull requests the *Pull requests* tab's filter keeps. */
export function visibleRepoPrs(rows: readonly EntityRow[], states: readonly string[], text: string): EntityRow[] {
  const wanted = states.map((state) => state.toLowerCase());
  return rows.filter((row) => {
    if (wanted.length > 0 && !wanted.includes(row.state?.text.toLowerCase() ?? "")) return false;
    return matchesAll(`${prNumber(row.entity_id) ?? ""} ${row.title}`, text);
  });
}

/** The edit of a file (#845): the Files tab's file, turned into an editor. */
export function editHash(repoId: string, path: string): string {
  return repoHash(repoId, "edit", ...path.split("/").map(encodeURIComponent));
}

/** The compare page of a branch (#840, #837's `compare/<branch>`), the name one segment. */
export function compareHash(repoId: string, branch: string): string {
  return repoHash(repoId, "compare", encodeURIComponent(branch));
}

/**
 * Where *New branch from here…* starts, from the commit's own view: its
 * toolbar's *New branch…* (`create_branch`), whose `from_ref` the source fills
 * with the commit's whole sha as the value and its short sha as the label
 * (#707). `null` where the view offers none; `disabled` is the source's reason
 * (a mirror) where it offers one it would refuse.
 */
export function branchFrom(view: SourceView | null): { from: ChoiceOption; disabled: string | null } | null {
  const action = toolbarAction(view, "create_branch");
  const field = action?.fields.find((candidate) => candidate.name === "from_ref");
  const from = field?.kind.type === "choice" ? field.kind.options[0] : undefined;
  return action && from ? { from, disabled: action.disabled } : null;
}

/**
 * The asks the repository page makes of `refused_writes` (#2241, #2243):
 * *New branch…* in the repository and *Commit change…* onto the branch the
 * tabs read at, together, so the source probes the repository once. Whether
 * either is refused is the source's answer, kept by the app and asked again
 * behind it (ADR-0032); the shell never reads protection or grants itself.
 */
export function repoAsks(repoId: string, at: string): WriteAsk[] {
  return [
    { entity: repoId, op: "create_branch" },
    { entity: at, op: "commit_file" },
  ];
}

/** What `refused_writes` answered for {@link repoAsks}: each write's reason, `null` where it is taken. */
export function repoRefusals(
  refused: readonly RefusedWrite[],
  repoId: string,
  at: string,
): { branch: string | null; commit: string | null } {
  const reason = (entity: string, op: string) =>
    refused.find((write) => write.entity === entity && write.op === op)?.reason ?? null;
  return { branch: reason(repoId, "create_branch"), commit: reason(at, "commit_file") };
}

/**
 * The repository's default branch as the mirror holds it (#2243): the
 * repository row's declared `default_branch`, so the page draws its tabs
 * before the live branch list answers. `null` where the row names none, and
 * the page waits for the list.
 *
 * A payload read outside the adapter (ADR-0007), in this one statement: the
 * code host's declared `default_branch` path covers a branch's row, not a
 * repository's, and no DTO field carries it yet. It misses, never guesses:
 * an absent or non-text key is `null`, the page waiting for the list as
 * before #2243 (pinned in `repo.test.ts` and `TfRepo.test`). A stale name only
 * labels the selector until the list answers: on the default branch the tabs
 * read through the repository entity, whose source names its own default.
 */
export function mirrorDefault(detail: EntityDetail | null): string | null {
  const payload = detail?.payload;
  if (payload === null || typeof payload !== "object") return null;
  const named = (payload as Record<string, unknown>)["default_branch"];
  return typeof named === "string" && named !== "" ? named : null;
}

/** The view's toolbar action for `writeOp`, where it offers one. */
function toolbarAction(view: SourceView | null, writeOp: string): ViewAction | undefined {
  return view?.actions.find((candidate) => candidate.write_op === writeOp);
}

/**
 * Where a branch stands, read off its source's own view (`entity_view` of the
 * branch, `knobas-source-gitea`'s `view/branch.rs`): ahead and behind the
 * default branch, the pull request open from it, what it carries, and which
 * of its writes the source offers right now.
 *
 * Whether *Open PR* and *Delete…* are offered is the source's answer, its
 * toolbar's actions (ADR-0020 rule 4), never this reading of a fact. Ahead and
 * behind are drawn, and *merged* is how a branch with nothing ahead is drawn.
 */
export interface BranchStanding {
  /** Commits it carries over the default branch; `null` for that branch itself. */
  ahead: number | null;
  /** Commits the default branch has that it lacks; `null` for that branch itself. */
  behind: number | null;
  /** Nothing on it that the default branch lacks. */
  merged: boolean;
  /** The pull request open from it, as the overview lists it. */
  pr: { entityId: string | null; number: string | null; state: string | null; tone: string | null } | null;
  /** The source offers a pull request from it now (*Open pull request…*, enabled). */
  canOpenPr: boolean;
  /** Why the source draws that pull request disabled (nothing ahead, a token that cannot write, #2030), in its words. */
  prBlocked: string | null;
  /** The source offers a delete at all: never for the default branch. */
  deletable: boolean;
  /** Why the delete cannot be taken now (a protected branch), in the source's words. */
  deleteBlocked: string | null;
  /** What it carries over the default branch, as its *Commits* lists it. */
  commits: ViewRow[];
  /** How many files its diff against the default branch touches; `null` where it has none. */
  files: number | null;
}

function count(value: string | undefined): number | null {
  return value !== undefined && /^\d+$/.test(value.trim()) ? Number(value.trim()) : null;
}

/** A branch's standing from its whole view; an empty one where the view holds nothing. */
export function branchStanding(view: SourceView | null): BranchStanding {
  const section = (id: string) => view?.sections.find((candidate) => candidate.id === id);
  const overview = section("overview")?.blocks ?? [];
  const facts = new Map<string, string>();
  let pr: BranchStanding["pr"] = null;
  for (const block of overview) {
    if (block.type === "facts") for (const fact of block.facts) facts.set(fact.label, fact.value);
    if (block.type === "entities" && pr === null) {
      const row = block.rows.find((candidate) => candidate.entity_id !== null && prNumber(candidate.entity_id) !== null);
      if (row) pr = { entityId: row.entity_id, number: prNumber(row.entity_id!), state: row.chip, tone: row.tone };
    }
  }
  const ahead = count(facts.get("Ahead"));
  const actions = view?.actions ?? [];
  const offer = actions.find((candidate) => candidate.write_op === "create_pull_request");
  const remove = actions.find((candidate) => candidate.write_op === "delete_branch");
  const diff = section("diff");
  return {
    ahead,
    behind: count(facts.get("Behind")),
    merged: ahead === 0,
    pr,
    canOpenPr: offer !== undefined && offer.disabled === null,
    prBlocked: offer?.disabled ?? null,
    deletable: remove !== undefined,
    deleteBlocked: remove?.disabled ?? null,
    commits: viewRows(section("commits")),
    files: diff?.count ?? null,
  };
}

/** One row of the *Branches* tab. */
export interface BranchRow {
  name: string;
  /** Its entity id, `null` where the mirror does not hold it yet. */
  entityId: string | null;
  isDefault: boolean;
  /** The listing's line: its tip and when it was committed. */
  meta: string | null;
  /** `null` until its view answers, and for a branch with no id to read at. */
  standing: BranchStanding | null;
  /** Its tip's author as the source spells the account (`code_facts`, #986), or `null`. */
  author: string | null;
  /** Whether that author is mine. */
  mine: boolean;
}

/** The rows the *Branches* filter keeps: every word typed in the name, and each chip set. */
export function visibleBranches(rows: readonly BranchRow[], show: readonly string[], text: string): BranchRow[] {
  return rows.filter((row) => {
    if (show.includes("Mine") && !row.mine) return false;
    if (show.includes("No PR") && (row.isDefault || row.standing === null || row.standing.pr !== null)) return false;
    if (show.includes("Merged") && !row.standing?.merged) return false;
    return matchesAll(row.name, text);
  });
}

/** The ticket key a branch name holds (`feature/PAY-231-sepa-retry`), or `null`. */
export function keyIn(name: string): string | null {
  const found = /(?:^|[^A-Za-z0-9])([A-Z][A-Z0-9]+-\d+)(?![0-9])/.exec(name);
  return found ? found[1]! : null;
}

/** The ticket a pull request is proposed from: its key, summary and description. */
export interface ProposedFrom {
  key: string;
  summary: string;
  description: string;
}

/**
 * The compare page's title and description (R-8.11): `<key>: <summary>` and
 * the ticket's description ending `Closes <key>.` where the branch holds a
 * key the mirror has; else the branch's last segment and nothing.
 */
export function proposal(head: string, ticket: ProposedFrom | null): { title: string; body: string } {
  if (ticket === null) return { title: head.slice(head.lastIndexOf("/") + 1), body: "" };
  const description = ticket.description.trim();
  return {
    title: `${ticket.key}: ${ticket.summary}`,
    body: description === "" ? `Closes ${ticket.key}.` : `${description}\n\nCloses ${ticket.key}.`,
  };
}

/** *New branch…*'s name, prefilled with the job's key (R-8.8). */
export function newBranchName(job: string | null): string {
  return job === null ? "feature/" : `feature/${job}-`;
}

/** The commit dialog's message and new branch, prefilled with the job's key (R-8.6). */
export function commitPrefill(job: string | null, path: string): { message: string; branch: string } {
  const file = path.slice(path.lastIndexOf("/") + 1);
  const stem = file.includes(".") && !file.startsWith(".") ? file.slice(0, file.indexOf(".")) : file;
  return {
    message: job === null ? `update ${file}` : `${job}: update ${file}`,
    branch: job === null ? `fix/${stem}` : `fix/${job}-${stem}`,
  };
}

/** What the repository page calls its source in a sentence: the display name, else the id, else a role word ("the code host", lower case; a sentence that opens with it spells its own capital). */
export function sourceName(source: WriteSource | null | undefined): string {
  return source?.display_name || source?.id || "the code host";
}
