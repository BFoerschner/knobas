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

test("a run: its log and tests, Re-run queued, its page on TeamCity", () => {
  const entries = menuFor({ kind: "run", id: "tc:9", pipelineId: "tc:Pay", hash: "#/build/tc%3APay/tc%3A9" }, null, ctx());
  expect(ids(entries)).toEqual(["open", "log", "tests", "rerun", "|", "web", "|", "copy-link"]);
  expect(item(entries, "rerun").act).toMatchObject({ do: "write", payload: { RerunBuild: { entity: "tc:9" } }, sourceId: "tc" });
  expect(item(entries, "web").label).toBe("Open in TeamCity");
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
