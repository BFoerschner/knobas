/**
 * The optional second credential, as both forms that offer one read it (#452).
 *
 * One rule in one place, because the Add-source dialog and the re-enter strip
 * were asserting it separately and a rule stated twice is a rule that can
 * disagree with itself: **both halves or neither**. A username with no password
 * logs in to nothing, so a half-filled pair is refused at the form rather than
 * stored as a credential that can only fail.
 *
 * `CONTEXT.md`, **Account**: a second credential stored beside a source's
 * ordinary one, in the same keychain item, for a source whose write channel its
 * ordinary credential cannot open.
 */
import type { SecretAccount } from "../ipc/sources";

/** What a form has typed into its two account fields. */
export interface AccountFields {
  username: string;
  password: string;
}

/**
 * The account as the backend takes it, or `null` for a submission that is not
 * about one — which the backend reads as *keep what is stored*.
 */
export function accountOf(fields: AccountFields): SecretAccount | null {
  return fields.username !== "" && fields.password !== ""
    ? { username: fields.username, password: fields.password }
    : null;
}

/** Whether the pair is half filled in, which is not an account. */
export function accountHalfDone(fields: AccountFields): boolean {
  return (fields.username === "") !== (fields.password === "");
}

/** What both forms say about a half-filled pair. */
export const ACCOUNT_INCOMPLETE = "An account needs both a username and a password.";
