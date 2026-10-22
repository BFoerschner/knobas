/**
 * Where a credential lives, in words (#1585, #1615): the machine's keychain,
 * unless this process keeps credentials in memory (`KNOBAS_SECRET_STORE=memory`,
 * headless QA; `AppStatus.secrets_in_memory`). The source page and both
 * importer dialogs read it from here, so the two answers never drift.
 */
import { appStatus } from "../ipc/app";

export const keychainName = /Mac/.test(navigator.userAgent) ? "the macOS keychain" : "the system keychain";

/** The source page's "where it went" line. */
export function storedWhere(inMemory: boolean): string {
  return inMemory ? "Stored for this session only" : `Stored in ${keychainName}`;
}

/** The importer token field's hint: where the token is kept. */
export function tokenKeptHint(inMemory: boolean): string {
  const where = inMemory ? "for this session only" : "in this machine's keychain, under this importer's own name";
  return `Kept ${where} — never in the database and never in a share export. You are asked once.`;
}

/** Whether the store is in memory; the keychain wording if `app_status` fails. */
export async function readSecretsInMemory(read: typeof appStatus = appStatus): Promise<boolean> {
  try {
    return (await read()).secrets_in_memory;
  } catch {
    return false;
  }
}
