/// <reference lib="webworker" />
/**
 * The spec worker (ux18): parses an OpenAPI spec and keeps it, so a 5,000-operation spec's parse and its document
 * never sit on the page's thread; the view asks for the index, then for each piece it opens (`openapi-port.ts`).
 * js-yaml loads here and nowhere else in the app.
 */
import { answerCall, type Opened, type SpecCall, type SpecFiles } from "./openapi";
import { loadFakes } from "./openapi-fakes";
import { packOpened } from "./openapi-pack";

const sessions = new Map<number, SpecFiles>();

// Faker and randexp, for examples made from a schema (`openapi-fakes.ts`): loaded with the worker, off the page's
// thread, while the first spec parses; only a call that makes an example waits for them.
const fakes = loadFakes(self.navigator?.language).catch(() => {});

self.onmessage = async (event: MessageEvent<{ id: number; session: number; call: SpecCall }>) => {
  const { id, session, call } = event.data;
  if (call.kind === "page" || call.kind === "sample") await fakes;
  let result: unknown;
  try {
    result = answerCall(sessions, session, call);
    // An open's rows cross as columns (`openapi-pack.ts`): cheaper for the page's thread to take.
    if (call.kind === "open") result = packOpened(result as Opened);
  } catch (cause) {
    result = { ok: false, error: cause instanceof Error ? cause.message : String(cause), line: null };
  }
  (self as unknown as Worker).postMessage({ id, result });
};
