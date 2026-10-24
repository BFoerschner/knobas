/**
 * No `??` on a left side the type checker says cannot be nullish (#1884).
 *
 * TypeScript lets `adapter.payload_paths ?? []` through when `payload_paths` is
 * required, and the frontend has no type-aware lint, so a fallback on an IPC
 * field the generated mirror types as always sent outlived its reason more
 * than once. This walks every `a ?? b` in the app's non-test `.ts` and `.svelte`
 * files with the checker under `tsconfig.json` and fails on the ones whose left
 * side is not nullish. The checker does not read `.svelte`, so each one goes
 * through svelte2tsx first (the transform svelte-check runs), script and
 * template alike, and the program gets the result as a virtual `X.svelte.tsx`
 * that `./X.svelte` imports resolve to (#1890). A hit in a `.svelte` file is
 * keyed by the `.svelte` path and the expression as svelte2tsx prints it.
 *
 * A hit that is right at runtime although the type says otherwise goes in
 * `KEPT`, with the reason; the key is the file and the expression, so a moved
 * line still matches and a second copy does not.
 */
import { readFileSync } from "node:fs";
import { dirname, join, relative, resolve, sep } from "node:path";

import { svelte2tsx } from "svelte2tsx";
import ts from "typescript";
import { expect, test } from "vitest";

const APP = process.cwd();
const svelteVersion = (JSON.parse(readFileSync(join(APP, "node_modules/svelte/package.json"), "utf8")) as { version: string }).version;

/** Hits that are right at runtime, keyed `file :: expression`. */
const KEPT = new Map<string, string>([
  [
    "src/lib/ipc/index.ts :: JSON.stringify(error) ?? String(error)",
    "`JSON.stringify` returns undefined for a function or a symbol, which its type hides",
  ],
  ["src/lib/tf/inbox/inbox.ts :: KIND[category] ?? category", "a `Record` index the checker types as present"],
  [
    "src/lib/detail/view/pending-comments.svelte.ts :: window.localStorage ?? null",
    "the accessor is null where site data is blocked",
  ],
  [
    "src/lib/shell/dev/fake/link-sets.ts :: draft.patterns ?? []",
    "Rust's `#[serde(default)]` request-side default, which the fake matches",
  ],
]);

const NULLISH =
  ts.TypeFlags.Any |
  ts.TypeFlags.Unknown |
  ts.TypeFlags.Undefined |
  ts.TypeFlags.Null |
  ts.TypeFlags.Void |
  ts.TypeFlags.TypeParameter;

/** A test or a fixture: no part of the app. */
const TEST_ONLY = /\.test\.|\.fixture\./;

/** Every `a ?? b` outside the tests whose left operand cannot be nullish, as `file :: expression`. */
function impossibleFallbacks(): string[] {
  const config = ts.readConfigFile(join(APP, "tsconfig.json"), ts.sys.readFile);
  const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, APP);
  // The tests are left out of the roots, not only out of the walk: half the
  // tree's text and the test libraries' types, for no `??` this reads (#2035).
  // What a source file imports is still in the program.
  const files = ts.sys
    .readDirectory(join(APP, "src"), [".ts"])
    .filter((file) => !file.endsWith(".d.ts") && !TEST_ONLY.test(file));
  const virtual = new Map<string, string>();
  for (const file of ts.sys.readDirectory(join(APP, "src"), [".svelte"])) {
    const out = svelte2tsx(readFileSync(file, "utf8"), { filename: file, isTsFile: true, mode: "ts", version: svelteVersion });
    virtual.set(`${file}.tsx`, out.code);
  }
  const shims = ["svelte-shims-v4.d.ts", "svelte-jsx-v4.d.ts"].map((name) => join(APP, "node_modules/svelte2tsx", name));
  const options: ts.CompilerOptions = { ...parsed.options, noEmit: true, jsx: ts.JsxEmit.Preserve };
  const host = ts.createCompilerHost(options);
  const { fileExists, readFile, getSourceFile } = host;
  host.fileExists = (file) => virtual.has(file) || fileExists(file);
  host.readFile = (file) => virtual.get(file) ?? readFile(file);
  host.getSourceFile = (file, ...rest) => {
    const code = virtual.get(file);
    return code === undefined ? getSourceFile(file, ...rest) : ts.createSourceFile(file, code, rest[0], true, ts.ScriptKind.TSX);
  };
  host.resolveModuleNameLiterals = (literals, containing) =>
    literals.map((literal) => {
      const target = literal.text.endsWith(".svelte") ? `${resolve(dirname(containing), literal.text)}.tsx` : "";
      if (virtual.has(target)) return { resolvedModule: { resolvedFileName: target, extension: ts.Extension.Tsx } };
      return ts.resolveModuleName(literal.text, containing, options, host);
    });
  const program = ts.createProgram([...files, ...virtual.keys(), ...shims], options, host);
  const checker = program.getTypeChecker();
  const found: string[] = [];
  for (const source of program.getSourceFiles()) {
    const name = relative(APP, source.fileName).split(sep).join("/").replace(/\.svelte\.tsx$/, ".svelte");
    if (!name.startsWith("src/") || TEST_ONLY.test(name)) continue;
    const visit = (node: ts.Node) => {
      if (ts.isBinaryExpression(node) && node.operatorToken.kind === ts.SyntaxKind.QuestionQuestionToken) {
        const type = checker.getTypeAtLocation(node.left);
        const parts = type.isUnion() ? type.types : [type];
        if (!parts.some((part) => part.flags & NULLISH)) {
          found.push(`${name} :: ${node.left.getText()} ?? ${node.right.getText()}`.replace(/\s+/g, " "));
        }
      }
      ts.forEachChild(node, visit);
    };
    visit(source);
  }
  return found;
}

test("no `??` falls back on a value the type says is always there", () => {
  const found = impossibleFallbacks();
  const unexpected = found.filter((hit) => !KEPT.has(hit));
  expect(
    unexpected,
    "the left side's type has no null or undefined: drop the fallback, or tighten the type that loosened it; if the runtime value really can be missing, add it to KEPT with the reason",
  ).toEqual([]);
  const stale = [...KEPT.keys()].filter((hit) => !found.includes(hit));
  expect(stale, "these KEPT entries match nothing: remove them").toEqual([]);
}, 60_000);
