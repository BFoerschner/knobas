/**
 * The job page's reads (#1320, spec #1304): what opening a job reads, in
 * order, each answer into its own slot, and the rereads a write asks for.
 *
 * Opening reads the sources, then the ticket, then **promotes its context**
 * (`promote_context`, idempotent, so a ticket with a context keeps it and one
 * without gets one) and reads what the context reaches. The promotion happens
 * once per job opened: a reread after a write never asks again.
 *
 * A token guards every answer: opening another job, or closing this one,
 * bumps it, so an answer for a job left behind is dropped. Opening resets
 * every slot at once, so nothing of the last job is drawn while the next is
 * read.
 *
 * `TfJob.svelte` binds to it and keeps the writes and the drawing. The reader
 * touches no DOM, so its race is tested on fake ports (`job-reader.test.ts`).
 */
import type {
  EntityDetail,
  EntityRow,
  JobLinkSets,
  Reached,
  SuggestionEntry,
  Thread,
} from "../../ipc/entity";
import { ipcErrorMessage } from "../../ipc";
import { PROPOSAL_LIMIT, proposalsFor } from "./connections";
import {
  assetTone,
  isMine,
  offersStartWork,
  resolveJob,
  type AssetLine,
  type FrameKinds,
  type SourceFacts,
} from "./job";
import type { JobPorts } from "./ports";
import { ASSET_LIMIT, reachFilter, readSection, SECTION_LIMIT, SECTION_READS, type SectionRead } from "./sections";

/** The reads the reader makes: the job's own ports, without its writes. */
export type JobReaderPorts = Pick<
  JobPorts,
  | "listSources"
  | "getEntity"
  | "promoteContext"
  | "listEntities"
  | "visitEntity"
  | "entityThread"
  | "roomSuggestions"
  | "jobReach"
  | "jobLinkSets"
  | "getAsset"
>;

/** One repository *Start work*'s dropdown offers. */
export interface RepoChoice {
  entity_id: string;
  title: string;
}

export interface JobReaderOptions {
  /** The adapters' claims, loaded when the sections are read, for routing a run (#1256). */
  registry: Pick<FrameKinds, "load">;
}

export interface JobReader {
  /** `undefined` while reading, `null` when nothing answers to the address. */
  readonly detail: EntityDetail | null | undefined;
  /** The other sources' tickets under the same key (#1476), in list order; empty when the key is one source's. */
  readonly others: EntityDetail[];
  /** Why the job could not be read, or `null`. */
  readonly failure: string | null;
  /** Every configured source, for which ticket is the person's own. */
  readonly sources: SourceFacts[];
  readonly code: EntityRow[] | null;
  /** Every repository, for *Start work*'s dropdown; `null` until asked. */
  readonly repos: RepoChoice[] | null;
  readonly builds: EntityRow[] | null;
  readonly pages: EntityRow[] | null;
  readonly monitors: EntityRow[] | null;
  readonly assets: AssetLine[] | null;
  /** The job-reach read, by entity id: the rows' labels. */
  readonly reached: Map<string, Reached>;
  /** The link sets that apply and the parent (#867), `null` until read. */
  readonly inherited: JobLinkSets | null;
  readonly thread: Thread | null;
  readonly threadFailure: string | null;
  /** When the person last opened this job before now: `null` for never. */
  readonly lastVisit: string | null;
  readonly proposals: SuggestionEntry[] | null;
  /** Opens the job `segment` names: every slot starts fresh, and an answer for the last job is dropped. */
  open(segment: string): void;
  /**
   * After *Hide here*, *Show again* or a link set saved: the groups, the
   * labels and the sections' rows again, since a hidden member leaves the
   * job's reach. Nothing when the job on screen is not `id`.
   */
  rereadReach(id: string): void;
  /**
   * After a move or a link changed: the ticket again, keeping what else the
   * job read. Nothing when the job on screen is not
   * `id` (by default, the one on screen).
   */
  rereadDetail(id?: string): void;
  /** After a refresh or a run brought a queued comment in (#2103): the thread again, keeping the visit it answered. */
  rereadThread(): void;
  /**
   * After a *Refresh* (#2134): the ticket, its thread and its reach with the sections, each over what the page drew as
   * `rereadDetail`, `rereadThread` and `rereadReach` read them (a failed
   * ticket or thread read keeps what it drew). Settles once those reads have
   * answered. Nothing is promoted again.
   */
  reread(): Promise<void>;
  /** Drops every answer still out. */
  close(): void;
}

/** Every slot as it is before a job answers. */
function fresh() {
  return {
    detail: undefined as EntityDetail | null | undefined,
    others: [] as EntityDetail[],
    code: null as EntityRow[] | null,
    failure: null as string | null,
    thread: null as Thread | null,
    threadFailure: null as string | null,
    lastVisit: null as string | null,
    proposals: null as SuggestionEntry[] | null,
    reached: new Map<string, Reached>(),
    inherited: null as JobLinkSets | null,
    builds: null as EntityRow[] | null,
    pages: null as EntityRow[] | null,
    monitors: null as EntityRow[] | null,
    assets: null as AssetLine[] | null,
    repos: null as RepoChoice[] | null,
  };
}

export function createJobReader(ports: JobReaderPorts, options: JobReaderOptions): JobReader {
  const { registry } = options;
  const state = $state({
    ...fresh(),
    // Kept from one job to the next: each job reads them again.
    sources: [] as SourceFacts[],
  });
  /** The read's generation, so an answer for a ticket left behind is dropped. */
  let token = 0;

  async function readJob(wanted: string, mine: number) {
    try {
      const listed = await ports.listSources().catch(() => [] as SourceFacts[]);
      if (mine !== token) return;
      state.sources = listed;
      const job = await resolveJob(wanted, listed, ports.getEntity);
      if (mine !== token) return;
      state.others = job?.others ?? [];
      const found = job?.ticket ?? null;
      state.detail = found;
      if (found === null) return;
      const id = found.row.entity_id;
      void readConversation(id, mine);
      void readInherited(id, mine);
      // The reach starts from the ticket's context, so the context first.
      const context = await ports.promoteContext(id);
      if (mine !== token) return;
      void readProposals(context.id, id, mine);
      void readReach(id, mine);
      void readSections(id, mine);
      if (offersStartWork(found.status, isMine(found, listed))) void readRepos(mine);
      await readCode(id, mine);
    } catch (cause) {
      if (mine === token) state.failure = ipcErrorMessage(cause);
    }
  }

  /** One section's rows, through the reach; `[]` when the read fails. */
  async function readRows(id: string, section: SectionRead, mine: number): Promise<EntityRow[] | null> {
    try {
      const rows = await readSection(ports.listEntities, id, section);
      return mine === token ? rows : null;
    } catch {
      // A section that cannot be read draws as having nothing; the Work tab's
      // line says when the reads themselves fail.
      return mine === token ? [] : null;
    }
  }

  /** The reach itself, for the labels, and the reached assets for their section. */
  async function readReach(id: string, mine: number) {
    let rows: Reached[] = [];
    try {
      rows = await ports.jobReach(id);
    } catch {
      // No labels, and an Assets section with nothing in it.
    }
    if (mine !== token) return;
    state.reached = new Map(rows.map((row) => [row.entity_id, row]));
    const ids = rows.filter((row) => row.kind === "asset").slice(0, ASSET_LIMIT);
    const read = await Promise.allSettled(ids.map((row) => ports.getAsset(row.entity_id)));
    if (mine !== token) return;
    state.assets = read.flatMap((answer) =>
      answer.status === "fulfilled"
        ? [
            {
              id: answer.value.asset.id,
              name: answer.value.asset.name,
              type: answer.value.asset.type_label,
              environment: answer.value.effective_environment?.value ?? null,
              on: answer.value.held_by.at(-1)?.name ?? null,
              tone: assetTone(answer.value.asset.health),
            },
          ]
        : [],
    );
  }

  /** The link sets that apply and the parent; nothing of either when the read fails. */
  async function readInherited(id: string, mine: number) {
    try {
      const read = await ports.jobLinkSets(id);
      if (mine === token) state.inherited = read;
    } catch {
      if (mine === token) state.inherited = { sets: [], parent: null };
    }
  }

  /** Builds, Pages and Monitors, each through the reach; settles once all three have answered. */
  async function readSections(id: string, mine: number) {
    // Builds asks for the CI role and the mirror answers its run kinds
    // (#1295); the adapters' claims only route a run to its page (#1256).
    void registry.load();
    await Promise.all([
      readRows(id, SECTION_READS.builds, mine).then((rows) => {
        if (mine === token && rows) state.builds = rows;
      }),
      readRows(id, SECTION_READS.pages, mine).then((rows) => {
        if (mine === token && rows) state.pages = rows;
      }),
      readRows(id, SECTION_READS.monitors, mine).then((rows) => {
        if (mine === token && rows) state.monitors = rows;
      }),
    ]);
  }

  async function readRepos(mine: number) {
    try {
      const page = await ports.listEntities(reachFilter(null, ["repo"], "title_asc", "code-host"), SECTION_LIMIT, 0);
      if (mine === token) state.repos = page.rows.map((row) => ({ entity_id: row.entity_id, title: row.title }));
    } catch {
      if (mine === token) state.repos = [];
    }
  }

  async function readConversation(id: string, mine: number) {
    // The visit is written first and answers the one before it; a visit that
    // cannot be written reads as no earlier visit, so nothing is called new.
    const [visit, read] = await Promise.allSettled([ports.visitEntity(id), ports.entityThread(id)]);
    if (mine !== token) return;
    state.lastVisit = visit.status === "fulfilled" ? visit.value : null;
    if (read.status === "fulfilled") state.thread = read.value;
    else state.threadFailure = ipcErrorMessage(read.reason);
  }

  async function readProposals(context: string, id: string, mine: number) {
    try {
      const page = await ports.roomSuggestions([], context, PROPOSAL_LIMIT);
      if (mine === token) state.proposals = proposalsFor(page.rows, id);
    } catch {
      // A tray that cannot be read proposes nothing; the button says connected only.
      if (mine === token) state.proposals = [];
    }
  }

  /** The ticket again, over what was drawn; a failed read keeps it. */
  async function readTicketAgain(id: string, mine: number) {
    try {
      const again = await ports.getEntity(id);
      if (mine === token) state.detail = again;
    } catch {
      // The card keeps what it has; the next open reads it again.
    }
  }

  /** The thread again, keeping the visit it answered; a failed read keeps it. */
  async function readThreadAgain(id: string, mine: number) {
    try {
      const again = await ports.entityThread(id);
      if (mine === token) state.thread = again;
    } catch {
      // The thread keeps what it has; the next open reads it again.
    }
  }

  /** The link sets, the reach and every section again, the checkout line kept; settles once all have answered. */
  async function readReachAgain(id: string, mine: number) {
    await Promise.allSettled([
      readInherited(id, mine),
      readReach(id, mine),
      readSections(id, mine),
      readRows(id, SECTION_READS.code, mine).then((rows) => {
        if (mine === token && rows) state.code = rows;
      }),
    ]);
  }

  async function readCode(id: string, mine: number) {
    const rows = await readSection(ports.listEntities, id, SECTION_READS.code);
    if (mine === token) state.code = rows;
  }

  return {
    get detail() {
      return state.detail;
    },
    get others() {
      return state.others;
    },
    get failure() {
      return state.failure;
    },
    get sources() {
      return state.sources;
    },
    get code() {
      return state.code;
    },
    get repos() {
      return state.repos;
    },
    get builds() {
      return state.builds;
    },
    get pages() {
      return state.pages;
    },
    get monitors() {
      return state.monitors;
    },
    get assets() {
      return state.assets;
    },
    get reached() {
      return state.reached;
    },
    get inherited() {
      return state.inherited;
    },
    get thread() {
      return state.thread;
    },
    get threadFailure() {
      return state.threadFailure;
    },
    get lastVisit() {
      return state.lastVisit;
    },
    get proposals() {
      return state.proposals;
    },
    open(segment) {
      const mine = ++token;
      Object.assign(state, fresh());
      void readJob(segment, mine);
    },
    rereadReach(id) {
      if (state.detail?.row.entity_id !== id) return;
      void readReachAgain(id, token);
    },
    rereadDetail(id = state.detail?.row.entity_id) {
      if (id === undefined || state.detail?.row.entity_id !== id) return;
      const mine = token;
      void readTicketAgain(id, mine);
    },
    rereadThread() {
      const id = state.detail?.row.entity_id;
      if (id === undefined) return;
      void readThreadAgain(id, token);
    },
    async reread() {
      const id = state.detail?.row.entity_id;
      if (id === undefined) return;
      const mine = token;
      await Promise.all([
        readTicketAgain(id, mine),
        readThreadAgain(id, mine),
        readReachAgain(id, mine),
      ]);
    },
    close() {
      token += 1;
    },
  };
}
