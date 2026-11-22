/**
 * The API explorer's sends (ux31, section 30), answered in the page: **fixture-only**. The real send is Rust's
 * (`crates/knobas-app/src/api/`), witnessed against servers of its tests' own in `tests/it/api_send.rs`.
 *
 * `api_send` answers after 40 to 230 ms (by the address's length) with a JSON echo of what came: `201` for a `POST`,
 * `204` for a `DELETE`, else `200`. `?fake-api=401` answers *Unauthorized*, `refused` a refused connection, `slow`
 * takes 2.6 s (so *Stop* can be pressed). A refusal the app makes before sending is made here too: a request that
 * changes something on a host that is not this computer, without the page's yes.
 */
import type { ApiAnswer, ApiSend, SavedServers, SentLine } from "../../../ipc/api";
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
keepSeed(fakeSent, fakeBodies, fakeHeld, fakeServers, stopped);

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
  };
}
