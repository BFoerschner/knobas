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
  CardRows,
  ContextRow,
  EntityDetail,
  EntityRow,
  JobLinkSets,
  Reached,
  Thread,
} from "../../ipc/entity";
import { ipcErrorMessage } from "../../ipc";
import {
  isEntityId,
  isMine,
  keyOf,
  offersStartWork,
  resolveJob,
  segmentKey,
  type AssetLine,
  type FrameKinds,
  type SourceFacts,
} from "./job";
import { bridgeTicketBoard, type JobPorts } from "./ports";
import { entityCheckout } from "../../ipc/entity";
import { parentOf } from "../sets/apply";
import { countSubtasks, readAssetLines, readUnpushed } from "./card-sections";
import { reachFilter, readSection, SECTION_LIMIT, SECTION_READS, type SectionRead } from "./sections";

/** The reads the reader makes: the job's own ports, without its writes. */
export type JobReaderPorts = Pick<
  JobPorts,
  | "listSources"
  | "getEntity"
  | "entityBriefs"
  | "promoteContext"
  | "listEntities"
  | "visitEntity"
  | "entityThread"
  | "jobReach"
  | "jobLinkSets"
  | "getAsset"
  | "ticketBoard"
  | "entityCheckout"
  | "cardReads"
>;

/** The ticket's sub-tasks as the card's row counts them (#2401). */
export interface Subtasks {
  total: number;
  open: number;
}

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
  /**
   * Whether the parent's title read has answered (or there is no parent to read): the epic chip draws once it has, so it
   * doesn't widen from the key to the key and title under the reader (the nothing-moves pass, 2026-11-17).
   */
  readonly parentRead: boolean;
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

/** How recent a context must be, against the promotion's send, to be one it made. */
const MADE_WITHIN_MS = 60_000;

/** Whether a promotion answered an epic's context it may have just made (`created_at` within {@link MADE_WITHIN_MS}). */
function madeEpic(row: ContextRow, sent: number): boolean {
  const made = Date.parse(row.created_at);
  return row.kind === "epic" && Number.isFinite(made) && made >= sent - MADE_WITHIN_MS;
}

/** Every slot as it is before a job answers. */
function fresh() {
  return {
    detail: undefined as EntityDetail | null | undefined,
    others: [] as EntityDetail[],
    parentTitle: null as string | null,
    parentRead: false,
    code: null as EntityRow[] | null,
    failure: null as string | null,
    thread: null as Thread | null,
    threadFailure: null as string | null,
    lastVisit: null as string | null,
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
    // An address that names the ticket's id (every press does) names the ticket it opens, or none: the card's read goes
    // with the press, beside the three reads that find the ticket, rather than after them (ux19).
    const early = ports.cardReads !== undefined && isEntityId(wanted) ? askCard(wanted) : null;
    // The sub-tasks are the board under the key, which every address names, whichever source's ticket it opens.
    const subtasks = askSubtasks(segmentKey(wanted));
    try {
      const listed = await ports.listSources().catch(() => [] as SourceFacts[]);
      if (mine !== token) return;
      state.sources = listed;
      const job = await resolveJob(wanted, listed, ports.getEntity, ports.entityBriefs);
      if (mine !== token) return;
      state.others = job?.others ?? [];
      const found = job?.ticket ?? null;
      state.detail = found;
      if (found === null) return;
      const id = found.row.entity_id;
      void readConversation(id, mine);
      void readParent(found, mine);
      void readSubtasks(id, mine, keyOf(id) === segmentKey(wanted) ? subtasks : null);
      void readInherited(id, mine);
      // The reach seeds from the ticket's context's adds, and a context promotion makes is born with none, so the
      // reach, the sections and Code need not wait for it (the query audit, 2026-11-12: three round trips in a row
      // before anything below the card was asked). An epic's is the exception, read again below.
      const promoted = ports.promoteContext(id);
      if (ports.cardReads !== undefined) {
        // An epic's context reaches its sub-tasks, and the card's walk may run before the promotion commits one it
        // makes: read the card again once it has.
        const sent = Date.now();
        void promoted.then((row) => {
          if (madeEpic(row, sent) && mine === token) void readCard(id, mine);
        }, () => {});
        // The reach and the four sections in one read over one walk (ux19): five round trips were five chances to be
        // the slowest, and the card holds its sections until the last has answered.
        const card = readCard(id, mine, id === wanted ? early : null);
        if (offersStartWork(found.status, isMine(found, listed))) void readRepos(mine);
        await Promise.all([promoted, card]);
        return;
      }
      void readReach(id, mine);
      void readSections(id, mine);
      if (offersStartWork(found.status, isMine(found, listed))) void readRepos(mine);
      await Promise.all([promoted, readCode(id, mine)]);
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

  /**
   * The reach and Code, Builds, Docs and Monitors in one read (`card_reads`), each section's rows as its own reach-scoped
   * `list_entities` answers them; then the reached assets and Code's *not pushed*. A read that fails draws every section
   * as having nothing, as a failed section read does alone.
   */
  async function readCard(id: string, mine: number, asked: Promise<CardRows | null> | null = null) {
    void registry.load();
    const answer = await (asked ?? askCard(id));
    if (mine !== token) return;
    const rows = (at: number) => answer?.sections[at] ?? [];
    const reached = answer?.reached ?? [];
    state.reached = new Map(reached.map((row) => [row.entity_id, row]));
    state.code = rows(0);
    state.builds = rows(1);
    state.pages = rows(2);
    state.monitors = rows(3);
    await Promise.all([
      readAssetLines(ports.getAsset, reached).then((lines) => {
        if (mine === token) state.assets = lines;
      }),
      readUnpushedOf(rows(0), mine),
    ]);
  }

  /** The card's read for `id`; `null` when it fails: nothing reached, and no section has anything in it. */
  function askCard(id: string): Promise<CardRows | null> {
    const order = [SECTION_READS.code, SECTION_READS.builds, SECTION_READS.pages, SECTION_READS.monitors];
    return ports.cardReads!(id, order.map((read) => reachFilter(id, read.kinds, read.order, read.role)), SECTION_LIMIT).catch(() => null);
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
    const lines = await readAssetLines(ports.getAsset, rows);
    if (mine === token) state.assets = lines;
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
    try {
      if (parent === null) return;
      const title = ports.entityBriefs
        ? (await ports.entityBriefs([parent]))[0]?.title
        : (await ports.getEntity(parent)).row.title;
      if (title === undefined) return;
      if (mine === token) state.parentTitle = title;
    } catch {
      // The chip says the key alone.
    } finally {
      if (mine === token) state.parentRead = true;
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
    if (ports.cardReads !== undefined) {
      await Promise.allSettled([readInherited(id, mine), readSubtasks(id, mine), readCard(id, mine)]);
      return;
    }
    await Promise.allSettled([
      readInherited(id, mine),
      readSubtasks(id, mine),
      readReach(id, mine),
      readSections(id, mine),
      readRows(id, SECTION_READS.code, mine).then((rows) => {
        if (mine !== token || !rows) return;
        state.code = rows;
        return readUnpushedOf(rows, mine);
      }),
    ]);
  }

  async function readCode(id: string, mine: number) {
    const rows = await readSection(ports.listEntities, id, SECTION_READS.code);
    if (mine !== token) return;
    state.code = rows;
    void readUnpushedOf(rows, mine);
  }

  /**
   * The sub-tasks (#2401): the ticket system's board under `under:<key>`,
   * counted by its columns' totals, a column that is not *Done* open. A read
   * that fails draws no Sub-tasks row.
   */
  async function readSubtasks(id: string, mine: number, asked: Promise<Subtasks> | null = null) {
    const counted = await (asked ?? askSubtasks(keyOf(id)));
    if (mine === token) state.subtasks = counted;
  }

  /** The sub-tasks under `key`; none when the board cannot be read. */
  function askSubtasks(key: string): Promise<Subtasks> {
    return (ports.ticketBoard ?? bridgeTicketBoard)(`under:${key}`).then(
      (board) => countSubtasks(board, key),
      () => ({ total: 0, open: 0 }),
    );
  }

  /** Code's *not pushed* (#2401). */
  async function readUnpushedOf(rows: readonly EntityRow[], mine: number) {
    const unpushed = await readUnpushed(ports.entityCheckout ?? entityCheckout, rows);
    if (mine === token) state.unpushed = unpushed;
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
    get parentRead() {
      return state.parentRead;
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
