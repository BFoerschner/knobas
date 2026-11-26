/**
 * The API explorer's sends (ux31, section 30), answered in the page: **fixture-only**. The real send is Rust's
 * (`crates/knobas-app/src/api/`), witnessed against servers of its tests' own in `tests/it/api_send.rs`.
 *
 * `api_send` answers after 40 to 230 ms (by the address's length) with a JSON echo of what came: `201` for a `POST`,
 * `204` for a `DELETE`, else `200`. `?fake-api=401` answers *Unauthorized*, `refused` a refused connection, `slow`
 * takes 2.6 s (so *Stop* can be pressed). A refusal the app makes before sending is made here too: a request that
 * changes something on a host that is not this computer, without the page's yes.
 */
import type { ApiAnswer, ApiSend, JsonQueryAnswer, SavedServers, SchemaLocations, SentLine, SpecToOpen, XmlCheck } from "../../../ipc/api";
import { fakeValuePiped, repoFileBytes } from "./views/repos";
import type { PipeStep } from "../../../ipc/entity";
import type { Handlers } from "../fake-tauri";
import { keepSeed } from "../fake-seed";

/** The fake's kept sends and their bodies, newest last. */
const fakeSent: SentLine[] = [];
const fakeBodies = new Map<number, Uint8Array>();
/** Origins and schemes a secret is held for. Never the secret: the fake throws it away. */
const fakeHeld = new Map<string, Set<string>>();
const fakeServers: SavedServers = { servers: [{ id: "tidewater-teamcity", name: "Tidewater TeamCity", url: "https://teamcity.tidewater.test", ca_file: null }], last: {} };
/** Sends *Stop* was pressed for. */
const stopped = new Set<number>();
/** The hosts whose images load when their dialog opens (the XML view, section 14). */
const fakeImageHosts: { hosts: string[] } = { hosts: [] };
/** The schema check's two rows (ADR-0058): checking on, nothing pointed to. */
const fakeSchemas: { check: XmlCheck; locations: SchemaLocations } = { check: { on: true }, locations: { pointed: [] } };
keepSeed(fakeSent, fakeBodies, fakeHeld, fakeServers, stopped, fakeImageHosts, fakeSchemas);

/** The fake's jq sessions: each document's value, parsed once. */
const fakeJqDocs = new Map<number, unknown>();
keepSeed(fakeJqDocs);
function fakeJqOpen(text: string | null, entityId: string | null, path: string | null): number {
  const id = fakeJqDocs.size + 1;
  let doc: unknown = null;
  try {
    const bytes = text === null && entityId !== null && path !== null ? repoFileBytes(entityId, path) : undefined;
    doc = JSON.parse(text ?? (bytes === undefined ? "null" : new TextDecoder().decode(bytes)));
  } catch {
    doc = null;
  }
  fakeJqDocs.set(id, doc);
  return id;
}
/**
 * A few of jq's filters, enough for the dev fake's probes: `.`, `.a.b`, `[n]`, `[]`, `| length`, `| select(.k > n)`,
 * `| map(.k)`, `| add`. Others say the fake has no jq.
 */
function fakeJq(id: number, filter: string): JsonQueryAnswer {
  const t0 = performance.now();
  type At = { v: unknown; path: (string | number)[] | null };
  let at: At[] = [{ v: fakeJqDocs.get(id), path: [] }];
  for (const stage of filter.split("|").map((p) => p.trim())) {
    const sel = /^select\(\.(\w+) > (-?[\d.]+)\)$/.exec(stage);
    const map = /^map\(\.(\w+)\)$/.exec(stage);
    if (sel !== null) at = at.filter((a) => typeof (a.v as Record<string, unknown> | null)?.[sel[1]!] === "number" && (a.v as Record<string, number>)[sel[1]!]! > Number(sel[2]));
    else if (stage === "length") at = at.map((a) => ({ v: Array.isArray(a.v) ? a.v.length : Object.keys((a.v as object | null) ?? {}).length, path: null }));
    else if (stage === "add") at = at.map((a) => ({ v: (a.v as number[]).reduce((s, x) => s + x, 0), path: null }));
    else if (map !== null) at = at.map((a) => ({ v: (a.v as Record<string, unknown>[]).map((x) => x[map[1]!]), path: null }));
    else if (/^(\.\w+|\[\d*\]|\.)+$/.test(stage)) {
      for (const step of stage.match(/\.\w+|\[\d*\]/g) ?? []) {
        if (step === "[]") at = at.flatMap((a) => (Array.isArray(a.v) ? a.v.map((v: unknown, i: number) => ({ v, path: a.path === null ? null : [...a.path, i] })) : []));
        else if (step.startsWith("[")) at = at.map((a) => ({ v: (a.v as unknown[] | null)?.[Number(step.slice(1, -1))], path: a.path === null ? null : [...a.path, Number(step.slice(1, -1))] }));
        else at = at.map((a) => ({ v: (a.v as Record<string, unknown> | null)?.[step.slice(1)], path: a.path === null ? null : [...a.path, step.slice(1)] }));
      }
    } else return { state: "failed", why: `The dev fake reads no jq like ${stage}.`, ms: 0 };
  }
  const ms = Math.round(performance.now() - t0);
  const picked = at.every((a) => a.path !== null);
  return { state: "answered", values: at.slice(0, 500).map((a) => JSON.stringify(a.v)), count: at.length, paths: picked ? at.slice(0, 5000).map((a) => a.path!) : null, ms };
}

const local = (host: string) => /^(localhost|127\.\d+\.\d+\.\d+|\[::1\])$/i.test(host) || host.endsWith(".localhost");

export function handlers(params: URLSearchParams = new URLSearchParams()): Handlers {
  const how = params.get("fake-api");
  return {
    // The bridge awaits what a handler answers; this one waits as a server would, so *Stop* can be pressed.
    api_send: (async (args: Record<string, unknown>) => {
      const send = args["send"] as ApiSend;
      const url = new URL(send.url);
      const method = send.method.toUpperCase();
      if (!["GET", "HEAD", "OPTIONS"].includes(method) && !local(url.hostname) && send.confirmed_host !== url.hostname) {
        throw { code: "invalid", message: `${method} changes something on ${url.hostname}, and knobas was not told to send it.` };
      }
      if (send.auth != null && send.auth.origin !== url.origin) {
        throw { code: "invalid", message: `This secret is kept for ${new URL(send.auth.origin).hostname}. The request goes to ${url.hostname}.` };
      }
      const ms = how === "slow" ? 2600 : 40 + ((send.url.length * 37) % 190);
      const t0 = Date.now();
      while (Date.now() - t0 < ms && !stopped.has(send.id)) await new Promise((go) => setTimeout(go, 20));
      let answer: ApiAnswer;
      let body: Uint8Array | null = null;
      if (stopped.has(send.id)) {
        stopped.delete(send.id);
        answer = { kind: "failed", id: send.id, stage: "stopped", words: "Stopped. The server may still have received the request." };
      } else if (how === "refused") {
        answer = { kind: "failed", id: send.id, stage: "connect", words: `No answer from ${url.host}: the connection was refused.` };
      } else {
        const status = how === "401" ? 401 : method === "POST" ? 201 : method === "DELETE" ? 204 : 200;
        const value = status === 401 ? { message: "Bad credentials" } : status === 204 ? null : { method, path: url.pathname, query: Object.fromEntries(url.searchParams), received: JSON.parse(send.body ?? "null") as unknown, signed_in: send.auth != null };
        body = new TextEncoder().encode(value === null ? "" : JSON.stringify(value));
        answer = {
          kind: "answered", id: send.id, status, reason: status === 401 ? "Unauthorized" : status === 201 ? "Created" : status === 204 ? "No Content" : "OK", version: "HTTP/1.1",
          headers: [["content-type", "application/json"], ["content-length", String(body.length)], ["date", "Sun, 22 Nov 2026 14:05:12 GMT"]],
          bytes: body.length, wire_bytes: body.length, to_headers_ms: Math.round(ms * 0.8), total_ms: ms, hops: [], cut: false, follow: null, content_type: "application/json",
        };
      }
      if (body !== null) fakeBodies.set(send.id, body);
      fakeSent.push({ id: send.id, at_ms: Date.now(), send, answer, kept: body !== null });
      while (fakeSent.length > 50) fakeSent.shift();
      return answer;
    }) as unknown as (args: Record<string, unknown>) => ApiAnswer,
    api_cancel: (args) => void stopped.add(Number(args["id"])),
    api_answer_body: (args) => {
      const body = fakeBodies.get(Number(args["id"]));
      if (body === undefined) throw { code: "not_found", message: "The answer is no longer kept." };
      if (args["pretty"] === true) {
        try {
          return new TextEncoder().encode(JSON.stringify(JSON.parse(new TextDecoder().decode(body)), null, 2));
        } catch {
          return body;
        }
      }
      return body;
    },
    // An answer through a pipeline (#2533): the kept body's text through the steps the fake runs on a value.
    api_answer_piped: (args) => {
      const body = fakeBodies.get(Number(args["id"]));
      if (body === undefined) throw { code: "not_found", message: "The answer is no longer kept." };
      return fakeValuePiped(new TextDecoder().decode(body), (args["steps"] as PipeStep[] | undefined) ?? []);
    },
    api_answer_piped_save: (args) => `~/Downloads/${String(args["saveAs"])}`,
    api_sent: (args) => fakeSent.filter((s) => s.send.file === args["file"]).reverse(),
    api_hold_secret: (args) => {
      const origin = String(args["origin"]);
      if (!fakeHeld.has(origin)) fakeHeld.set(origin, new Set());
      fakeHeld.get(origin)!.add(String(args["scheme"]));
    },
    api_held: (args) => [...(fakeHeld.get(String(args["origin"])) ?? [])].sort(),
    api_forget_held: (args) => void fakeHeld.delete(String(args["origin"])),
    api_servers: () => structuredClone(fakeServers),
    api_set_servers: (args) => {
      const next = args["servers"] as SavedServers;
      fakeServers.servers = next.servers.map((s, i) => ({ ...s, id: s.id || `server-${i + 1}` }));
      fakeServers.last = next.last;
      return structuredClone(fakeServers);
    },
    api_save_answer: (args) => `~/Downloads/${String(args["name"])}`,
    // An image at an address (the XML view, section 14): the fake has no network, and says so.
    image_fetch: () => {
      throw { code: "invalid", message: "The image could not be loaded: the dev fake reaches no address.", source_id: null };
    },
    image_hosts: () => structuredClone(fakeImageHosts),
    xml_check: () => structuredClone(fakeSchemas.check),
    set_xml_check: (args) => {
      fakeSchemas.check = { on: (args["check"] as XmlCheck).on };
      return structuredClone(fakeSchemas.check);
    },
    xsd_locations: () => structuredClone(fakeSchemas.locations),
    xsd_set_locations: (args) => {
      fakeSchemas.locations = structuredClone(args["list"] as SchemaLocations);
      return structuredClone(fakeSchemas.locations);
    },
    // The fixture's other repositories hold no schema: another repository answers nothing.
    files_declaring: () => [],
    // The dev fake reaches no address: a download says so, and nothing was ever downloaded.
    xsd_fetch: () => {
      throw { code: "invalid", message: "The schema could not be downloaded: the dev fake reaches no address.", source_id: null };
    },
    xsd_downloaded: () => null,
    xsd_forget: () => undefined,
    xsd_save_example: (args) => `~/Downloads/${String(args["name"])}`,
    // A value of a document held in memory (E1): taken out of the text as a file's value is.
    text_piped: (args) => fakeValuePiped(String(args["text"]), (args["steps"] as PipeStep[] | undefined) ?? []),
    text_piped_save: (args) => `~/Downloads/${String(args["saveAs"])}`,
    // The dev fake has no jq: a session answers the path filters `fakeJq` reads, and says so for others.
    json_query_open: (args) => fakeJqOpen(typeof args["text"] === "string" ? args["text"] : null, typeof args["entityId"] === "string" ? args["entityId"] : null, typeof args["path"] === "string" ? args["path"] : null),
    json_query: (args) => fakeJq(Number(args["id"]), String(args["filter"])),
    json_query_close: () => undefined,
    set_image_hosts: (args) => {
      fakeImageHosts.hosts = [...(args["hosts"] as typeof fakeImageHosts).hosts];
      return structuredClone(fakeImageHosts);
    },
    // Open in an API client (ux31, section 23): the argument vector, as `open_checkout` answers; nothing runs.
    // `?fake-no-bruno` answers that Bruno is not on this computer.
    api_open_in: (args) => {
      const spec = args["spec"] as SpecToOpen;
      const folder = `~/knobas/api-clients/${spec.name.replace(/[^A-Za-z0-9_-]+/g, "-")}-1a2b3c4d`;
      return { argv: ["open", "-a", "Bruno", folder], folder, wrote: true, left_out: [] };
    },
    api_clients_found: () => ({ bruno: !params.has("fake-no-bruno") }),
    api_scratch_size: () => 0,
    api_scratch_clear: () => undefined,
  };
}
