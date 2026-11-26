/**
 * The roles a token can carry (ADR-0051, `docs/decisions/2026-11-21-a-theme-is-yours-and-code-is-coloured-by-two-engines.md`):
 * what a token is to a reader, never a colour. Each role is a class (`.tf-hl-<role>`, `tf/css/code.css`) and a variable
 * (`--hl-<role>`, `tf/css/tokens.css`) a theme sets, so a theme change recolours what is drawn with no second pass.
 *
 * One way in for every engine: an engine cuts text into tokens with names of its own, its {@link RoleMap} turns a name
 * into a role, and {@link tokenRole} folds a data language's properties, tags and attributes into keys, whichever
 * engine cut them. Adding an engine is adding its map to {@link ROLE_MAPS}. Shiki's names are the colours its one theme
 * gives each scope (`var(--hl-<role>)`, from `ROLE_SCOPES` in `highlight-core.ts`); Pierre Highlights' are its token
 * types ({@link PIERRE_ROLE}).
 */

/** The seventeen roles a theme colours, in the order the editor lists them. */
export const ROLES = ["com", "kw", "str", "esc", "num", "con", "fn", "ty", "prop", "key", "tag", "attr", "var", "op", "punct", "head", "link"] as const;
export type Role = (typeof ROLES)[number];

/** Two more a token can carry, a shape and never a colour: Markdown source's bold (700) and italic. */
export const SHAPES = ["strong", "emph"] as const;
export type Shape = (typeof SHAPES)[number];

/** A token's role or shape as the port carries it; `""` is plain ink. */
export type TokenRole = Role | Shape | "";

/** The names a person reads (Settings › Appearance › Code colours). */
export const ROLE_NAMES: Record<Role, string> = {
  com: "Comment",
  kw: "Keyword",
  str: "String",
  esc: "Escape, regex",
  num: "Number",
  con: "Constant",
  fn: "Function",
  ty: "Type",
  prop: "Property",
  key: "Key",
  tag: "Tag",
  attr: "Attribute, annotation",
  var: "Variable, parameter",
  op: "Operator",
  punct: "Punctuation",
  head: "Heading",
  link: "Link",
};

/**
 * Pierre Highlights' token types onto roles (the highlighter re-ruling, 2026-11-21, section 5). The type is read from the
 * token's colour, `var(--hls-<type>)`, where the package writes a type's dots and underscores as dashes (`link_text` is
 * `link-text`; pinned by `engines/pierre.test.ts`). A type left out is plain (`none`, `variable`, `namespace`, `embedded`,
 * `primary`, `text-jsx`, `markup-strikethrough`, `diff-plus`, `diff-minus`, `background`, `foreground`). A map, so
 * `constructor` is a type.
 */
export const PIERRE_ROLE: ReadonlyMap<string, TokenRole> = new Map<string, TokenRole>([
  ["comment", "com"],
  ["comment-doc", "com"],
  ["predictive", "com"],
  ["hint", "com"],
  ["keyword", "kw"],
  ["keyword-control", "kw"],
  ["keyword-declaration", "kw"],
  ["keyword-import", "kw"],
  ["keyword-operator", "kw"],
  ["keyword-jsdoc", "kw"],
  ["variable-special", "kw"],
  ["string", "str"],
  ["string-special", "str"],
  ["text-literal", "str"],
  ["string-escape", "esc"],
  ["string-regex", "esc"],
  ["punctuation-special", "esc"],
  ["number", "num"],
  ["boolean", "con"],
  ["constant", "con"],
  ["constant-builtin", "con"],
  ["string-special-symbol", "con"],
  ["label", "con"],
  ["variant", "con"],
  ["function", "fn"],
  ["function-definition", "fn"],
  ["function-method", "fn"],
  ["type", "ty"],
  ["type-builtin", "ty"],
  ["type-class", "ty"],
  ["type-jsdoc", "ty"],
  ["constructor", "ty"],
  ["enum", "ty"],
  ["tag-component-jsx", "ty"],
  ["property", "prop"],
  ["property-json-key", "key"],
  ["tag", "tag"],
  ["tag-jsx", "tag"],
  ["selector", "tag"],
  ["selector-class", "tag"],
  ["selector-id", "tag"],
  ["selector-pseudo", "tag"],
  ["attribute", "attr"],
  ["attribute-jsx", "attr"],
  ["preproc", "attr"],
  ["tag-doctype", "attr"],
  ["variable-parameter", "var"],
  ["variable-jsdoc", "var"],
  ["operator", "op"],
  ["punctuation", "punct"],
  ["punctuation-bracket", "punct"],
  ["punctuation-bracket-html", "punct"],
  ["punctuation-bracket-jsx", "punct"],
  ["punctuation-delimiter", "punct"],
  ["punctuation-delimiter-html", "punct"],
  ["punctuation-delimiter-jsx", "punct"],
  ["punctuation-list-marker", "punct"],
  ["punctuation-markup", "punct"],
  ["title", "head"],
  ["link-text", "link"],
  ["link-uri", "link"],
  ["emphasis", "emph"],
  ["emphasis-strong", "strong"],
  ["emphasis-strong-italic", "strong"],
]);

/** A Pierre type's role: `punctuation-*` but `-special` is punctuation, anything not in the table plain. */
export function pierreRole(type: string): TokenRole {
  const role = PIERRE_ROLE.get(type);
  if (role !== undefined) return role;
  return type.startsWith("punctuation-") ? "punct" : "";
}

/** Languages (Shiki's ids) whose properties, tags and attributes are keys: data, not code. */
export const DATA_LANGS: ReadonlySet<string> = new Set(["json", "jsonc", "json5", "yaml", "properties", "ini", "toml"]);

/** What `role` is in `lang`: in a data language a property, tag or attribute is a key. */
export function roleIn(lang: string, role: TokenRole): TokenRole {
  return DATA_LANGS.has(lang) && (role === "prop" || role === "tag" || role === "attr") ? "key" : role;
}

/**
 * TanStack Highlight's twenty classes onto roles (section 5). It has no class for an escape, a key or punctuation: an
 * escape stays in its string's colour, punctuation is ink, and the data-language fold makes the keys. Typed by the
 * package's closed union, so `svelte-check` fails when the union changes.
 */
export const TANSTACK_ROLE: Readonly<Record<import("@tanstack/highlight/core").HighlightTokenClass, TokenRole>> = {
  comment: "com",
  keyword: "kw",
  string: "str",
  "code-inline": "str",
  number: "num",
  literal: "con",
  function: "fn",
  command: "fn",
  type: "ty",
  property: "prop",
  tag: "tag",
  selector: "tag",
  attr: "attr",
  meta: "attr",
  variable: "var",
  operator: "op",
  heading: "head",
  link: "link",
  inserted: "",
  deleted: "",
};

/** The engines that cut text into tokens: a setting picks one, and a fixed order says who draws what it cannot (ADR-0051). */
export type EngineId = "pierre" | "tanstack" | "shiki";

/** How one engine's token names become roles. */
export interface RoleMap {
  readonly engine: EngineId;
  /** The role of one of the engine's token names; `""` for one that draws plain. */
  role(name: string): TokenRole;
}

const ROLE_SET: ReadonlySet<string> = new Set<string>([...ROLES, ...SHAPES]);

/** Each engine's map. */
export const ROLE_MAPS: Readonly<Record<EngineId, RoleMap>> = {
  // Shiki's one theme colours a scope `var(--hl-<role>)`: the name is that colour.
  shiki: {
    engine: "shiki",
    role(name) {
      if (!name.startsWith("var(--hl-")) return "";
      const role = name.slice("var(--hl-".length, -1);
      return ROLE_SET.has(role) ? (role as TokenRole) : "";
    },
  },
  // Pierre Highlights' `cssVariables` theme colours a token `var(--hls-<type>)`.
  pierre: { engine: "pierre", role: (name) => (name.startsWith("var(--hls-") ? pierreRole(name.slice("var(--hls-".length, -1)) : pierreRole(name)) },
  // TanStack's token carries its class.
  tanstack: { engine: "tanstack", role: (name) => (Object.hasOwn(TANSTACK_ROLE, name) ? TANSTACK_ROLE[name as keyof typeof TANSTACK_ROLE] : "") },
};

/** The role a token `engine` named `name` carries in `lang`. */
export function tokenRole(engine: EngineId, name: string, lang: string): TokenRole {
  return roleIn(lang, ROLE_MAPS[engine].role(name));
}
