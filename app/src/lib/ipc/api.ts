/**
 * The API explorer's way out — `crates/knobas-app/src/commands/api.rs` (ux31, ADR-0057). The types are generated from
 * the Rust (`./generated/api.ts`); the `invoke` functions stay hand-written. Nothing here ever answers a secret: a
 * secret typed on the page is handed to the app once (`apiHoldSecret`) and the page keeps none.
 */
import { invoke } from "./invoke";
import type { ApiAnswer, ApiAuth, ApiSend, AuthPlace, Hop, SavedServer, SavedServers, SentLine } from "./generated/api";

export type { ApiAnswer, ApiAuth, ApiSend, AuthPlace, Hop, SavedServer, SavedServers, SentLine };

/** Sends one request, once. A refusal before anything was sent rejects; a failure is an answer. */
export function apiSend(send: ApiSend): Promise<ApiAnswer> {
  return invoke<ApiAnswer>("api_send", { send });
}

/** Stops send `id` at once. */
export function apiCancel(id: number): Promise<void> {
  return invoke<void>("api_cancel", { id });
}

/** What `api_answer_body` answers: an `ArrayBuffer` from the bridge, bytes from the dev fake. */
type RawBytes = ArrayBuffer | ArrayLike<number>;

/** The body of send `id`, JSON indented where `pretty`; rejects once it is no longer kept. */
export async function apiAnswerBody(id: number, pretty: boolean): Promise<Uint8Array> {
  const answer = await invoke<RawBytes>("api_answer_body", { id, pretty });
  return answer instanceof ArrayBuffer ? new Uint8Array(answer) : Uint8Array.from(answer);
}

/** What was sent from `file` since knobas started, the newest first. */
export function apiSent(file: string): Promise<SentLine[]> {
  return invoke<SentLine[]>("api_sent", { file });
}

/** Holds a secret typed on the page for `origin` and `scheme`, until knobas quits. */
export function apiHoldSecret(origin: string, scheme: string, secret: string, user: string | null): Promise<void> {
  return invoke<void>("api_hold_secret", { origin, scheme, secret, user });
}

/** Which schemes a secret is held for at `origin`: names only. */
export function apiHeld(origin: string): Promise<string[]> {
  return invoke<string[]>("api_held", { origin });
}

/** Forgets every secret held for `origin`. */
export function apiForgetHeld(origin: string): Promise<void> {
  return invoke<void>("api_forget_held", { origin });
}

/** The saved servers. */
export function apiServers(): Promise<SavedServers> {
  return invoke<SavedServers>("api_servers", {});
}

/** Saves the whole list of servers; answers it as stored. */
export function apiSetServers(servers: SavedServers): Promise<SavedServers> {
  return invoke<SavedServers>("api_set_servers", { servers });
}

/** Saves the body of send `id` to the Downloads folder under `name`; answers the path. */
export function apiSaveAnswer(id: number, name: string): Promise<string> {
  return invoke<string>("api_save_answer", { id, name });
}
