/**
 * **A bare `open`, `name`, `close`... is a local that went missing, never the
 * browser's global.** (#930)
 *
 * PR #928 renamed a parameter in `week-view.ts` and left `bar: !open && !empty`
 * behind. `open` then resolved to `window.open`, a function, so `!open` was
 * always `false` and no Week cell drew its bar. `svelte-check` passed: `lib.dom`
 * declares `open` as a global, so the name is bound and the type is fine.
 *
 * The repo has no ESLint, so this is the `no-restricted-globals` rule with
 * `confusing-browser-globals`' list, done with the type checker instead: an
 * identifier is a hit when it *resolves to a declaration in `lib.dom`*, which a
 * local, a parameter, an import or a property name never does. No grep false
 * positives on `item.open` or an `open` object key, and `window.open(...)`,
 * the spelling that means the browser, stays legal.
 *
 * Scope: `.ts` files, and the `<script>` blocks of `.svelte` files. A name used
 * only inside a template expression is not seen (the compiler for those is not
 * in the dependency tree); script blocks are where the #928 shape lives.
 */
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

import ts from "typescript";
import { expect, test } from "vitest";

/** `eslint-config-react-app`'s `confusing-browser-globals`, cut to the names `lib.dom` declares as plain globals that a local plausibly shadows. */
const CONFUSING = new Set([
  "close",
  "event",
  "length",
  "name",
  "open",
  "origin",
  "parent",
  "status",
  "top",
]);

/** `app/src/`; off `process.cwd()` for the reason `house-rules.test.ts` gives. */
const ROOT = join(process.cwd(), "src") + "/";

/** The TypeScript a file contributes: itself, or a `.svelte` file's script blocks. */
function codeOf(path: string, text: string): string {
  if (!path.endsWith(".svelte")) return text;
  return [...text.matchAll(/<script\b[^>]*>([\s\S]*?)<\/script>/g)].map((m) => m[1]).join("\n");
}

/** A name that is a *member* of something, or a key: it binds nothing here. */
function isMember(node: ts.Identifier): boolean {
  const parent = node.parent;
  return (
    (ts.isPropertyAccessExpression(parent) && parent.name === node) ||
    (ts.isQualifiedName(parent) && parent.right === node) ||
    (ts.isPropertyAssignment(parent) && parent.name === node) ||
    (ts.isPropertySignature(parent) && parent.name === node) ||
    (ts.isMethodSignature(parent) && parent.name === node) ||
    (ts.isMethodDeclaration(parent) && parent.name === node) ||
    (ts.isPropertyDeclaration(parent) && parent.name === node) ||
    (ts.isGetAccessor(parent) && parent.name === node) ||
    (ts.isSetAccessor(parent) && parent.name === node) ||
    (ts.isBindingElement(parent) && parent.propertyName === node) ||
    (ts.isImportSpecifier(parent) && parent.propertyName === node) ||
    (ts.isExportSpecifier(parent) && parent.propertyName === node) ||
    (ts.isJsxAttribute(parent) && parent.name === node) ||
    ts.isTypeReferenceNode(parent)
  );
}

/** The identifiers in `source` that may be a bare confusing global: each confusing name that is not a member or a key. */
function candidates(source: ts.SourceFile): ts.Identifier[] {
  const found: ts.Identifier[] = [];
  const visit = (node: ts.Node) => {
    if (ts.isIdentifier(node) && CONFUSING.has(node.text) && !isMember(node)) found.push(node);
    ts.forEachChild(node, visit);
  };
  visit(source);
  return found;
}

const OPTIONS: ts.CompilerOptions = {
  target: ts.ScriptTarget.ES2022,
  lib: ["lib.es2022.d.ts", "lib.dom.d.ts", "lib.dom.iterable.d.ts"],
  types: [],
  noResolve: true,
  skipLibCheck: true,
};

/**
 * The lib files, parsed once for every program in this file. `lib.dom` is
 * most of a program's parse and bind, and it never changes (#2035).
 */
const LIBS = new Map<string, ts.SourceFile | undefined>();

/**
 * Every bare use of a confusing browser global in `files` (path to code),
 * as `path:line name`.
 *
 * Only the files with a candidate go to the checker, and in each only the
 * candidates are looked up (#2035): a member lookup made the checker type the
 * object it hangs off, which was most of the scan's time on the runner. With
 * `noResolve` every file is checked against `lib` alone, so a file left out
 * changes no other file's answer.
 */
function bareGlobalUses(files: Map<string, string>): string[] {
  const roots = new Map<string, { source: ts.SourceFile; names: ts.Identifier[] }>();
  for (const [path, text] of files) {
    const source = ts.createSourceFile(path, text, ts.ScriptTarget.ES2022, true);
    const names = candidates(source);
    if (names.length > 0) roots.set(path, { source, names });
  }
  if (roots.size === 0) return [];
  const host = ts.createCompilerHost(OPTIONS);
  const read = host.getSourceFile.bind(host);
  host.getSourceFile = (name, lang, ...rest) => {
    const root = roots.get(name)?.source;
    if (root) return root;
    if (!LIBS.has(name)) LIBS.set(name, read(name, lang, ...rest));
    return LIBS.get(name);
  };
  const program = ts.createProgram([...roots.keys()], OPTIONS, host);
  const checker = program.getTypeChecker();

  const fromLibDom = (symbol: ts.Symbol | undefined): boolean =>
    !!symbol?.declarations?.length &&
    symbol.declarations.every(
      (d) =>
        /lib\.dom\.d\.ts$/.test(d.getSourceFile().fileName) &&
        (ts.isVariableDeclaration(d) || ts.isFunctionDeclaration(d)),
    );

  const hits: string[] = [];
  for (const [path, { source, names }] of roots) {
    for (const node of names) {
      const symbol = ts.isShorthandPropertyAssignment(node.parent)
        ? checker.getShorthandAssignmentValueSymbol(node.parent)
        : checker.getSymbolAtLocation(node);
      if (fromLibDom(symbol)) {
        const { line } = source.getLineAndCharacterOfPosition(node.getStart());
        hits.push(`${path}:${line + 1} ${node.text}`);
      }
    }
  }
  return hits;
}

/** Every `.svelte` and `.ts` file under `app/src/`, as path to checkable code. */
function sources(): Map<string, string> {
  const found = new Map<string, string>();
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (/\.(svelte|ts)$/.test(entry.name)) {
        // A virtual `.ts` name for a `.svelte` file: the checker needs an extension it parses.
        found.set(path.endsWith(".svelte") ? `${path}.ts` : path, codeOf(path, readFileSync(path, "utf8")));
      }
    }
  };
  walk(ROOT);
  return found;
}

test("the scan flags the #928 shape and nothing a local or a property can be", () => {
  const files = new Map([
    ["/virtual/bug.ts", "export const f = (empty: boolean) => ({ bar: !open && !empty });"],
    [
      "/virtual/fine.ts",
      [
        "export const a = (open: boolean) => !open;",
        "export const b = (d: HTMLDialogElement) => d.open && window.open('x');",
        "export const c = { name: 'x', status: 1 };",
        "export function d() { const close = 1; return { close }; }",
        "export const e = (s: { parent: number }) => s.parent;",
      ].join("\n"),
    ],
  ]);
  expect(bareGlobalUses(files)).toEqual(["/virtual/bug.ts:1 open"]);
});

test("no source file uses a confusing browser global bare", () => {
  expect(bareGlobalUses(sources())).toEqual([]);
});
