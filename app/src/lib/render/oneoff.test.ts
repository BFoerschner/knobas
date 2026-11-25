/** Commands' pure half (ADR-0060): pieces, endings, findings, copies and the twins of the Rust rules. */
import { expect, test } from "vitest";

import twin from "./oneoff-twin.json";

import type { OneOff, OneOffLine } from "../ipc/oneoff";
import { ago, check, classOf, clock, endedShort, fill, kindOf, outputText, pieces, problems, problemsText, shown, stateOf, takesFile, takesFolder, takesRepository, took } from "./oneoff";

const line = (n: number, text: string, extra: Partial<OneOffLine> = {}): OneOffLine => ({ n, stream: "out", text, ...extra });

test("a line is cut into pieces where a colour, a place or an address starts or ends", () => {
  const l = line(0, "  12:7  error  x see https://e.example/r", {
    runs: [{ from: 2, to: 6, style: { dim: true } }, { from: 8, to: 13, style: { fg: "r", bold: true } }],
    places: [{ from: 2, to: 6, path: "src/a.ts", line: 12, col: 7 }],
    urls: [{ from: 21, to: 40, href: "https://e.example/r" }],
  });
  const got = pieces(l).map((p) => [p.text, p.cls, p.place?.line ?? null, p.url?.href ?? null]);
  expect(got).toEqual([
    ["  ", "", null, null],
    ["12:7", "tf-oo-dim", 12, null],
    ["  ", "", null, null],
    ["error", "tf-oo-r tf-oo-bold", null, null],
    ["  x see ", "", null, null],
    ["https://e.example/r", "", null, "https://e.example/r"],
  ]);
  // A wrapped row is a slice of the line.
  expect(pieces(l, 4, 10).map((p) => p.text).join("")).toBe(l.text.slice(4, 10));
  expect(pieces(line(1, ""))).toEqual([]);
});

test("a program's colours are the theme's names, never a value", () => {
  expect(classOf({ fg: "g", bg: "r", bold: true, dim: true, italic: true, underline: true })).toBe("tf-oo-g tf-oo-on-r tf-oo-bold tf-oo-dim tf-oo-it tf-oo-ul");
  expect(classOf({})).toBe("");
});

test("each ending's words and tone", () => {
  expect(stateOf(null)).toEqual({ tone: "info", words: "Running", code: null });
  expect(stateOf({ how: "exit", code: 0 })).toEqual({ tone: "ok", words: "Exited with", code: 0 });
  expect(stateOf({ how: "exit", code: 1 })).toEqual({ tone: "err", words: "Exited with", code: 1 });
  expect(stateOf({ how: "timed_out", seconds: 120 }).tone).toBe("warn");
  expect(stateOf({ how: "stopped" })).toEqual({ tone: "muted", words: "Stopped by you", code: null });
  expect(stateOf({ how: "signal", signal: 9 })).toEqual({ tone: "err", words: "Ended by signal", code: 9 });
  expect(stateOf({ how: "not_started", reason: "x" }).words).toBe("Did not start");
  expect(endedShort({ how: "exit", code: 1 })).toBe("exit 1");
  expect(endedShort(null)).toBe("running");
});

test("times read as a person reads them", () => {
  expect(clock(7_400)).toBe("0:07");
  expect(clock(72_000)).toBe("1:12");
  expect(took(2_400)).toBe("2.4s");
  expect(took(72_000)).toBe("1m 12s");
  expect(ago(1_000, 1_000 + 14 * 60_000)).toBe("14 min ago");
  expect(ago(0, 30 * 3600_000)).toBe("yesterday");
});

test("findings are grouped by file, and copied as path:line:col: severity: words", () => {
  const lines = [
    line(0, "src/a.ts"),
    line(1, "  3:1  error  x  semi", { places: [{ from: 2, to: 5, path: "src/a.ts", line: 3, col: 1 }], finding: { severity: "error", said: "x", rule: "semi" } }),
    line(2, "b.ts:9: warning: y", { places: [{ from: 0, to: 6, path: "b.ts", line: 9, col: null }], finding: { severity: "warning", said: "y", rule: "" } }),
    line(3, "  at f (src/a.ts:1:1)", { places: [{ from: 8, to: 20, path: "src/a.ts", line: 1, col: 1 }] }),
  ];
  const groups = problems(lines);
  expect(groups.map((g) => [g.path, g.list.length])).toEqual([["src/a.ts", 1], ["b.ts", 1]]);
  expect(problemsText(groups)).toBe("src/a.ts:3:1: error: x [semi]\nb.ts:9: warning: y");
  expect(outputText(lines.slice(0, 2))).toBe("src/a.ts\n  3:1  error  x  semi\n");
});

test("the twins read a command's kind and refuse what Save refuses", () => {
  expect(kindOf("eslint {file}")).toBe("file");
  expect(kindOf("eslint {dir}")).toBe("folder");
  expect(kindOf("cargo clippy")).toBe("repository");
  expect(kindOf("git show {sha}")).toBe("commit");
  expect(kindOf("git diff {base} {head}")).toBe("range");
  expect(kindOf("git blame {sha} -- {relfile}")).toBe("file");
  expect(kindOf("jq .", "selection")).toBe("file");
  expect(check("eslint {file}")).toBeNull();
  expect(check("eslint {path}")).toContain("{root}");
  expect(check("eslint {title}")).toContain("{title} is not something knobas fills in");
  expect(check(`sh -c "eslint {file} | head"`)).toContain("quoted script");
  expect(check(`sh -c 'eslint "$1" | head -40' _ {file}`)).toBeNull();
  expect(check("eslint -c {root}/x.js {file}")).toBeNull();
  expect(check("sh -c {file}")).toContain("script of sh -c");
  expect(check("wsl ruff check {file}")).toContain("-e stands");
  expect(check("x {out}.png {outdir}")).toContain("one of them");
  expect(check("x --in={files}")).toContain("stands alone");
  expect(check("sh", "file")).toContain("would run");
});

test("fill makes the words the backend will run, one argument a path", () => {
  const at = { root: "/c/payout-service", file: "src/a b;$(x).ts", dir: "src" };
  expect(fill("eslint --max-warnings 0 {file}", at)).toEqual({ argv: ["eslint", "--max-warnings", "0", "/c/payout-service/src/a b;$(x).ts"] });
  expect(fill("git blame -- {relfile} {reldir} {name}", at)).toEqual({ argv: ["git", "blame", "--", "src/a b;$(x).ts", "src", "payout-service"] });
  expect(fill("x {relfile}", { ...at, file: "-rf" })).toEqual({ error: "{relfile} would be '-rf', which a program would read as an option: use {file}" });
  expect(fill("x {sha}", at)).toEqual({ error: "there is no {sha} here" });
  expect(fill("p {files}", { ...at, files: ["a", "b"] })).toEqual({ argv: ["p", "/c/payout-service/a", "/c/payout-service/b"] });
  expect(shown(["eslint", "/a b/c.ts", ""])).toBe('eslint "/a b/c.ts" ""');
});

const command = (text: string, extra: Partial<OneOff> = {}): OneOff => ({ id: "x", name: "X", command: text, stdin: "none", files: [], folders: [], has: [], repo_holds: [], repos: [], shows: "auto", start_in: "root", path: [], env: [], colours: true, timeout_secs: 120, wsl_paths: false, ...extra });

test("a menu offers only what applies", () => {
  const facts = { repo: "gitea:a/b", top: ["package.json", "src"] };
  const eslint = command("eslint {file}", { files: ["*.ts"], repo_holds: ["package.json"] });
  expect(takesFile(eslint, "src/a.ts", facts)).toBe(true);
  expect(takesFile(eslint, "src/a.rs", facts)).toBe(false);
  expect(takesFile(eslint, "src/a.ts", { repo: "x", top: ["Cargo.toml"] })).toBe(false);
  expect(takesFile(eslint, "src/a.ts", { repo: "x", top: null })).toBe(true);
  const folder = command("ruff check {dir}", { has: ["*.py"] });
  expect(takesFolder(folder, "scripts", ["a.py"], facts)).toBe(true);
  expect(takesFolder(folder, "scripts", ["a.ts"], facts)).toBe(false);
  expect(takesRepository(command("cargo clippy"), facts)).toBe(true);
  expect(takesRepository(command("cargo clippy", { repos: ["gitea:other"] }), facts)).toBe(false);
});

test("the twin holds to the Rust: the shared cases of oneoff-twin.json", () => {
  for (const c of twin.cases) {
    expect(kindOf(c.command, c.stdin as "none" | "file" | "selection"), c.command).toBe(c.kind);
    const said = check(c.command, c.stdin as "none" | "file" | "selection");
    if (c.refused === null) expect(said, c.command).toBeNull();
    else expect(said, c.command).toContain(c.refused);
  }
});
