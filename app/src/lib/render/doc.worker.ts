/// <reference lib="webworker" />
/**
 * The document worker (the XML view's design, 2026-11-23, section 27): reads an XML file into its outline and keeps
 * it, so a 50 MB file's read and its columns never sit on the page's thread. The view asks for the fold table once,
 * then for what each pick or step needs (`doc-port.ts`).
 */
import { answerDoc, type DocCall, type DocSessions } from "./doc-model";

const sessions: DocSessions = new Map();

self.onmessage = (event: MessageEvent<{ id: number; session: number; call: DocCall }>) => {
  const { id, session, call } = event.data;
  let result: unknown;
  try {
    result = answerDoc(sessions, session, call);
  } catch (cause) {
    result = { failed: cause instanceof Error ? cause.message : String(cause) };
  }
  (self as unknown as Worker).postMessage({ id, result });
};
