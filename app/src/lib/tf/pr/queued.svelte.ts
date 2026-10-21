/**
 * What the pull request page queued this session (#841), so a write shows at
 * once where it will land, as `repo/queued.svelte.ts` does for the repository
 * page: a chip beside the state (`close queued`) and a comment at the end of
 * the conversation. Module `$state` by the pull request's entity id, never
 * written anywhere; a comment's mark goes once the conversation holds its
 * words.
 */

const chips: Record<string, string[]> = $state({});
const comments: Record<string, string[]> = $state({});

/** A write on `prId` queued, drawn as `chip` beside its state. */
export function markQueued(prId: string, chip: string): void {
  chips[prId] = [...(chips[prId] ?? []).filter((held) => held !== chip), chip];
}

/** The chips of what this session queued on `prId`. */
export function queuedChips(prId: string): string[] {
  return chips[prId] ?? [];
}

/** Whether this session queued `chip` on `prId`: what holds a toolbar button back (#1441). */
export function isQueued(prId: string, chip: string): boolean {
  return chips[prId]?.includes(chip) ?? false;
}

/** A comment on `prId` queued. */
export function markComment(prId: string, body: string): void {
  comments[prId] = [...(comments[prId] ?? []), body];
}

/** The comments queued on `prId` whose words `said` does not hold yet. */
export function pendingComments(prId: string, said: readonly string[]): string[] {
  return (comments[prId] ?? []).filter((body) => !said.some((words) => words.trim() === body.trim()));
}

/** Forget every mark: a new session, which is what each test is. */
export function forgetPrQueued(): void {
  for (const bag of [chips, comments]) for (const key of Object.keys(bag)) delete bag[key];
}
