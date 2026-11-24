/// <reference lib="webworker" />
/**
 * The schema worker (the XSD design, 2026-11-23, section 6): reads a schema set and keeps it, so reading, compiling and
 * every page's rows stay off the page's thread. The page asks through `xsd-port.ts`.
 */
import { answerXsd, type XsdCall, type XsdSessions } from "./xsd-answer";
import { BUNDLED } from "./xsd-known";

const sessions: XsdSessions = new Map();

self.onmessage = (event: MessageEvent<{ id: number; session: number; call: XsdCall }>) => {
  const { id, session, call } = event.data;
  let result: unknown;
  try {
    result = answerXsd(sessions, session, call, BUNDLED);
  } catch (cause) {
    result = { ok: false, error: cause instanceof Error ? cause.message : String(cause), line: null };
  }
  (self as unknown as Worker).postMessage({ id, result });
};
