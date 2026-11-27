/**
 * Text written into XML the app makes (an example made from a schema or an OpenAPI schema, a wrapper schema for a
 * check): `&`, `<` and `>` as references, and in an attribute value `"` and a line break too, so a parser reads back
 * the value as it was (a raw line break in an attribute is read as a space).
 */
export function xmlText(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/** {@link xmlText} for a value in double quotes. */
export function xmlAttr(s: string): string {
  return xmlText(s).replace(/"/g, "&quot;").replace(/\n/g, "&#10;");
}
