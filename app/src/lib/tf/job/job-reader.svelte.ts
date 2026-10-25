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
  CheckoutView,
  EntityDetail,
  EntityRow,
  JobLinkSets,
  Reached,
  SuggestionEntry,
  Thread,
} from "../../ipc/entity";
import type { CredentialHealth, SourceSyncStatus } from "../../ipc/sources";
import { ipcErrorMessage } from "../../ipc";
import { dayKey } from "../../time/day";
import { weekWindows } from "../../time/week";
import { PROPOSAL_LIMIT, proposalsFor } from "./connections";
import {
  assetTone,
  byAttention,
  isMine,
  offersStartWork,
  resolveJob,
  weekReading,
  type AssetLine,
  type FrameKinds,
  type SourceFacts,
  type WeekReading,
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
  | "reachableTransitions"
  | "entityCheckout"
  | "weekTimesheet"
  | "visitEntity"
  | "entityThread"
  | "roomSuggestions"
  | "jobReach"
  | "jobLinkSets"
  | "getAsset"
  | "credentialHealth"
  | "syncStatus"
>;

/** One repository *Start work*'s dropdown offers. */
export interface RepoChoice {
  entity_id: string;
  title: string;
}

export interface JobReaderOptions {
  /** What *Time this week* counts from. */
  today: () => Date;
  /** The adapters' claims, loaded when the sections are read, for routing a run (#1256). */
  registry: Pick<FrameKinds, "load">;
}

export interface JobReader {
  /** `undefined` while reading, `null` when nothing answers to the address. */
  readonly detail: EntityDetail | null | undefined;
  /** The other sources' tickets under the same key (#1476), in list order; empty when the key is one source's. */
  readonly others: EntityDetail[];
  /** The statuses the ticket can move to, `null` until read, `[]` when the read fails. */
  readonly transitions: string[] | null;
  readonly week: WeekReading | null;
  /** Why the job could not be read, or `null`. */
  readonly failure: string | null;
  /** Every configured source, for which ticket is the person's own. */
  readonly sources: SourceFacts[];
  /** Every source's credential and last run: the Builds section's warning. */
  readonly health: CredentialHealth[];
  readonly sync: SourceSyncStatus[];
  readonly code: EntityRow[] | null;
  /** The Code section's checkout line, and the row it is the checkout of. */
  readonly checkout: CheckoutView | null;
  readonly checkoutOf: string | null;
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
   * job's reach. The checkout line stays as it was. Nothing when the job on
   * screen is not `id`.
   */
  rereadReach(id: string): void;
  /**
   * After a move or a link changed: the ticket and its transitions again,
   * keeping what else the job read. Nothing when the job on screen is not
   * `id` (by default, the one on screen).
   */
  rereadDetail(id?: string): void;
  /** After a refresh or a run brought a queued comment in (#2103): the thread again, keeping the visit it answered. */
  rereadThread(): void;
  /** After a clone finished: the checkout line's repository read again, while it has no clone. */
  rereadCheckout(): void;
  /**
   * After a *Refresh* (#2134): the ticket and its transitions, its week, its
   * thread and its reach with the sections, each over what the page drew as
   * `rereadDetail`, `rereadThread` and `rereadReach` read them (a failed
   * ticket or thread read keeps what it drew). Settles once those reads have
   * answered. Nothing is promoted again.
   */
  reread(): Promise<void>;
  /** Drops every answer still out. */
  close(): void;
}

/** How many branches and repositories are asked for a clone before giving up. */
const CHECKOUT_TRIES = 3;

/** Every slot as it is before a job answers. */
function fresh() {
  return {
    detail: undefined as EntityDetail | null | undefined,
    others: [] as EntityDetail[],
    transitions: null as string[] | null,
    week: null as WeekReading | null,
    code: null as EntityRow[] | null,
    checkout: null as CheckoutView | null,
    checkoutOf: null as string | null,
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
  const { today, registry } = options;
  const state = $state({
    ...fresh(),
    // Kept from one job to the next: each job reads them again.
    sources: [] as SourceFacts[],
    health: [] as CredentialHealth[],
    sync: [] as SourceSyncStatus[],
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
      void readWorkflow(id, mine);
      void readWeek(id, mine);
      void readConversation(id, mine);
      void readStale(mine);
      void readInherited(id, mine);
      // The reach starts from the ticket's context, so the context first.
      const context = await ports.promoteContext(id);
      if (mine !== token) return;
      void readProposals(context.id, id, mine);
      void readReach(id, mine);
      readSections(id, mine);
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

  /** Builds, Pages and Monitors, each through the reach. */
  function readSections(id: string, mine: number) {
    // Builds asks for the CI role and the mirror answers its run kinds
    // (#1295); the adapters' claims only route a run to its page (#1256).
    void registry.load();
    void readRows(id, SECTION_READS.builds, mine).then((rows) => {
      if (mine === token && rows) state.builds = rows;
    });
    void readRows(id, SECTION_READS.pages, mine).then((rows) => {
      if (mine === token && rows) state.pages = rows;
    });
    void readRows(id, SECTION_READS.monitors, mine).then((rows) => {
      if (mine === token && rows) state.monitors = rows;
    });
  }

  /** What the Builds section's warning reads: every source's credential and last run. */
  async function readStale(mine: number) {
    const [credentials, runs] = await Promise.all([
      ports.credentialHealth().catch(() => [] as CredentialHealth[]),
      ports.syncStatus().catch(() => [] as SourceSyncStatus[]),
    ]);
    if (mine !== token) return;
    state.health = credentials;
    state.sync = runs;
  }

  async function readRepos(mine: number) {
    try {
      const page = await ports.listEntities(reachFilter(null, ["repo"], "title_asc", "code-host"), SECTION_LIMIT, 0);
      if (mine === token) state.repos = page.rows.map((row) => ({ entity_id: row.entity_id, title: row.title }));
    } catch {
      if (mine === token) state.repos = [];
    }
  }

  async function readWorkflow(id: string, mine: number) {
    try {
      const statuses = await ports.reachableTransitions(id);
      if (mine === token) state.transitions = statuses;
    } catch {
      // A workflow that cannot be read offers nothing: the select shows where
      // the ticket stands, and the move waits for a read that works.
      if (mine === token) state.transitions = [];
    }
  }

  async function readWeek(id: string, mine: number) {
    try {
      const now = today();
      const answer = await ports.weekTimesheet(weekWindows(dayKey(now)));
      if (mine === token) state.week = weekReading(answer, id, dayKey(now));
    } catch {
      // *Time this week* is an extra; the line stays at its ellipsis.
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

  /** The link sets, the reach and every section again, the checkout line kept. */
  async function readReachAgain(id: string, mine: number) {
    readSections(id, mine);
    void readRows(id, SECTION_READS.code, mine).then((rows) => {
      if (mine === token && rows) state.code = rows;
    });
    await Promise.allSettled([readInherited(id, mine), readReach(id, mine)]);
  }

  async function readCode(id: string, mine: number) {
    const rows = await readSection(ports.listEntities, id, SECTION_READS.code);
    if (mine !== token) return;
    state.code = rows;
    // A clone is looked for in the repositories Code draws: the read asked for
    // the code host role, so they are its fillers' alone (#1298).
    const sorted = byAttention(rows);
    const candidates = [
      ...sorted.filter((row) => row.kind === "branch"),
      ...sorted.filter((row) => row.kind === "repo"),
    ].slice(0, CHECKOUT_TRIES);
    // The first clone found; else the first repository, which then offers
    // *Clone* on the line (R-8.25).
    let uncloned: { view: CheckoutView; of: string } | null = null;
    for (const row of candidates) {
      try {
        const view = await ports.entityCheckout(row.entity_id);
        if (mine !== token) return;
        if (view.path) {
          state.checkout = view;
          state.checkoutOf = row.entity_id;
          break;
        }
        if (uncloned === null && row.kind === "repo") uncloned = { view, of: row.entity_id };
      } catch {
        // A checkout that cannot be read is a checkout not shown.
      }
    }
    if (state.checkout === null && uncloned !== null) {
      state.checkout = uncloned.view;
      state.checkoutOf = uncloned.of;
    }
  }

  return {
    get detail() {
      return state.detail;
    },
    get others() {
      return state.others;
    },
    get transitions() {
      return state.transitions;
    },
    get week() {
      return state.week;
    },
    get failure() {
      return state.failure;
    },
    get sources() {
      return state.sources;
    },
    get health() {
      return state.health;
    },
    get sync() {
      return state.sync;
    },
    get code() {
      return state.code;
    },
    get checkout() {
      return state.checkout;
    },
    get checkoutOf() {
      return state.checkoutOf;
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
      void readWorkflow(id, mine);
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
        readWorkflow(id, mine),
        readWeek(id, mine),
        readThreadAgain(id, mine),
        readReachAgain(id, mine),
      ]);
    },
    rereadCheckout() {
      const of = state.checkoutOf;
      const mine = token;
      if (of === null || state.checkout?.path) return;
      void ports.entityCheckout(of).then(
        (view) => {
          if (mine === token && of === state.checkoutOf) state.checkout = view;
        },
        // The line keeps what it has; the next open reads it again.
        () => {},
      );
    },
    close() {
      token += 1;
    },
  };
}
