/// <reference lib="webworker" />
/**
 * The spec worker (ux18): parses an OpenAPI spec and keeps it, so a 5,000-operation spec's parse and its document
 * never sit on the page's thread; the view asks for the index, then for each piece it opens (`openapi-port.ts`).
 * js-yaml loads here and nowhere else in the app.
 */
import { answerCall, type SpecCall, type SpecFiles } from "./openapi";

const sessions = new Map<number, SpecFiles>();

self.onmessage = (event: MessageEvent<{ id: number; session: number; call: SpecCall }>) => {
  const { id, session, call } = event.data;
  let result: unknown;
  try {
    result = answerCall(sessions, session, call);
  } catch (cause) {
    result = { ok: false, error: cause instanceof Error ? cause.message : String(cause), line: null };
  }
  (self as unknown as Worker).postMessage({ id, result });
};
