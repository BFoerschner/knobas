#!/usr/bin/env node
// Writes app/src/lib/render/languages.ts: knobas's own table of languages, each one's name in each highlighter, and the
// file names, extensions and Markdown fence names that pick one (the highlighter re-ruling, 2026-11-21, section 2).
//
//   node scripts/gen-languages.mjs            write it
//   node scripts/gen-languages.mjs --check    exit 1 if the file differs from what the installed packages give
//
// The ids are Shiki's (so `languageOf("Dockerfile")` stays "docker"), plus the few languages only Pierre Highlights or
// TanStack Highlight knows. An engine has a language when the id or one of Shiki's aliases is one of the engine's names,
// with the overrides and the deny list below. Run from the repository's root with `app/node_modules` installed; a
// version bump of an engine regenerates this file in its own pull request.
import { readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const app = join(root, "app");
const require = createRequire(join(app, "package.json"));
const OUT = join(app, "src/lib/render/languages.ts");

const { bundledLanguagesInfo } = await import(require.resolve("shiki/langs"));
const { LANGS } = await import(join(dirname(require.resolve("@pierre/highlights")), "highlighter.js"));
const tanstack = Object.values(await import(require.resolve("@tanstack/highlight/languages")));
const version = (name) => JSON.parse(readFileSync(join(app, "node_modules", name, "package.json"), "utf8")).version;

// Pierre: LANGS maps every name and alias to its lexer's number. A language is named to Pierre as Shiki spells it where
// Pierre takes that name, so the table reads as the languages do.
const pierreLexer = new Map(Object.entries(LANGS));
// TanStack: a definition's name, and its aliases; a language is named to it by the definition's own name, which is what
// its answer's `lang` says back.
const tanstackName = new Map();
for (const def of tanstack) for (const name of [def.name, ...(def.aliases ?? [])]) if (!tanstackName.has(name)) tanstackName.set(name, def.name);

/** Names an engine takes for a Shiki id where matching by name finds the wrong one or none. */
const OVERRIDE = {
  pierre: { docker: "dockerfile", shellscript: "bash", make: "makefile" },
  tanstack: { docker: "dockerfile", shellscript: "shell", make: "makefile", dotenv: "env", xml: "html" },
};
/** Names that must not match: TanStack's alias `v` is Verilog, Shiki's `v` the V language. */
const DENY = { pierre: new Set(["plain", "plaintext", "text", "txt", "ansi"]), tanstack: new Set(["v", "plaintext", "text", "txt", "-->"]) };
/** The languages only another engine knows, by the id knobas gives them. */
const EXTRA = {
  cuda: { pierre: "cuda" },
  groq: { pierre: "groq" },
  promql: { pierre: "promql" },
  tsrx: { pierre: "tsrx", tanstack: "tsrx" },
  ejs: { tanstack: "ejs" },
  // Shiki reads a properties file with its ini grammar; the other two have a lexer of their own.
  properties: { pierre: "properties", tanstack: "properties", shiki: "ini" },
};

function nameIn(engine, names, id) {
  const override = OVERRIDE[engine][id];
  if (override !== undefined) return override;
  for (const n of names) {
    if (DENY[engine].has(n)) continue;
    if (engine === "pierre" && pierreLexer.has(n)) return n;
    if (engine === "tanstack" && tanstackName.has(n)) return tanstackName.get(n);
  }
  return undefined;
}

const languages = {};
const known = new Map(); // a name or alias -> id, Shiki's first
for (const info of bundledLanguagesInfo) {
  const names = [info.id, ...(info.aliases ?? [])];
  const entry = { shiki: info.id };
  const p = nameIn("pierre", names, info.id);
  const t = nameIn("tanstack", names, info.id);
  if (p !== undefined) entry.pierre = p;
  if (t !== undefined) entry.tanstack = t;
  languages[info.id] = entry;
  for (const n of names) if (!known.has(n)) known.set(n, info.id);
}

// What each engine has that no Shiki language reached: by Pierre's lexer and by TanStack's definition.
const pierreReached = new Set(Object.values(languages).flatMap((l) => (l.pierre ? [pierreLexer.get(l.pierre)] : [])));
const tanstackReached = new Set(Object.values(languages).flatMap((l) => (l.tanstack ? [l.tanstack] : [])));
const plainLexer = pierreLexer.get("plain");
const unreached = [
  ...new Set([...pierreLexer].filter(([, lexer]) => lexer !== plainLexer && !pierreReached.has(lexer)).map(([, lexer]) => `pierre:${[...pierreLexer].find(([, l]) => l === lexer)[0]}`)),
  ...tanstack.filter((d) => !tanstackReached.has(d.name) && !DENY.tanstack.has(d.name)).map((d) => `tanstack:${d.name}`),
];
for (const [id, entry] of Object.entries(EXTRA)) {
  languages[id] = entry;
  const names = [id, ...[...pierreLexer].filter(([, l]) => entry.pierre !== undefined && l === pierreLexer.get(entry.pierre)).map(([n]) => n), ...(tanstack.find((d) => d.name === entry.tanstack)?.aliases ?? [])];
  for (const n of names) if (!known.has(n) && !DENY.tanstack.has(n)) known.set(n, id);
  known.set(id, id);
}
const missed = unreached.filter((u) => !Object.values(EXTRA).some((e) => u === `pierre:${e.pierre}` || u === `tanstack:${e.tanstack}` || (e.pierre && pierreLexer.get(u.slice(7)) === pierreLexer.get(e.pierre))));
if (missed.length > 0) console.error(`gen-languages: no id for ${missed.join(", ")}: add it to EXTRA or a Shiki alias`);

// File names and extensions knobas has always read (`highlight-core.ts` until 2026-11-21).
const BY_NAME = { dockerfile: "docker", containerfile: "docker", makefile: "make", justfile: "just", "cargo.lock": "toml", "go.mod": "go", ".gitignore": "shellscript", ".env": "dotenv" };
const BY_EXT_OWN = { h: "c", hpp: "cpp", cc: "cpp", htm: "html", svg: "xml", kts: "kotlin", mjs: "javascript", cjs: "javascript", gradle: "groovy" };
// The endings whose files are XML (the XML view's design, 2026-11-23, section 4). Source colours each as XML, and the
// XML view's sniff (`render/xml-sniff.ts`) reads this list from the generated file. An ending a Shiki grammar already
// claims keeps it (`xsl`), and `xslt` is that grammar too.
const XML_ENDINGS = ["xml", "xsd", "xsl", "xslt", "wsdl", "svg", "xhtml", "rss", "atom", "xlf", "xliff", "plist", "csproj", "vbproj", "fsproj", "vcxproj", "props", "targets", "nuspec", "resx", "config", "xaml", "axml", "pom", "jrxml", "fxml", "kml", "gpx", "drawio", "iml", "wxs", "dita", "ditamap", "xmi", "manifest"];
const XML_OWN = { xslt: "xsl" };
const byExt = Object.fromEntries([...known].sort(([a], [b]) => a.localeCompare(b)));
Object.assign(byExt, BY_EXT_OWN);
for (const ending of XML_ENDINGS) if (byExt[ending] === undefined) byExt[ending] = XML_OWN[ending] ?? "xml";
const byFence = Object.fromEntries([...known].sort(([a], [b]) => a.localeCompare(b)));

const sorted = (o) => Object.fromEntries(Object.entries(o).sort(([a], [b]) => a.localeCompare(b)));
/** An object as TypeScript, one entry a line, so a version bump reads as a diff. */
const json = (o) => `{\n${Object.entries(o).map(([k, v]) => `  ${JSON.stringify(k)}: ${JSON.stringify(v).replace(/,"/g, ', "').replace(/":/g, '": ')},`).join("\n")}\n}`;
const text = `/**
 * knobas's languages: each one's name in each highlighter, and what picks one. Generated by
 * \`scripts/gen-languages.mjs\` from shiki ${version("shiki")}, @pierre/highlights ${version("@pierre/highlights")} and
 * @tanstack/highlight ${version("@tanstack/highlight")}; do not edit by hand (\`languages.test.ts\` holds it to the packages).
 *
 * The ids are Shiki's, plus the languages only another engine knows. Importing this file loads no engine.
 */
export type LangId = string;

/** Each language's name in each engine that has it. */
export const LANGUAGES: Readonly<Record<LangId, { readonly pierre?: string; readonly tanstack?: string; readonly shiki?: string }>> = ${json(sorted(languages))};

/** File names with no extension that name their language. */
export const BY_NAME: Readonly<Record<string, LangId>> = ${json(BY_NAME)};

/** Extensions, lower case. */
export const BY_EXT: Readonly<Record<string, LangId>> = ${json(byExt)};

/** The endings whose files are XML, lower case: Source colours each, and the XML view sniffs a file of one. */
export const XML_ENDINGS: readonly string[] = ${JSON.stringify(XML_ENDINGS).replace(/,/g, ", ")};

/** A Markdown fence's language, lower case. */
export const BY_FENCE: Readonly<Record<string, LangId>> = ${json(byFence)};
`;

if (process.argv.includes("--check")) {
  const now = readFileSync(OUT, "utf8");
  if (now !== text) {
    console.error("gen-languages: app/src/lib/render/languages.ts differs from the installed packages; run node scripts/gen-languages.mjs");
    process.exit(1);
  }
  console.log("gen-languages: ok");
} else {
  writeFileSync(OUT, text);
  console.log(`gen-languages: ${Object.keys(languages).length} languages, ${Object.keys(byExt).length} extensions -> ${OUT}`);
}
