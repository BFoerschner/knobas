/**
 * The frontend house rules, enforced instead of remembered.
 *
 * Each rule here is a production failure that is invisible in development:
 * `tauri dev` on desktop navigates straight to the Vite dev server over
 * `http://`, so **no CSP header is attached at all** (`knobas_app`'s crate
 * docs spell this out). A `style="..."` attribute, a remote font, an
 * `{@html}` — every one of them works perfectly under `just dev` and is
 * either blocked or dangerous in a bundle. Nobody notices until a release
 * build, which is exactly when nobody is looking.
 *
 * The scan reads the files off disk rather than importing them, so a rule
 * applies to code no test happens to mount.
 */
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve, sep } from "node:path";

import { expect, test } from "vitest";

/**
 * `app/src/` — everything the bundle is built from.
 *
 * Off `process.cwd()` and not off `import.meta.url`: under the jsdom
 * environment Vitest serves modules over `http://localhost`, so
 * `import.meta.url` is not a `file:` URL and cannot be turned into a path.
 * Vitest's root is `app/` (the directory holding `vite.config.ts`), which is
 * what `cwd` is here.
 */
const ROOT = join(process.cwd(), "src") + "/";

/**
 * This file is excluded from its own scan.
 *
 * It has to contain the very patterns it forbids — `{@html`, a `style=`
 * matcher — so scanning it would make every rule fail on the rule itself, and
 * the only way to keep it green would be to weaken the patterns until they no
 * longer match their own text. That is a lint that has been tuned to pass
 * rather than to catch.
 */
const SELF = "house-rules.test.ts";

/** Every `.svelte` and `.ts` file under `app/src/`, absolute, sorted. */
function sources(): string[] {
  const found: string[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(path);
      } else if (/\.(svelte|ts)$/.test(entry.name) && entry.name !== SELF) {
        found.push(path);
      }
    }
  };
  walk(ROOT);
  return found.sort();
}

/**
 * A `.svelte` file's template: everything outside its `<script>` blocks, with
 * HTML comments removed.
 *
 * Both removals matter. A `<script>` block is TypeScript, where `style:` is an
 * ordinary object key and `{@html` cannot occur at all; an HTML comment is
 * documentation, and this codebase documents these very rules next to the code
 * that obeys them. Scanning either would make the lint fire on prose.
 */
function markupOf(text: string): string {
  return text
    .replace(/<script\b[\s\S]*?<\/script>/g, "")
    .replace(/<!--[\s\S]*?-->/g, "");
}

/**
 * A file's code with its comments removed, string literals left intact.
 *
 * Load-bearing for any rule that asks whether the *code* does something: this
 * codebase documents its own house rules next to the code that obeys them, so
 * a scan over raw text can be satisfied by the sentence describing the rule
 * rather than by the rule being followed. That is a lint that fails open, and
 * it is exactly how the guard below went green against an unguarded import.
 */
function codeOf(text: string): string {
  let out = "";
  let index = 0;
  while (index < text.length) {
    const c = text[index]!;
    const next = text[index + 1];
    if (c === '"' || c === "'" || c === "`") {
      out += c;
      index += 1;
      while (index < text.length) {
        const s = text[index]!;
        out += s;
        index += 1;
        if (s === "\\") {
          out += text[index] ?? "";
          index += 1;
        } else if (s === c) {
          break;
        }
      }
    } else if (c === "/" && next === "/") {
      while (index < text.length && text[index] !== "\n") index += 1;
    } else if (c === "/" && next === "*") {
      index += 2;
      while (index < text.length && !(text[index] === "*" && text[index + 1] === "/")) index += 1;
      index += 2;
      out += " ";
    } else if (c === "<" && text.startsWith("<!--", index)) {
      const close = text.indexOf("-->", index);
      index = close === -1 ? text.length : close + 3;
      out += " ";
    } else {
      out += c;
      index += 1;
    }
  }
  return out;
}

/** The offending paths, relative to `app/src/`, so a failure is readable. */
function offenders(predicate: (text: string, file: string) => boolean): string[] {
  return sources()
    .filter((file) => predicate(readFileSync(file, "utf8"), file))
    .map((file) => file.slice(ROOT.length));
}

/**
 * `app.security.csp` is `style-src 'self'` with no `'unsafe-inline'`, which
 * blocks inline style *attributes* in CSP2 and later. Component `<style>`
 * blocks are fine: `vite build` extracts them into `assets/index-*.css`, which
 * `'self'` covers. Dynamic geometry uses a class or a native `<progress>`.
 *
 * The scan looks at markup only — `<script>` blocks are stripped first, so a
 * TypeScript object with a `style:` key is not an offence.
 */
test("no inline style attributes or directives (CSP style-src 'self')", () => {
  expect(
    offenders((text, file) => {
      if (!file.endsWith(".svelte")) {
        // A `setAttribute("style", ..)` is the same inline attribute by
        // another route, and CSP refuses it the same way.
        return /setAttribute\(\s*["']style["']/.test(text);
      }
      const markup = markupOf(text);
      return (
        /\sstyle\s*=\s*["'{]/.test(markup) ||
        /\sstyle:[a-z][a-z0-9-]*/.test(markup) ||
        /setAttribute\(\s*["']style["']/.test(text)
      );
    }),
  ).toEqual([]);
});

/**
 * Roadmap §4 gotcha 7: `ts_headline` output is not XSS-safe, and every title,
 * body, author, payload and snippet segment in this app is raw text somebody
 * typed into a ticket. Rendering any of it as markup is a script-injection
 * path from a source system straight into the webview.
 */
test("no {@html} — source text is untrusted (gotcha 7)", () => {
  // `.svelte` only, and markup only: `{@html}` is template syntax. It cannot
  // appear in a `.ts` module or inside a `<script>` block, so anything that
  // looks like it there is prose about the rule.
  expect(
    offenders(
      (text, file) => file.endsWith(".svelte") && markupOf(text).includes("{@html"),
    ),
  ).toEqual([]);
});

/**
 * `default-src 'self'`: a bundle reaches nothing on the network but the IPC.
 * Fonts are npm-vendored (`@fontsource/*`) and bundled, never fetched.
 *
 * Two exemptions, and both are exempt because they cannot reach anything:
 *
 * * **loopback**, because the QA convention documents a local preview server;
 * * **the reserved names** of RFC 2606 and RFC 6761 — `.example`, `.invalid`,
 *   `.test`, `.localhost`, and `example.com/net/org`. The IANA guarantees
 *   these never resolve, which is the exact property this rule protects, and
 *   a source's `base_url` is the single most common thing a sources-view
 *   fixture has to spell. Without them the rule pushes fixtures towards a
 *   plausible-looking real domain, which is strictly worse: it is a hostname
 *   that *could* answer.
 *
 * A hostname that is neither loopback nor reserved is an offence wherever it
 * appears, test file or not.
 */
const RESERVED_HOST =
  /^(?:localhost|127\.0\.0\.1|\[::1\]|(?:[\w-]+\.)*(?:example|invalid|test|localhost)|(?:[\w-]+\.)*example\.(?:com|net|org))(?::[^/.]*)?$/;

test("no runtime network references (default-src 'self')", () => {
  expect(
    offenders((text) => {
      for (const match of text.matchAll(/https?:\/\/([^\s"'`/)\]}>]+)/g)) {
        if (!RESERVED_HOST.test(match[1]!)) return true;
      }
      return false;
    }),
  ).toEqual([]);
});

/**
 * The rule above is a regex over a hostname, and a regex that is wrong in the
 * permissive direction is a lint that has quietly stopped being one. So it is
 * exercised against the shapes it has to separate, rather than trusted.
 */
test("the reserved-host exemption admits only names that cannot resolve", () => {
  const allowed = [
    "localhost",
    "localhost:5301",
    // The QA convention's documented command line, where the port is a shell
    // variable rather than a number.
    "localhost:$PORT",
    "127.0.0.1:1420",
    "jira.tidewater.example",
    "example.com",
    "api.example.org",
    "anything.invalid",
    "mock.test",
  ];
  const refused = [
    "jira.tidewater.example.co",
    "example.company.com",
    "notexample.com",
    "atlassian.net",
    "fonts.googleapis.com",
    "127.0.0.1.evil.com",
    "example.com.evil.net",
    // The port is allowed to be non-numeric, but it may not smuggle a host in.
    "localhost:evil.com",
  ];
  expect(allowed.filter((host) => !RESERVED_HOST.test(host))).toEqual([]);
  expect(refused.filter((host) => RESERVED_HOST.test(host))).toEqual([]);
});

/** The dev harness, as a path rather than as a spelling. */
const DEV_DIR = join(ROOT, "lib/shell/dev");

/**
 * Every module specifier a file imports, tagged static or dynamic.
 *
 * Three forms, because all three reach the module graph: `… from "x"` (which
 * also covers `export * from`), a bare side-effect `import "x"`, and the
 * dynamic `import("x")`.
 */
function specifiers(code: string): { specifier: string; dynamic: boolean }[] {
  const found: { specifier: string; dynamic: boolean }[] = [];
  for (const match of code.matchAll(/\bfrom\s*["'`]([^"'`]+)["'`]/g)) {
    found.push({ specifier: match[1]!, dynamic: false });
  }
  for (const match of code.matchAll(/\bimport\s+["'`]([^"'`]+)["'`]/g)) {
    found.push({ specifier: match[1]!, dynamic: false });
  }
  for (const match of code.matchAll(/\bimport\s*\(\s*["'`]([^"'`]+)["'`]\s*\)/g)) {
    found.push({ specifier: match[1]!, dynamic: true });
  }
  return found;
}

/**
 * The imports of `file` that land inside the dev harness.
 *
 * **Resolved, not string-matched.** A substring test against the specifier
 * text is spelling-dependent, and the spellings are not equivalent to a
 * reader: every module that would import the harness lives in
 * `lib/shell/`, where the natural form — and the one auto-import produces —
 * is `./dev/fake-tauri`, which contains no `shell/dev/` at all. That is a
 * working, unguarded, static import the old check could not see. Resolving
 * against the importing file's own directory removes the question of how the
 * path happens to be written, which matters more as tasks 7-23 add roughly
 * twenty files into exactly that directory.
 */
function devImports(code: string, file: string): { specifier: string; dynamic: boolean }[] {
  return specifiers(code).filter(({ specifier }) => {
    // Only relative specifiers can reach it; a bare one is a package.
    if (!specifier.startsWith(".")) return false;
    const target = resolve(dirname(file), specifier);
    return target === DEV_DIR || target.startsWith(DEV_DIR + sep);
  });
}

/**
 * `shell/dev/**` answers `invoke` from a fixture. In a production bundle that
 * would be a frontend quietly serving itself fake data, so its only reachable
 * import site is behind `import.meta.env.DEV`, which Rollup constant-folds to
 * `false` and drops the branch — and the module with it.
 */
test("the dev harness is only reachable behind import.meta.env.DEV", () => {
  const importers = sources()
    .filter((file) => !file.startsWith(DEV_DIR + sep))
    .map((file) => ({ file, code: codeOf(readFileSync(file, "utf8")) }))
    .map((entry) => ({ ...entry, imports: devImports(entry.code, entry.file) }))
    .filter((entry) => entry.imports.length > 0);

  expect(
    importers.length,
    "nothing imports the dev harness at all, so this test proves nothing",
  ).toBeGreaterThan(0);

  // The violations first, so a failure names the actual sin rather than the
  // bookkeeping mismatch that follows from it.
  for (const { file, code, imports } of importers) {
    const where = file.slice(ROOT.length);

    // A static import is a hard edge in the module graph: Rollup cannot drop
    // it however the branch around its *use* is written.
    expect(
      imports.filter((entry) => !entry.dynamic).map((entry) => entry.specifier),
      `${where} imports the dev harness statically`,
    ).toEqual([]);

    expect(code, `${where} imports the dev harness without an import.meta.env.DEV guard`).toMatch(
      /if\s*\(\s*import\.meta\.env\.DEV\s*\)/,
    );
  }

  // ...and the set itself, because every door into the fake bridge is a
  // decision someone should have to make on purpose. Growing this list is
  // allowed; doing it without noticing is not.
  expect(
    importers.map((entry) => entry.file.slice(ROOT.length)).sort(),
    "a module started importing the dev harness — add it here only if a second entry point to the fixture is genuinely wanted",
  ).toEqual(["App.svelte"]);
});
