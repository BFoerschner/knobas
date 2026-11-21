/**
 * The roles a token can carry (ADR-0051, `docs/decisions/2026-11-21-a-theme-is-yours-and-code-is-coloured-by-two-engines.md`):
 * what a token is to a reader, never a colour. Each role is a class (`.tf-hl-<role>`, `tf/css/code.css`) and a variable
 * (`--hl-<role>`, `tf/css/tokens.css`) a theme sets, so a theme change recolours what is drawn with no second pass.
 *
 * The highlight worker turns each engine's own token names into these: Shiki's scopes through `ROLE_SCOPES`
 * (`highlight-core.ts`), Pierre Highlights' types through {@link PIERRE_ROLE}. Then {@link roleIn} folds a data
 * language's properties, tags and attributes into keys, whichever engine cut them.
 */

/** The seventeen roles a theme colours, in the order the editor lists them. */
export const ROLES = ["com", "kw", "str", "esc", "num", "con", "fn", "ty", "prop", "key", "tag", "attr", "var", "op", "punct", "head", "link"] as const;
export type Role = (typeof ROLES)[number];

/** Two more a token can carry, a shape and never a colour: Markdown source's bold (700) and italic. */
export const SHAPES = ["strong", "emph"] as const;
export type Shape = (typeof SHAPES)[number];

/** A token's role or shape as the port carries it; `""` is plain ink. */
export type TokenRole = Role | Shape | "";

/** The names a person reads (Settings › Themes). */
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
 * Pierre Highlights' token types onto roles. The type is read from the token's colour, `var(--hls-<type>)`, its dots
 * spelt as dashes (pinned by `highlight-core.test.ts`). A type left out is plain. A map, so `constructor` is a type.
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
  ["property-json_key", "key"],
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
  ["punctuation-list_marker", "punct"],
  ["punctuation-markup", "punct"],
  ["title", "head"],
  ["link_text", "link"],
  ["link_uri", "link"],
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
