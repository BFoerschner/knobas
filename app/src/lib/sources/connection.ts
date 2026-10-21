/**
 * What a *Test connection* result reads as, wherever one is shown.
 *
 * Two surfaces draw the same report -- the Add-source dialog on a draft and a
 * saved source's row from its *Test* action (#326) -- and one rule for the line
 * is what keeps them from drifting: the dialog used to hold this as a
 * `$derived` of its own, and the row would have been a second copy.
 */
import { ipcErrorMessage, isIpcError } from "../ipc";
import type { ConnectionReport } from "../ipc/sources";

/**
 * *"Connected as ‹account› · ‹version› · ‹ms› ms"*, minus what is absent; the
 * error line when the test failed.
 *
 * The three readings depend on P4 and may be `null`. An absent one is simply
 * not written -- never "null", never an orphaned separator.
 */
export function connectionLine(report: ConnectionReport): string {
  if (!report.ok) return report.error ?? "The connection failed.";
  return [
    report.account ? `Connected as ${report.account}` : "Connected",
    report.server_version,
    `${report.elapsed_ms} ms`,
  ]
    .filter((part): part is string => Boolean(part))
    .join(" · ");
}

/**
 * The connection note to show beneath the line, or `null` for no element.
 *
 * Only when it connected: a failed test's one line is `error`, and a note the
 * adapter attached before failing (none does today) would read as a verdict
 * about a far end that was never reached.
 */
export function connectionNote(report: ConnectionReport): string | null {
  return report.ok && report.detail ? report.detail : null;
}

/**
 * The note for a dialog whose *Test connection* has just filled fields (#1479).
 *
 * The adapter words its note from the draft it was handed: a discovered id the
 * draft did not carry reads "‹id› found but not configured: ...", which is the
 * warning a saved source's *Test* needs (#598, #640). In the Add-source dialog
 * the same click then writes that id into the empty field, so the warning is
 * stale the moment it is drawn. Where the report's note names a value this
 * click filled, that clause reads "‹id› found and set". A note about a value
 * already in the field (or a saved source's, which nothing fills) is the
 * report's own.
 *
 * The match is the adapter's literal sentence: `epic_link_note` in
 * `crates/knobas-source-jira/src/source.rs`, pinned there by its unit test. Reword
 * one and the other, and the fake in `shell/dev/fake/sources.ts`, move together.
 */
export function noteAfterFill(note: string | null, filled: Record<string, string>): string | null {
  if (note === null) return null;
  let out = note;
  for (const value of new Set(Object.values(filled))) {
    const at = out.indexOf(`${value} found but not configured`);
    if (at < 0) continue;
    out = `${out.slice(0, at)}${value} found and set`;
  }
  return out;
}

/**
 * A `test_source` that **rejected**, as a report -- so the surface draws it
 * the way it draws `ok: false`: the message as the line, the code kept for
 * the one branch on it (`unauthorized` is what offers *Re-enter*). A saved
 * source whose credential is gone rejects rather than answering, and so does
 * a bridge that is not there at all.
 */
export function failedReport(cause: unknown): ConnectionReport {
  return {
    ok: false,
    account: null,
    server_version: null,
    secret_expires_at: null,
    error: ipcErrorMessage(cause),
    code: isIpcError(cause) ? cause.code : null,
    elapsed_ms: 0,
    detail: null,
    discovered: {},
  };
}
