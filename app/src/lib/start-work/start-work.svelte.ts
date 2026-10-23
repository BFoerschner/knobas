/**
 * The start-work stepper's state (issue #44).
 *
 * Two things live here rather than in the component: the **loading and running**
 * of a flow, so that every action goes through one guard and one re-read, and
 * the **reading of a step's proposal**, because what is editable is per step
 * and a component should not be the place that knows it.
 *
 * ## The rule this module must not break
 *
 * **A failed step never advances the sequence, and this file may not work
 * around that.** The backend decides what runs next; everything here does is
 * ask. There is no loop that walks the rows and dispatches the next one, and a
 * later "convenience" that added one would be the reviewable stepper turning
 * back into the fire-and-forget macro it exists not to be.
 *
 * ## What `queued` means on screen, and why it is neither of the other two
 *
 * A queued step's write is with the source's queue and will go. Drawn as a
 * failure it invites a retry of something already on its way; drawn as a
 * success it tells the reader work happened that has not. It gets its own
 * word, which is what {@link demandOf} is for.
 */
import { ipcErrorMessage } from "../ipc";
import {
  startWorkAmend,
  startWorkFlow,
  startWorkRetry,
  startWorkRun,
  startWorkSkip,
  type StartWorkOutcome,
  type StartWorkStep,
  type StartWorkStepKind,
} from "../ipc/entity";

/** What one step asks of the reader, in one word. */
export type Demand = "waiting" | "running" | "decide" | "done";

/**
 * What a step's outcome asks of the reader.
 *
 * A total switch with **no default**, deliberately: a seventh outcome on the
 * wire is then a TypeScript error here rather than a row drawn under whichever
 * heading the fall-through happened to pick.
 */
export function demandOf(outcome: StartWorkOutcome): Demand {
  switch (outcome) {
    case "succeeded":
    case "skipped":
      return "done";
    case "running":
      return "running";
    case "queued":
      return "waiting";
    case "failed":
      return "decide";
    case "pending":
      return "waiting";
  }
}

/** What the stepper calls each step, in the reader's words rather than the wire's. */
export function labelOf(step: StartWorkStepKind): string {
  switch (step) {
    case "create_branch":
      return "Create the branch";
    case "create_pull_request":
      return "Open the pull request";
    case "link_pull_request":
      return "Link it to the ticket";
    case "transition":
      return "Move the ticket to In Progress";
  }
}

/** One editable line of a step's proposal. */
export interface Field {
  /** The key inside the op's object — `name`, `title`, `body`, `from_ref`. */
  key: string;
  label: string;
  value: string;
  /** Whether it wants a `textarea` rather than an `input`. */
  long: boolean;
}

/**
 * The parts of a step's proposal the reader may change before it runs.
 *
 * Read out of the stored payload rather than from a table of what each op has,
 * so a proposal carrying a field this list does not name is still shown. That
 * includes `entity`: the repository dropdown repoints the branch and pull
 * request steps, and `start_work_amend` takes the amendment.
 */
export function fieldsOf(step: StartWorkStep): Field[] {
  const op = opBody(step);
  if (!op) return [];
  const named: Record<string, [string, boolean]> = {
    name: ["Branch name", false],
    from_ref: ["Starting from", false],
    title: ["Pull request title", false],
    body: ["Pull request body", true],
    head: ["From branch", false],
    base: ["Into branch", false],
    status: ["Status", false],
  };
  return Object.entries(op)
    .filter(([, value]) => typeof value === "string")
    .map(([key, value]) => {
      const [label, long] = named[key] ?? [key, false];
      return { key, label, value: value as string, long };
    });
}

/** A step's payload with `key` replaced — the value `startWorkAmend` takes. */
export function edited(
  step: StartWorkStep,
  key: string,
  value: string,
): unknown {
  const payload = step.payload as Record<string, unknown>;
  const tag = Object.keys(payload)[0];
  if (tag === undefined) return payload;
  const body = { ...(payload[tag] as Record<string, unknown>), [key]: value };
  return { [tag]: body };
}

/**
 * The op's fields, or `null` for a step whose payload is not an op — the link
 * step, whose payload is a relation and whose proposal is not editable.
 */
function opBody(step: StartWorkStep): Record<string, unknown> | null {
  const payload = step.payload;
  if (typeof payload !== "object" || payload === null) return null;
  const entries = Object.entries(payload as Record<string, unknown>);
  if (entries.length !== 1) return null;
  const [, body] = entries[0]!;
  if (typeof body !== "object" || body === null) return null;
  return body as Record<string, unknown>;
}

export interface StartWork {
  /** The steps, in the order they run. Empty until a flow exists. */
  readonly steps: StartWorkStep[];
  /** Set when the last read or action failed, so the view can say so. */
  readonly error: string | null;
  /** True while an action's command is out, so a button cannot be double-fired. */
  readonly busy: boolean;
  /** True once a read has answered — so "no flow yet" is not drawn while loading. */
  readonly loaded: boolean;
  /** True while the flow runs behind an action's answer and is re-read as it moves (#779). */
  readonly following: boolean;
  /** Read the flow; propose one when `repoId` names where it would go. */
  load(repoId: string | null): Promise<void>;
  run(): Promise<void>;
  retry(stepId: number): Promise<void>;
  skip(stepId: number): Promise<void>;
  amend(stepId: number, payload: unknown): Promise<void>;
  /** Stop following, for a stepper that goes away. The flow runs on regardless. */
  stop(): void;
}

/**
 * How often the stepper re-reads the flow while it moves.
 *
 * Since #779 *Run*, *Retry* and *Skip* answer at once and the backend runs the
 * steps behind the answer, one at a time through the write queue. The rows
 * move underneath -- a step is marked `running` while its dispatch is out --
 * and re-reading them is how the stepper draws each step as it lands (story
 * 11: a slow step must be distinguishable from a stuck one).
 */
const POLL_MS = 700;

/**
 * Whether the backend is still taking the flow on: a step is out, or the
 * first step not done yet is one nothing has started -- between two steps,
 * or before the first. A failed or queued step is where the run stopped, and
 * a flow with every step done has nothing left to run.
 */
export function moving(steps: readonly StartWorkStep[]): boolean {
  const current = steps.find((step) => demandOf(step.outcome) !== "done");
  return current !== undefined && (current.outcome === "running" || current.outcome === "pending");
}

/** The five commands the stepper calls, injectable so a test needs no bridge. */
export interface StartWorkApi {
  flow: (entityId: string, repoId: string | null) => Promise<StartWorkStep[]>;
  run: (entityId: string) => Promise<StartWorkStep[]>;
  retry: (stepId: number) => Promise<StartWorkStep[]>;
  skip: (stepId: number) => Promise<StartWorkStep[]>;
  amend: (stepId: number, payload: unknown) => Promise<StartWorkStep[]>;
}

export const BRIDGE: StartWorkApi = {
  flow: (entityId, repoId) => startWorkFlow(entityId, repoId),
  run: (entityId) => startWorkRun(entityId),
  retry: (stepId) => startWorkRetry(stepId),
  skip: (stepId) => startWorkSkip(stepId),
  amend: (stepId, payload) => startWorkAmend(stepId, payload),
};

export function createStartWork(entityId: string, api: StartWorkApi = BRIDGE): StartWork {
  const state = $state<{
    steps: StartWorkStep[];
    error: string | null;
    busy: boolean;
    loaded: boolean;
    following: boolean;
  }>({ steps: [], error: null, busy: false, loaded: false, following: false });

  /**
   * The generation of the newest action, so a read that resolves late cannot
   * overwrite the action's own answer -- the same device the suggestion
   * tray's `token` is.
   */
  let generation = 0;
  let poll: ReturnType<typeof setInterval> | null = null;

  async function load(repoId: string | null): Promise<void> {
    try {
      state.steps = await api.flow(entityId, repoId);
      state.error = null;
    } catch (error) {
      // The steps are left where they were rather than emptied: a failed read
      // is not evidence the flow is gone, and a stepper that blinked to nothing
      // is the thing story 16 exists to prevent.
      state.error = ipcErrorMessage(error);
    } finally {
      state.loaded = true;
    }
  }

  function stop() {
    if (poll !== null) clearInterval(poll);
    poll = null;
    state.following = false;
  }

  /**
   * Re-read the flow while it moves. A read applies only while no newer action
   * has answered, so a slow one can never overwrite that answer; the first
   * read that finds the flow stopped or done ends the following.
   */
  function follow() {
    stop();
    state.following = true;
    const mine = generation;
    poll = setInterval(() => {
      void (async () => {
        let steps: StartWorkStep[];
        try {
          steps = await api.flow(entityId, null);
        } catch {
          // A failed read changes nothing; the next one asks again.
          return;
        }
        if (mine !== generation || poll === null) return;
        state.steps = steps;
        if (!moving(steps)) stop();
      })();
    }, POLL_MS);
  }

  /**
   * Run one action and take what it answers.
   *
   * Every one of these commands returns the flow as it now stands, so there is
   * no second read to disagree with the first — and a failure leaves the rows
   * alone, because an action that was refused did not change them. Those that
   * run the flow on (`behind`) are then followed while it moves.
   */
  async function act(run: () => Promise<StartWorkStep[]>, behind = false): Promise<void> {
    if (state.busy) return;
    state.busy = true;
    try {
      const answer = await run();
      generation += 1;
      state.steps = answer;
      state.error = null;
      if (behind) follow();
    } catch (error) {
      generation += 1;
      state.error = ipcErrorMessage(error);
    } finally {
      state.busy = false;
    }
  }

  return {
    get steps() {
      return state.steps;
    },
    get error() {
      return state.error;
    },
    get busy() {
      return state.busy;
    },
    get loaded() {
      return state.loaded;
    },
    get following() {
      return state.following;
    },
    load,
    run: () => act(() => api.run(entityId), true),
    retry: (stepId) => act(() => api.retry(stepId), true),
    skip: (stepId) => act(() => api.skip(stepId), true),
    amend: (stepId, payload) => act(() => api.amend(stepId, payload)),
    stop,
  };
}
