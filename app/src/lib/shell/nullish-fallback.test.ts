/**
 * No `??` on a left side the type checker says cannot be nullish (#1884).
 *
 * TypeScript lets `adapter.payload_paths ?? []` through when `payload_paths` is
 * required, and the frontend has no type-aware lint, so a fallback on an IPC
 * field the generated mirror types as always sent outlived its reason more
 * than once. This walks every `a ?? b` in the app's non-test `.ts` files with
 * the checker under `tsconfig.json` and fails on the ones whose left side is
 * not nullish. `.svelte` files are not covered: the checker does not read them.
 *
 * A hit that is right at runtime although the type says otherwise goes in
 * `KEPT`, with the reason; the key is the file and the expression, so a moved
 * line still matches and a second copy does not.
 */
import { join, relative, sep } from "node:path";

import ts from "typescript";
import { expect, test } from "vitest";

const APP = process.cwd();

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

/** Every `a ?? b` outside the tests whose left operand cannot be nullish, as `file :: expression`. */
function impossibleFallbacks(): string[] {
  const config = ts.readConfigFile(join(APP, "tsconfig.json"), ts.sys.readFile);
  const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, APP);
  const files = ts.sys.readDirectory(join(APP, "src"), [".ts"]).filter((file) => !file.endsWith(".d.ts"));
  const program = ts.createProgram(files, { ...parsed.options, noEmit: true });
  const checker = program.getTypeChecker();
  const found: string[] = [];
  for (const source of program.getSourceFiles()) {
    const name = relative(APP, source.fileName).split(sep).join("/");
    if (!name.startsWith("src/") || /\.test\.|\.fixture\./.test(name)) continue;
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
