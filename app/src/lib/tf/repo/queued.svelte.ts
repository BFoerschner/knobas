/**
 * What the repository page queued this session (#839, #845), so a write shows
 * at once where it will land, as prototype-6's `branches.push({sub:'queued'})`
 * and `S.edits` do: a branch being made or deleted, a file whose commit is on
 * its way. Held for the session as module `$state`, never written anywhere;
 * the write queue is the record, and a restart reads only what the source has.
 *
 * Each mark goes as soon as the source's own read shows the write landed: a
 * made branch once the listing has it, a commit once the file's blob moved.
 */

/** Per repository entity id: branches queued to be made, and to be deleted. */
const made: Record<string, string[]> = $state({});
const deleted: Record<string, string[]> = $state({});

/** A queued commit of one file: its message and the blob it was made against. */
export interface QueuedCommit {
  message: string;
  base: string;
}

/** By `repo entity id`, branch and path. */
const commits: Record<string, QueuedCommit> = $state({});

const commitKey = (repoId: string, branch: string, path: string) => `${repoId}\u0000${branch}\u0000${path}`;

/** A branch queued to be made in `repoId`. */
export function markMade(repoId: string, name: string): void {
  if (!(made[repoId] ?? []).includes(name)) made[repoId] = [...(made[repoId] ?? []), name];
}

/** The branches queued to be made in `repoId` that `listed` does not hold yet. */
export function madeNotListed(repoId: string, listed: readonly string[]): string[] {
  return (made[repoId] ?? []).filter((name) => !listed.includes(name));
}

/** A branch of `repoId` queued to be deleted. */
export function markDeleted(repoId: string, name: string): void {
  if (!(deleted[repoId] ?? []).includes(name)) deleted[repoId] = [...(deleted[repoId] ?? []), name];
}

export function isDeleteQueued(repoId: string, name: string): boolean {
  return deleted[repoId]?.includes(name) ?? false;
}

/** A commit of `path` onto `branch` queued against the blob `base`. */
export function markCommit(repoId: string, branch: string, path: string, commit: QueuedCommit): void {
  commits[commitKey(repoId, branch, path)] = commit;
}

/**
 * The commit queued for this file, while the file read is still the blob it
 * was made against; `null` once Gitea holds something newer.
 */
export function queuedCommit(repoId: string, branch: string, path: string, sha: string | null): QueuedCommit | null {
  const commit = commits[commitKey(repoId, branch, path)];
  return commit !== undefined && commit.base === sha ? commit : null;
}

/** Forget every mark: a new session, which is what each test is. */
export function forgetEchoes(): void {
  for (const bag of [made, deleted, commits] as Record<string, unknown>[]) {
    for (const key of Object.keys(bag)) delete bag[key];
  }
}
