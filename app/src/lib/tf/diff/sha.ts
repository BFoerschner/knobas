/**
 * The shell's sha checks, one place (review of spec #2234, Standards 10). They
 * mirror `knobas_core::checkout::open::{is_sha, is_full_sha}`: a sha names a
 * commit by 7 to 64 hex digits; a whole one is SHA-1's 40 or SHA-256's 64,
 * the only kind that fixes what a read answers.
 */

/** Whether `sha` is 7 to 64 hex digits. */
export function isSha(sha: string): boolean {
  return /^[0-9a-f]{7,64}$/i.test(sha);
}

/** Whether `sha` is a whole object id: 40 or 64 hex digits. */
export function isFullSha(sha: string): boolean {
  return (sha.length === 40 || sha.length === 64) && isSha(sha);
}

/** `end` without the one `^` a diff's base may end in, and that caret (`""` for none). */
export function caretOf(end: string): [stem: string, caret: string] {
  return end.endsWith("^") ? [end.slice(0, -1), "^"] : [end, ""];
}

/** A diff end shortened as the source shortens a whole sha (10 digits), its caret kept; anything else as written. */
export function shortEnd(end: string): string {
  const [stem, caret] = caretOf(end);
  return isFullSha(stem) ? `${stem.slice(0, 10)}${caret}` : end;
}
