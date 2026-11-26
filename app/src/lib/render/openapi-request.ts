/**
 * A request from the API explorer (ux31, section 14): one model and three writers, so the line under the fields, the
 * curl text, what Rust sends and what a Bruno request holds cannot disagree. Pure: the view, the worker and the tests
 * read it alike.
 *
 * - **`startDraft`** is what an operation's request starts as: each parameter with the value the spec offers (its
 *   `example`, its schema's `example`, `default`, first `enum` value, else empty), the first media type, and its body
 *   from the spec's example, else the example made from the schema.
 * - **`outgoing`** is the one place a request takes shape: the address with its path values encoded (but `:` and `,`,
 *   TeamCity's locators), the ticked query and header rows, the body for a method that carries one, and where a
 *   secret goes, as a {@link Secret} that names it and never holds it.
 * - **`requestLine`** draws a secret as eight dots; **`curl`** as a shell variable's name.
 *
 * No secret's value is ever in a draft, an outgoing request, the line or the curl text: Rust attaches it.
 */
import type { Operation, Param, Scheme, Server } from "./openapi";

/** Where a request goes: a server of the spec with its variables, or one a person saved. */
export type DraftServer = { kind: "spec"; index: number; variables: Record<string, string> } | { kind: "saved"; id: string };

export interface DraftParam {
  name: string;
  in: "path" | "query" | "header";
  required: boolean;
  /** Whether the row is sent. A required one always is. */
  on: boolean;
  value: string;
  /** The parameter's schema says an array: a value of commas is sent as repeated names. */
  array: boolean;
}

/** Manual auth scheme ids (prefixed so they cannot collide with a spec's `securitySchemes` names). */
export const MANUAL_BASIC = "__manual:basic";
export const MANUAL_BEARER = "__manual:bearer";
export const MANUAL_API_KEY = "__manual:apiKey";
export type ManualSchemeId = typeof MANUAL_BASIC | typeof MANUAL_BEARER | typeof MANUAL_API_KEY;

/** Placement and name for a manual API key in Try-it. */
export interface ManualApiKey {
  in: "header" | "query";
  name: string;
}

export const DEFAULT_MANUAL_API_KEY: ManualApiKey = { in: "header", name: "X-API-Key" };

export interface RequestDraft {
  server: DraftServer;
  /** The security scheme signed in with, by spec name, a manual scheme id (`__manual:*`), or `null` for none. */
  scheme: string | null;
  /** Placement and name for manual `API key` auth (defaults to `Header: X-API-Key`). */
  manualKey?: ManualApiKey;
  params: DraftParam[];
  media: string | null;
  body: string;
}

/** Resolves `schemeId` to a spec-declared {@link Scheme} or one of the manual Try-it auth schemes. */
export function resolveScheme(
  schemeId: string | null,
  specSchemes: readonly Scheme[],
  manualKey: ManualApiKey = DEFAULT_MANUAL_API_KEY,
): Scheme | null {
  if (schemeId === null || schemeId === "") return null;
  if (schemeId === MANUAL_BASIC) {
    return { name: MANUAL_BASIC, type: "http", scheme: "basic", in: "", keyName: "", bearerFormat: "", description: "", flows: [] };
  }
  if (schemeId === MANUAL_BEARER) {
    return { name: MANUAL_BEARER, type: "http", scheme: "bearer", in: "", keyName: "", bearerFormat: "", description: "", flows: [] };
  }
  if (schemeId === MANUAL_API_KEY) {
    return { name: MANUAL_API_KEY, type: "apiKey", scheme: "", in: manualKey.in, keyName: manualKey.name.trim(), bearerFormat: "", description: "", flows: [] };
  }
  return specSchemes.find((s) => s.name === schemeId) ?? null;
}

/** Whether credentials typed for `scheme` have something to hold (`Basic` accepts a user name with an empty password). */
export function hasTypedSecret(scheme: Scheme | null, secret: string, user: string): boolean {
  if (scheme === null) return false;
  if (scheme.type === "http" && scheme.scheme === "basic") return user !== "" || secret !== "";
  return secret !== "";
}

/** A secret in an outgoing request: which scheme's, never its value. */
export interface Secret {
  secret: "bearer" | "basic" | "key";
  scheme: string;
  /** A cookie's name, where the key goes in one. */
  cookie?: string;
}

export type HeaderValue = string | Secret;

export interface Outgoing {
  method: string;
  /** The address as it goes out, a query secret not in it (`secretQuery` names it). */
  url: string;
  headers: [string, HeaderValue][];
  body: string | null;
  /** A key that goes in the query: its name, and the scheme it is. */
  secretQuery: { name: string; scheme: string } | null;
  /** The path parameters with no value. */
  missing: string[];
  /** Whether the host is this computer. */
  local: boolean;
  /** Whether the method changes nothing (`GET`, `HEAD`, `OPTIONS`). */
  safe: boolean;
  /** Why it cannot be sent as it stands: no server, a relative address, a variable unfilled. */
  blocked: string | null;
  host: string;
}

type Obj = Record<string, unknown>;
const isObj = (v: unknown): v is Obj => typeof v === "object" && v !== null && !Array.isArray(v);
const text = (v: unknown): string => (v === undefined || v === null ? "" : typeof v === "string" ? v : typeof v === "number" || typeof v === "boolean" ? String(v) : JSON.stringify(v));

/** The value a parameter starts with: its example, its schema's example, default, first enum value, else empty. */
export function startValue(p: Pick<Param, "example" | "schema">, resolved: unknown = p.schema): string {
  if (p.example !== undefined) return text(p.example);
  const s = isObj(resolved) ? resolved : {};
  if (s.example !== undefined) return text(s.example);
  if (s.default !== undefined) return text(s.default);
  if (Array.isArray(s.enum) && s.enum.length > 0) return text(s.enum[0]);
  return "";
}

/** A body's text for `media`: a form's `name=value` lines, JSON indented, a string as it is. */
export function bodyText(media: string, value: unknown): string {
  if (value === undefined || value === null) return "";
  if (/x-www-form-urlencoded|multipart\/form-data/.test(media) && isObj(value)) {
    return Object.entries(value)
      .filter(([, v]) => v !== null && typeof v !== "object")
      .map(([k, v]) => `${k}=${text(v)}`)
      .join("\n");
  }
  return typeof value === "string" ? value : JSON.stringify(value, null, 2);
}

/**
 * The request operation `op` starts as. `bodies` holds each media type's starting value (the spec's example, else the
 * one made from the schema); `resolve` gives a parameter's schema with its ref followed, where the page has it.
 */
export function startDraft(op: Operation, bodies: Record<string, unknown>, firstScheme: string | null, resolve: (schema: unknown) => unknown = (s) => s): RequestDraft {
  const params: DraftParam[] = [];
  for (const p of op.parameters) {
    if (p.ref !== null || (p.in !== "path" && p.in !== "query" && p.in !== "header")) continue;
    const schema = resolve(p.schema);
    const array = isObj(schema) && schema.type === "array";
    params.push({ name: p.name, in: p.in, required: p.required, on: p.required, value: startValue(p, schema), array });
  }
  const content = op.body?.content ?? [];
  const media = content.find((m) => /json/.test(m.type)) ?? content[0] ?? null;
  return {
    server: { kind: "spec", index: 0, variables: {} },
    scheme: firstScheme,
    params,
    media: media?.type ?? null,
    body: media === null ? "" : bodyText(media.type, bodies[media.type]),
  };
}

/** The first scheme an operation accepts, of those the spec defines. */
export function firstSchemeOf(op: Operation, schemes: readonly Scheme[]): string | null {
  for (const req of op.security) for (const name of req) if (schemes.some((s) => s.name === name)) return name;
  return null;
}

/** A server's address with its variables filled: the draft's, else each variable's default. */
export function serverUrl(server: Server, variables: Record<string, string>): string {
  return server.url.replace(/\{([^}]+)\}/g, (whole, name: string) => variables[name] ?? (server.variables[name]?.default || whole));
}

/** A path value encoded for the address, `:` and `,` kept (TeamCity's `id:4711,branch:main`). */
export function encodePathValue(value: string): string {
  return encodeURIComponent(value).replace(/%3A/gi, ":").replace(/%2C/gi, ",");
}

/** Whether `host` is this computer, as Rust reads it (`api/send.rs`): `localhost`, `*.localhost`, `127.0.0.0/8`, `::1`. */
export function isThisComputer(host: string): boolean {
  const h = host.toLowerCase().replace(/\.$/, "");
  const bare = h.replace(/^\[|\]$/g, "");
  return h === "localhost" || h.endsWith(".localhost") || bare === "::1" || /^127\.\d{1,3}\.\d{1,3}\.\d{1,3}$/.test(bare);
}

/** What `method` says: changes nothing. */
export const SAFE = new Set(["get", "head", "options"]);

/** The request as it will go out (see the module's words). `base` is the server's address, its variables filled. */
export function outgoing(op: Operation, draft: RequestDraft, base: string, scheme: Scheme | null): Outgoing {
  const missing: string[] = [];
  let path = op.path;
  for (const p of draft.params.filter((x) => x.in === "path")) {
    if (p.value === "") missing.push(p.name);
    else path = path.split(`{${p.name}}`).join(encodePathValue(p.value));
  }
  const query: string[] = [];
  for (const p of draft.params) {
    if (p.in !== "query" || !(p.on || p.required) || p.value === "") continue;
    const values = p.array ? p.value.split(",").map((v) => v.trim()).filter((v) => v !== "") : [p.value];
    for (const v of values) query.push(`${encodeURIComponent(p.name)}=${encodeURIComponent(v)}`);
  }
  const headers: [string, HeaderValue][] = [];
  for (const p of draft.params) if (p.in === "header" && (p.on || p.required) && p.value !== "") headers.push([p.name, p.value]);
  let secretQuery: Outgoing["secretQuery"] = null;
  let authBlocked: string | null = null;
  if (scheme !== null) {
    if (scheme.type === "apiKey") {
      const keyName = scheme.keyName.trim();
      if (keyName === "") authBlocked = "Enter the API key name.";
      else if (scheme.in === "query") secretQuery = { name: keyName, scheme: scheme.name };
      else if (scheme.in === "header") headers.push([keyName, { secret: "key", scheme: scheme.name }]);
      else if (scheme.in === "cookie") headers.push(["Cookie", { secret: "key", scheme: scheme.name, cookie: keyName }]);
    } else if (scheme.type === "http" && scheme.scheme === "basic") headers.push(["Authorization", { secret: "basic", scheme: scheme.name }]);
    else headers.push(["Authorization", { secret: "bearer", scheme: scheme.name }]);
  }
  const method = op.method.toLowerCase();
  const sends = !SAFE.has(method) && draft.media !== null && draft.body.trim() !== "";
  if (sends) headers.push(["Content-Type", draft.media!]);
  const accept = op.responses.find((r) => r.status.startsWith("2"))?.content[0]?.type;
  if (accept !== undefined && !headers.some(([n]) => n.toLowerCase() === "accept")) headers.push(["Accept", accept]);
  const root = base.replace(/\/+$/, "");
  const url = `${root}${path}${query.length > 0 ? `?${query.join("&")}` : ""}`;
  let host = "";
  let blocked: string | null = null;
  if (root === "") blocked = "The spec names no server. Choose one of yours.";
  else if (!/^https?:\/\//i.test(root)) blocked = `${root} is relative: choose a server of yours for its host.`;
  else if (/\{[^}]+\}/.test(root)) blocked = "Fill in the server's variables.";
  else {
    try {
      host = new URL(url).hostname;
    } catch {
      blocked = `${url} is not an address.`;
    }
  }
  blocked ??= authBlocked;
  return { method: op.method.toUpperCase(), url, headers, body: sends ? draft.body : null, secretQuery, missing, local: host !== "" && isThisComputer(host), safe: SAFE.has(method), blocked, host };
}

const DOTS = "••••••••";

/** A header value as the line draws it: a secret as dots. */
function shown(v: HeaderValue): string {
  if (typeof v === "string") return v;
  return v.secret === "bearer" ? `Bearer ${DOTS}` : v.secret === "basic" ? `Basic ${DOTS}` : v.cookie !== undefined ? `${v.cookie}=${DOTS}` : DOTS;
}

/** The method and the whole address as it will go out, a query secret as dots. */
export function requestLine(out: Outgoing): string {
  const q = out.secretQuery === null ? "" : `${out.url.includes("?") ? "&" : "?"}${encodeURIComponent(out.secretQuery.name)}=${DOTS}`;
  return `${out.method} ${out.url}${q}`;
}

/** The headers as the page draws them, secrets as dots. */
export function shownHeaders(out: Outgoing): [string, string][] {
  return out.headers.map(([n, v]) => [n, shown(v)]);
}

/** A shell word in single quotes, a quote inside as `'\''`. */
export function shellQuote(s: string): string {
  return `'${s.replace(/'/g, `'\\''`)}'`;
}

/**
 * The request as one curl command, a line an option (section 21). A secret is a variable's name, never its value:
 * `-H "Authorization: Bearer $TOKEN"`, `-u "$USER:$PASSWORD"`, `$API_KEY` in a query or a header.
 */
export function curl(out: Outgoing): string {
  const url = out.secretQuery === null ? shellQuote(out.url) : `"${out.url.replace(/(["\\$`])/g, "\\$1")}${out.url.includes("?") ? "&" : "?"}${encodeURIComponent(out.secretQuery.name)}=$API_KEY"`;
  const lines = [`curl -X ${out.method} ${url}`];
  for (const [name, value] of out.headers) {
    if (typeof value === "string") lines.push(`  -H ${shellQuote(`${name}: ${value}`)}`);
    else if (value.secret === "basic") lines.push(`  -u "$USER:$PASSWORD"`);
    else if (value.secret === "bearer") lines.push(`  -H "${name}: Bearer $TOKEN"`);
    else lines.push(`  -H "${name}: ${value.cookie !== undefined ? `${value.cookie}=` : ""}$API_KEY"`);
  }
  if (out.body !== null) lines.push(`  --data ${shellQuote(out.body)}`);
  return lines.join(" \\\n");
}

/** Where Rust attaches the held secret of `scheme` (`api/send.rs`'s `AuthPlace`), and the name it goes by. */
export function authPlace(scheme: Scheme): { place: "bearer" | "basic" | "header" | "query" | "cookie"; name: string } {
  if (scheme.type === "apiKey") return { place: scheme.in === "query" ? "query" : scheme.in === "cookie" ? "cookie" : "header", name: scheme.keyName };
  if (scheme.type === "http" && scheme.scheme === "basic") return { place: "basic", name: "" };
  return { place: "bearer", name: "" };
}
