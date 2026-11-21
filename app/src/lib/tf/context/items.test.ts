/** What each kind of thing's right-click menu offers (`items.ts`), in its order. */
import { expect, test } from "vitest";

import type { CheckoutSettings, CheckoutView, EntityDetail } from "../../ipc/entity";
import { menuFor, searchable, type Entry, type Item, type MenuContext, type MenuFacts } from "./items";
import type { Thing } from "./thing";

const APPS: CheckoutSettings = {
  clones_root: "/c",
  git: { status: "found", path: "/usr/bin/git" },
  default_application: "vscode",
  applications: [
    { id: "terminal", name: "Terminal", command: "open -a Terminal {path}", file_command: null },
    { id: "vscode", name: "VS Code", command: "code {path}", file_command: "code --goto {file}:{line}", commit_command: null },
    { id: "fork", name: "Fork", command: "fork {path}", file_command: null, commit_command: "fork --commit {sha} {path}" },
  ],
};

function ctx(over: Partial<MenuContext> = {}): MenuContext {
  return {
    job: null,
    canTime: true,
    apps: APPS,
    sourceName: (id) => ({ jira: "Jira", gitea: "Gitea", tc: "TeamCity" })[id] ?? null,
    mirrored: (url) => url.startsWith("https://jira.example/"),
    ...over,
  };
}

const ids = (entries: Entry[]) => entries.map((entry) => ("rule" in entry ? "|" : entry.id));
const item = (entries: Entry[], id: string) => entries.find((entry): entry is Item => !("rule" in entry) && entry.id === id)!;
const view = (path: string | null): CheckoutView => ({ repo_entity_id: "gitea:t/pay", repo_url: null, path, found_by: path ? "scan" : "nothing", clones_root: "/c", clone_command: null, tree: null });

const TICKET: Thing = { kind: "ticket", id: "jira:PAY-231", hash: "#/ticket/jira%3APAY-231" };

test("a ticket: Open, the sheet's own, its source, then the copies; Open first so no write is under the pointer", () => {
  const entries = menuFor(TICKET, null, ctx({ job: { entityId: "jira:PAY-9", label: "PAY-9" } }));
  expect(ids(entries)).toEqual(["open", "work", "timer", "link", "|", "web", "|", "copy-key", "copy-key-title", "copy-link"]);
  expect(item(entries, "work").label).toBe("Work on this");
  expect(item(entries, "link").label).toBe("Link to PAY-9");
  expect(item(entries, "web").label).toBe("Open in Jira");
  expect(item(entries, "copy-key")).toMatchObject({ hint: "PAY-231", act: { do: "copy", text: "PAY-231" } });
  expect(item(entries, "copy-key-title").act).toEqual({ do: "copy", text: { read: "key-title" }, said: "Copied PAY-231 and its title" });
});

test("the job's own ticket has no link to itself, and no timer where the shell takes none", () => {
  expect(ids(menuFor(TICKET, null, ctx({ job: { entityId: TICKET.id, label: "PAY-231" }, canTime: false })))).not.toContain("link");
  expect(ids(menuFor(TICKET, null, ctx({ canTime: false })))).not.toContain("timer");
});

test("a page's own thing has no Open: the reader is there", () => {
  expect(ids(menuFor(TICKET, null, ctx({ own: true })))[0]).toBe("work");
});

test("a pull request: its pages, never Approve or Merge, which others see and one press doesn't undo", () => {
  const entries = menuFor({ kind: "pr", id: "gitea:t/pay#12", hash: "#/pr/x" }, null, ctx());
  expect(ids(entries)).toEqual(["open", "diff", "talk", "|", "web", "|", "copy-title", "copy-link"]);
  expect(ids(entries)).not.toContain("approve");
  expect(ids(entries)).not.toContain("merge");
});

test("Link to the job ends the first group on every kind, after the menu's own pages", () => {
  const job = { entityId: "jira:PAY-9", label: "PAY-9" };
  expect(ids(menuFor({ kind: "repo", id: "gitea:t/pay", hash: "#/r" }, null, ctx({ job }))).slice(0, 6)).toEqual(["open", "prs", "branches", "commits", "link", "|"]);
});

test("a repository: Open in its default application flat, the rest in Open in…, and its copies", () => {
  const entries = menuFor({ kind: "repo", id: "gitea:t/pay", hash: "#/repo/gitea%3At%2Fpay" }, null, ctx());
  expect(ids(entries)).toEqual(["open", "prs", "branches", "commits", "|", "open-default", "open-in", "web", "|", "copy-name", "copy-full", "copy-link"]);
  expect(item(entries, "open-default")).toMatchObject({ label: "Open in VS Code", act: { do: "app", app: "vscode", at: { kind: "repository" } } });
  const sub = item(entries, "open-in").sub!;
  expect(ids(sub)).toEqual(["app-vscode", "app-terminal", "app-fork", "|", "add-app", "manage-apps"]);
  expect(item(sub, "app-vscode").hint).toBe("default");
  expect(item(sub, "add-app").act).toEqual({ do: "go", hash: "#/settings/checkouts/add/gitea%3At%2Fpay" });
  expect(item(entries, "copy-name").hint).toBe("t/pay");
});

test("a folder: each application opens the folder; a file: the file, with its line where there is one", () => {
  const folder = menuFor({ kind: "folder", repoId: "gitea:t/pay", path: "src/sepa" }, null, ctx());
  expect(ids(folder)).toEqual(["open-default", "open-in", "forge", "|", "copy-path", "copy-full", "copy-link"]);
  expect(item(folder, "open-default").act).toEqual({ do: "app", entityId: "gitea:t/pay", app: "vscode", name: "VS Code", at: { kind: "folder", path: "src/sepa" } });
  expect(item(item(folder, "open-in").sub!, "app-terminal").act).toMatchObject({ at: { kind: "folder", path: "src/sepa" } });

  const line = menuFor({ kind: "line", repoId: "gitea:t/pay", path: "src/sepa/retry.rs", line: 42 }, null, ctx());
  expect(ids(line)).toEqual(["open", "history", "|", "open-default", "open-in", "forge", "|", "copy-path", "copy-full", "copy-link"]);
  expect(item(line, "copy-path")).toMatchObject({ label: "Copy path and line", hint: "src/sepa/retry.rs:42", act: { text: "src/sepa/retry.rs:42" } });
  expect(item(line, "copy-link").label).toBe("Copy link to line");
  expect(item(line, "open-default").act).toMatchObject({ at: { kind: "file", path: "src/sepa/retry.rs", line: 42 } });
  expect(item(line, "add-app")).toBeUndefined();
  expect(item(item(line, "open-in").sub!, "add-app").act).toEqual({ do: "go", hash: "#/settings/checkouts/add/gitea%3At%2Fpay/file/src%2Fsepa%2Fretry.rs" });
});

test("not cloned: Clone stands in the default's slot, and Open in… and the full path say why they can't", () => {
  const thing: Thing = { kind: "folder", repoId: "gitea:t/pay", path: "src" };
  const reading = menuFor(thing, null, ctx());
  const facts: MenuFacts = { checkout: view(null) };
  const cloned = menuFor(thing, null, ctx(), { checkout: view("/c/pay") });
  const none = menuFor(thing, null, ctx(), facts);
  // The rows hold still whatever the read answers: only labels change.
  expect(ids(none)).toEqual(ids(reading).map((id) => (id === "open-default" ? "clone" : id)));
  expect(ids(cloned)).toEqual(ids(reading));
  expect(item(none, "clone")).toMatchObject({ label: "Clone pay", act: { do: "clone", repoId: "gitea:t/pay" } });
  expect(item(none, "open-in").disabled).toBe("Not cloned on this machine");
  expect(item(none, "copy-full").disabled).toBe("Not cloned on this machine");
  expect(item(cloned, "open-in").disabled).toBeNull();
});

test("a commit: its applications are the ones with a commit command, and both hashes copy", () => {
  const sha = "c90d11e4a7b2c90d11e4a7b2c90d11e4a7b2c90d";
  const entries = menuFor({ kind: "commit", id: `gitea:t/pay@${sha}`, repoId: "gitea:t/pay", hash: "#/x" }, null, ctx());
  expect(ids(entries)).toEqual(["open", "|", "open-default", "open-in", "web", "|", "copy-sha", "copy-short", "copy-link"]);
  expect(item(entries, "open-default").label).toBe("Open in Fork");
  expect(ids(item(entries, "open-in").sub!)).toEqual(["app-fork", "|", "add-app", "manage-apps"]);
  expect(item(entries, "copy-sha").act).toMatchObject({ text: sha });
  expect(item(entries, "copy-short")).toMatchObject({ hint: "c90d11e", act: { text: "c90d11e" } });
});

test("a commit of a repository cloned here offers Compare with working tree after Open; elsewhere none (ux26)", () => {
  const sha = "c90d11e4a7b2c90d11e4a7b2c90d11e4a7b2c90d";
  const thing = { kind: "commit", id: `gitea:t/pay@${sha}`, repoId: "gitea:t/pay", hash: "#/x" } as const;
  const cloned = menuFor(thing, null, ctx({ checkout: (id) => id === "gitea:t/pay" }));
  expect(ids(cloned).slice(0, 3)).toEqual(["open", "compare-working", "|"]);
  expect(item(cloned, "compare-working").act).toEqual({ do: "go", hash: `#/diff/gitea%3At%2Fpay/${sha}/~working-tree` });
  expect(ids(menuFor(thing, null, ctx({ checkout: () => false })))).not.toContain("compare-working");
  expect(ids(menuFor(thing, null, ctx()))).not.toContain("compare-working");
});

test("a branch name offers Compare with working tree, the branch as Into (ux26)", () => {
  const entries = menuFor({ kind: "branch-name", repoId: "gitea:t/pay", name: "main" } as never, null, ctx());
  expect(item(entries, "compare-working").act).toEqual({ do: "go", hash: "#/repo/gitea%3At%2Fpay/compare/main...~working-tree" });
});

test("a run: its log and tests, Re-run queued, its page on TeamCity", () => {
  const entries = menuFor({ kind: "run", id: "tc:9", pipelineId: "tc:Pay", hash: "#/build/tc%3APay/tc%3A9" }, null, ctx());
  expect(ids(entries)).toEqual(["open", "log", "tests", "rerun", "|", "web", "|", "copy-link"]);
  expect(item(entries, "rerun").act).toMatchObject({ do: "write", payload: { RerunBuild: { entity: "tc:9" } }, sourceId: "tc" });
  expect(item(entries, "web").label).toBe("Open in TeamCity");
});

test("each Open in names its own source by its display name: two of one kind, and an installed adapter's", () => {
  // Two TeamCity servers and an installed (wasm) CI adapter, each named in Settings › Sources (the build-header pass, 2026-11-20).
  const names: Record<string, string> = { tcwork: "TeamCity (work)", tcoss: "TeamCity (OSS)", acme: "Acme Pipelines", fj: "Forgejo at home" };
  const named = ctx({ sourceName: (id) => names[id] ?? null });
  const run = (source: string) => menuFor({ kind: "run", id: `${source}:9`, pipelineId: `${source}:Pay`, hash: "#/x" }, null, named);
  expect(item(run("tcwork"), "web").label).toBe("Open in TeamCity (work)");
  expect(item(run("tcoss"), "web").label).toBe("Open in TeamCity (OSS)");
  expect(item(run("acme"), "web").label).toBe("Open in Acme Pipelines");
  // Each opens its own item's page: the item's own web address, read as the menu opens.
  expect(item(run("acme"), "web").act).toEqual({ do: "web", url: { read: "web" }, source: "acme" });
  // A file's page on its code host names that host too, not the browser.
  const file = menuFor({ kind: "line", repoId: "fj:t/pay", path: "src/a.rs", line: 3 }, null, named);
  expect(item(file, "forge")).toMatchObject({ label: "Open in Forgejo at home", act: { do: "web", url: { read: "forge" }, source: "fj" } });
  // A source the list does not know: the browser, which is what opens it.
  expect(item(menuFor({ kind: "folder", repoId: "gone:t/pay", path: "src" }, null, named), "forge").label).toBe("Open in browser");
});

test("a monitor offers Acknowledge only where its row shows an open alert", () => {
  expect(ids(menuFor({ kind: "monitor", id: "k:1", hash: "#/m" }, null, ctx()))).not.toContain("ack");
  expect(ids(menuFor({ kind: "monitor", id: "k:1", hash: "#/m", alerting: true }, null, ctx()))).toContain("ack");
});

test("an Inbox line: the page's own writes, Snooze left to the row since it asks", () => {
  const entries = menuFor({ kind: "inbox", key: "k", title: "Down", open: "#/monitors/at/k%3A1", ops: ["rerun_build", "ack", "snooze", "done"], entityId: "tc:9", sourceId: "tc" }, null, ctx());
  expect(ids(entries)).toEqual(["open", "rerun", "ack", "done"]);
});

test("a web link: Open in knobas first only on a source knobas mirrors", () => {
  expect(ids(menuFor({ kind: "web", url: "https://jira.example/browse/PAY-1" }, null, ctx()))).toEqual(["in-knobas", "browser", "|", "copy-link"]);
  expect(ids(menuFor({ kind: "web", url: "https://elsewhere.example/" }, null, ctx()))).toEqual(["browser", "|", "copy-link"]);
});

test("selected words: Copy, Search for short ones, then the thing's own below a rule", () => {
  expect(ids(menuFor(null, "retry", ctx()))).toEqual(["copy-sel", "search-sel"]);
  expect(item(menuFor(null, "retry", ctx()), "search-sel")).toMatchObject({ label: "Search knobas for “retry”", act: { do: "search", text: "retry" } });
  expect(ids(menuFor(null, "a\nb", ctx()))).toEqual(["copy-sel"]);
  expect(ids(menuFor(TICKET, "x", ctx())).slice(0, 4)).toEqual(["copy-sel", "search-sel", "|", "open"]);
  expect(searchable("x".repeat(80))).toBe(true);
  expect(searchable("x".repeat(81))).toBe(false);
});

test("nothing on nothing", () => {
  expect(menuFor(null, null, ctx())).toEqual([]);
});

test("the read only changes labels, never the rows", () => {
  const head = { row: { entity_id: TICKET.id, kind: "ticket", source_id: "jira", title: "Payout CSV", updated_at: null, synced_at: "", path: null, state: null }, web_url: "https://jira.example/browse/PAY-231" } as unknown as EntityDetail;
  expect(ids(menuFor(TICKET, null, ctx(), { head }))).toEqual(ids(menuFor(TICKET, null, ctx())));
});

test("a folder a folder renderer takes offers its drawing by each that takes it, after Open's place (ADR-0044)", () => {
  const thing: Thing = { kind: "folder", repoId: "gitea:t/pay", path: "flows/orders" };
  const asked: [string, string][] = [];
  const menu = menuFor(thing, null, { ...ctx(), renders: (repo, path) => (asked.push([repo, path]), [{ id: "lc", name: "Low code" }, { id: "tree", name: "Tree" }]) });
  expect(asked).toEqual([["gitea:t/pay", "flows/orders"]]);
  expect(ids(menu).slice(0, 3)).toEqual(["render-lc", "render-tree", "|"]);
  expect(item(menu, "render-lc")).toMatchObject({ label: "Render with Low code", act: { do: "render-with", hash: "#/repo/gitea%3At%2Fpay/render/lc/flows/orders", repoId: "gitea:t/pay", folder: "flows/orders", renderer: "lc" } });
  // A file is offered none.
  expect(ids(menuFor({ kind: "file", repoId: "gitea:t/pay", path: "a.md" }, null, { ...ctx(), renders: () => [{ id: "lc", name: "Low code" }] }))).not.toContain("render-lc");
});

// ---- where Docs opens and the spaces it hides (PR #2479, 2026-11-17) ----

const docs = (home: { kind: "space" | "page"; id: string } | null, hidden: string[] = []) => ({
  isHome: (kind: "space" | "page", id: string) => home !== null && home.kind === kind && home.id === id,
  hidden: new Set(hidden),
});
const PAGE_THING: Thing = { kind: "page", id: "confluence:9", hash: "#/page/confluence%3A9" };
const SPACE: Thing = { kind: "space", id: "confluence/ENG", name: "Engineering", hash: "#/docs/space/ENG" };

test("a Docs page offers Open Docs here after Open, which makes it Docs' home by its title", () => {
  const entries = menuFor(PAGE_THING, null, ctx({ docs: docs(null) }));
  expect(ids(entries).slice(0, 2)).toEqual(["open", "docs-home"]);
  expect(item(entries, "docs-home")).toMatchObject({ label: "Open Docs here", act: { do: "docs-home", kind: "page", id: "confluence:9", title: { read: "title" } } });
});

test("on Docs' home the item reads Stop opening Docs here, which goes back to Docs' own page", () => {
  const entries = menuFor(PAGE_THING, null, ctx({ docs: docs({ kind: "page", id: "confluence:9" }) }));
  expect(item(entries, "docs-home")).toMatchObject({ label: "Stop opening Docs here", act: { do: "docs-home", id: null } });
});

test("a space: Open, Open Docs here, Hide from Docs, then Copy name", () => {
  const entries = menuFor(SPACE, null, ctx({ docs: docs(null) }));
  expect(ids(entries)).toEqual(["open", "docs-home", "docs-hide", "|", "copy-name"]);
  expect(item(entries, "docs-home").act).toEqual({ do: "docs-home", kind: "space", id: "confluence/ENG", title: "Engineering" });
  expect(item(entries, "docs-hide")).toMatchObject({ label: "Hide from Docs", act: { do: "docs-hide", id: "confluence/ENG", name: "Engineering", hidden: true } });
  expect(item(entries, "copy-name").act).toEqual({ do: "copy", text: "Engineering", said: "Copied the name" });
});

test("a hidden space offers Unhide, and a space that is Docs' home offers to stop", () => {
  const entries = menuFor(SPACE, null, ctx({ docs: docs({ kind: "space", id: "confluence/ENG" }, ["confluence/ENG"]) }));
  expect(item(entries, "docs-hide")).toMatchObject({ label: "Unhide", act: { hidden: false } });
  expect(item(entries, "docs-home").label).toBe("Stop opening Docs here");
});

test("a failed run offers Mark as read, or Mark as unread once read; a run the page does not draw as failed offers neither (ux18)", () => {
  const failed: Thing = { kind: "run", id: "tc:9", pipelineId: "tc:Pay", hash: "#/build/tc%3APay/tc%3A9", failed: true };
  const unread = menuFor(failed, null, ctx({ runs: { isRead: () => false } }));
  expect(ids(unread)).toEqual(["open", "log", "tests", "rerun", "read", "|", "web", "|", "copy-link"]);
  expect(item(unread, "read")).toMatchObject({ label: "Mark as read", act: { do: "run-read", id: "tc:9", read: true } });
  const read = menuFor(failed, null, ctx({ runs: { isRead: (id) => id === "tc:9" } }));
  expect(item(read, "unread")).toMatchObject({ label: "Mark as unread", act: { do: "run-read", id: "tc:9", read: false } });
  expect(ids(menuFor({ ...failed, failed: false }, null, ctx({ runs: { isRead: () => false } })))).not.toContain("read");
  expect(ids(menuFor(failed, null, ctx()))).not.toContain("read");
});

test("a pipeline offers Mark all as read among what it does (ux18)", () => {
  const entries = menuFor({ kind: "pipeline", id: "tc:Pay", hash: "#/builds/tc%3APay" }, null, ctx({ runs: { isRead: () => false } }));
  expect(item(entries, "read-all")).toMatchObject({ label: "Mark all as read", act: { do: "pipeline-read", id: "tc:Pay" } });
  expect(ids(entries).indexOf("read-all")).toBeLessThan(ids(entries).indexOf("|"));
});

test("a source whose pages open inside knobas offers Open in browser right after its Open in (ADR-0050)", () => {
  const names: Record<string, string> = { jira: "Jira", fj: "Forgejo" };
  const inside = ctx({ sourceName: (id) => names[id] ?? null, opensInside: (id) => id === "jira" || id === "fj" });
  const ticket = menuFor({ kind: "ticket", id: "jira:PAY-1", hash: "#/ticket/jira%3APAY-1" }, null, inside);
  const at = ids(ticket).indexOf("web");
  expect(ids(ticket)[at + 1]).toBe("web-browser");
  expect(item(ticket, "web").act).toEqual({ do: "web", url: { read: "web" }, source: "jira" });
  expect(item(ticket, "web-browser")).toMatchObject({ label: "Open in browser", act: { do: "browser", url: { read: "web" } } });
  const file = menuFor({ kind: "line", repoId: "fj:t/pay", path: "src/a.rs", line: 3 }, null, inside);
  expect(ids(file)[ids(file).indexOf("forge") + 1]).toBe("forge-browser");
  // With the switch off, Open in ⟨source⟩ alone, as before: both would do the same.
  const off = menuFor({ kind: "ticket", id: "jira:PAY-1", hash: "#/ticket/jira%3APAY-1" }, null, ctx({ sourceName: (id) => names[id] ?? null }));
  expect(ids(off)).not.toContain("web-browser");
});

test("a ticket's Move to: the statuses it can reach as a submenu, after what it does; disabled with the reason; absent without a page offering moves (ADR-0054)", () => {
  const offer = { statuses: ["In Review", "Done"], asking: null, unsure: null, disabled: null };
  const entries = menuFor(TICKET, null, ctx({ moves: () => offer }));
  expect(ids(entries).slice(0, 4)).toEqual(["open", "work", "timer", "move"]);
  const move = item(entries, "move");
  expect(move.label).toBe("Move to");
  expect(move.sub!.map((one) => ("rule" in one ? "|" : one.label))).toEqual(["In Review", "Done"]);
  expect((move.sub![1] as Item).act).toEqual({ do: "move", entityId: TICKET.id, status: "Done" });

  const asking = item(menuFor(TICKET, null, ctx({ moves: () => ({ ...offer, statuses: [], asking: "Asking Jira…" }) })), "move");
  expect(asking.sub).toEqual([{ id: "move-asking", label: "Asking Jira…", disabled: "Asking Jira…" }]);
  const unsure = item(menuFor(TICKET, null, ctx({ moves: () => ({ ...offer, unsure: "Jira did not answer; it decides when the move is sent." }) })), "move");
  expect((unsure.sub![0] as Item).disabled).toBe("Jira did not answer; it decides when the move is sent.");

  const refused = item(menuFor(TICKET, null, ctx({ moves: () => ({ ...offer, disabled: "Jira tickets cannot be moved from knobas: its adapter offers no moves." }) })), "move");
  expect(refused).toEqual({ id: "move", label: "Move to", disabled: "Jira tickets cannot be moved from knobas: its adapter offers no moves." });
  expect(ids(menuFor(TICKET, null, ctx()))).not.toContain("move");
  expect(ids(menuFor(TICKET, null, ctx({ moves: () => null })))).not.toContain("move");
});
