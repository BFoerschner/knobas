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
  keyOf,
  offersStartWork,
  resolveJob,
  type AssetLine,
  type FrameKinds,
  type SourceFacts,
} from "./job";
import { bridgeTicketBoard, type JobPorts } from "./ports";
import { entityCheckout } from "../../ipc/entity";
import { isDone } from "../tickets/tickets";
import { parentOf } from "../sets/apply";
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
  | "ticketBoard"
  | "entityCheckout"
>;

/** The ticket's sub-tasks as the card's row counts them (#2401). */
export interface Subtasks {
  total: number;
  open: number;
}

/** How many of a ticket's branches have their checkout read for Code's *not pushed* (#2401). */
export const UNPUSHED_LIMIT = 10;

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
  /**
   * The title of the ticket's direct parent, its path line's last key, read
   * from the mirror for the card's epic chip (#2400); `null` with no parent,
   * while it is read, or when it cannot be.
   */
  readonly parentTitle: string | null;
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
  /** The sub-tasks under the ticket, as the ticket system's board answers `under:<key>`; `null` until read. */
  readonly subtasks: Subtasks | null;
  /** The ids of the Code rows' branches whose checkout here has commits not pushed (#2401). */
  readonly unpushed: ReadonlySet<string>;
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
    parentTitle: null as string | null,
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
    subtasks: null as Subtasks | null,
    unpushed: new Set<string>() as ReadonlySet<string>,
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
      void readParent(found, mine);
      void readSubtasks(id, mine);
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
      // A section that cannot be read draws as having nothing.
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

  /** The parent's title for the epic chip; a parent that cannot be read keeps the path line alone. */
  async function readParent(ticket: EntityDetail, mine: number) {
    const parent = parentOf(ticket.row);
    if (parent === null) return;
    try {
      const read = await ports.getEntity(parent);
      if (mine === token) state.parentTitle = read.row.title;
    } catch {
      // The chip says the key alone.
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

  /** The link sets, the reach, the sub-tasks and every section again; settles once all have answered. */
  async function readReachAgain(id: string, mine: number) {
    await Promise.allSettled([
      readInherited(id, mine),
      readSubtasks(id, mine),
      readReach(id, mine),
      readSections(id, mine),
      readRows(id, SECTION_READS.code, mine).then((rows) => {
        if (mine !== token || !rows) return;
        state.code = rows;
        return readUnpushed(rows, mine);
      }),
    ]);
  }

  async function readCode(id: string, mine: number) {
    const rows = await readSection(ports.listEntities, id, SECTION_READS.code);
    if (mine !== token) return;
    state.code = rows;
    void readUnpushed(rows, mine);
  }

  /**
   * The sub-tasks (#2401): the ticket system's board under `under:<key>`,
   * counted by its columns' totals, a column that is not *Done* open. A read
   * that fails draws no Sub-tasks row.
   */
  async function readSubtasks(id: string, mine: number) {
    try {
      const board = await (ports.ticketBoard ?? bridgeTicketBoard)(`under:${keyOf(id)}`);
      if (mine !== token) return;
      const total = board.columns.reduce((sum, column) => sum + column.total, 0);
      const open = board.columns.filter((column) => !isDone(column.status)).reduce((sum, column) => sum + column.total, 0);
      state.subtasks = { total, open };
    } catch {
      if (mine === token) state.subtasks = { total: 0, open: 0 };
    }
  }

  /**
   * Code's *not pushed* (#2401): a branch counts when its checkout on this
   * machine stands on that branch with commits ahead of its upstream. The
   * mirror's branches are the forge's, so only the checkout knows. A branch
   * with no checkout, or one standing elsewhere, counts nothing.
   */
  async function readUnpushed(rows: readonly EntityRow[], mine: number) {
    const read = ports.entityCheckout ?? entityCheckout;
    const branches = rows.filter((row) => row.kind === "branch").slice(0, UNPUSHED_LIMIT);
    const answers = await Promise.allSettled(branches.map(async (row) => read(row.entity_id)));
    if (mine !== token) return;
    state.unpushed = new Set(
      branches
        .filter((row, at) => {
          const answer = answers[at]!;
          if (answer.status !== "fulfilled") return false;
          const tree = answer.value.tree;
          return tree !== null && tree.status === "read" && tree.branch === row.title && tree.ahead > 0;
        })
        .map((row) => row.entity_id),
    );
  }

  return {
    get detail() {
      return state.detail;
    },
    get others() {
      return state.others;
    },
    get parentTitle() {
      return state.parentTitle;
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
    get subtasks() {
      return state.subtasks;
    },
    get unpushed() {
      return state.unpushed;
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
