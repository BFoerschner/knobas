/**
 * What was typed on an application's page and not saved (PR #2479, 2026-11-13), per page (an application's id, or
 * `add`), for the session: leaving the page keeps it, coming back finds it, *Cancel* and *Save* drop it. Its own
 * module, so a test's reset clears it without loading the page and everything it reads.
 */
import type { AppDraft } from "./app-page";

export const typedDrafts = new Map<string, AppDraft>();

/** Tests: forget what was typed. */
export function forgetAppDrafts(): void {
  typedDrafts.clear();
}
