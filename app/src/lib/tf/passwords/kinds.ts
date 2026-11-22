/**
 * The Passwords role's two kinds (ADR-0056), as `knobas_source::passwords`
 * names them: what the launcher, `itemHash` and the page tell an entry and a
 * folder by.
 */
export const ENTRY_KIND = "password_entry";
export const FOLDER_KIND = "password_folder";

/** Whether `kind` is one of the Passwords role's: no link, no timer, and a page of its own. */
export function isPasswordKind(kind: string): boolean {
  return kind === ENTRY_KIND || kind === FOLDER_KIND;
}
