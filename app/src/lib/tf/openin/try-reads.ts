/**
 * What the application page reads to try a command (PR #2479, 2026-11-13):
 * the repositories that have a checkout on this disk, and from the one picked
 * its files, its newest commits and its pull requests. Every read is one the
 * app already makes elsewhere (Code's list and its checkouts, *Go to file*,
 * a repository's Commits and Pull requests, a pull request's header); none is
 * new.
 */
import { entityCheckouts, entityLocalDir, entityLocalPaths, entityView, listEntities, objectPage, type EntityFilter, type EntityRow, type OpenObject } from "../../ipc/entity";
import { prNumber } from "../code/code";
import { shaOfCommit } from "../diff/address";
import { headerOf } from "../pr/pr";
import { readAllRows, repoPrs, viewRows } from "../repo/repo";

/** A repository with a checkout here. */
export interface TryRepo {
  entityId: string;
  /** Its `owner/name`. */
  title: string;
  sourceId: string;
  path: string;
  clonesRoot: string | null;
}

export interface TryCommit {
  sha: string;
  title: string;
}

export interface TryPr {
  id: string;
  number: number;
  title: string;
}

export interface TryReads {
  /** Every repository the mirror holds. */
  repos(): Promise<EntityRow[]>;
  /** Their checkouts, `null` where there is none here. */
  checkouts(ids: readonly string[]): Promise<{ entityId: string; path: string | null; clonesRoot: string | null }[]>;
  /** Every file at the checkout, `null` where it can't say. */
  files(repo: string): Promise<string[] | null>;
  /** The newest commits, newest first. */
  commits(repo: string): Promise<TryCommit[]>;
  /** Its pull requests, newest first. */
  prs(repo: TryRepo): Promise<TryPr[]>;
  /** A pull request's two branches. */
  prEnds(prId: string): Promise<{ base: string; head: string } | null>;
  /** The object's page at its source: `{url}`. */
  page(repo: string, object: OpenObject): Promise<string | null>;
}

const ROWS = 200;

function filter(kinds: string[], extra: Partial<EntityFilter> = {}): EntityFilter {
  return {
    sources: [],
    kinds,
    updated_within_days: null,
    order: "updated_desc",
    context: null,
    project: null,
    include_deleted: false,
    reach: null,
    ...extra,
  } as EntityFilter;
}

export const realTryReads: TryReads = {
  repos: () => readAllRows((offset) => listEntities(filter(["repo"]), ROWS, offset)),
  async checkouts(ids) {
    const answers = await entityCheckouts(ids);
    return answers.map((answer) => ({ entityId: answer.entity_id, path: answer.checkout.path, clonesRoot: answer.checkout.clones_root }));
  },
  async files(repo) {
    const top = await entityLocalDir(repo, null, "");
    if (top === null) return null;
    const all = await entityLocalPaths(repo, top.commit);
    return all === null ? null : all.entries.filter((entry) => entry.kind === "file").map((entry) => entry.path);
  },
  async commits(repo) {
    const view = await entityView(repo, "commits");
    return viewRows(view?.sections[0]).flatMap((row) => {
      // A commit the mirror holds has its sha in its id; one it doesn't still shows its short sha as the row's chip.
      const sha = shaOfCommit(row.entity_id) ?? (row.chip !== null && /^[0-9a-f]{7,64}$/i.test(row.chip) ? row.chip : null);
      return sha === null ? [] : [{ sha, title: row.title }];
    });
  },
  async prs(repo) {
    const rows = await readAllRows((offset) => listEntities(filter(["pr"], { sources: [repo.sourceId], parent: repo.title } as Partial<EntityFilter>), ROWS, offset));
    return repoPrs(rows, { source_id: repo.sourceId, title: repo.title }).flatMap((row) => {
      const number = prNumber(row.entity_id);
      return number === null ? [] : [{ id: row.entity_id, number: Number(number.slice(1)), title: row.title }];
    });
  },
  async prEnds(prId) {
    const header = headerOf(await entityView(prId, null));
    return header.base === null || header.head === null ? null : { base: header.base, head: header.head };
  },
  page: (repo, object) => objectPage(repo, object),
};

/** How many checkouts one read asks for, as Code asks. */
export const CHECKOUT_BATCH = 50;
