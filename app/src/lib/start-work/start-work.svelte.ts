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
 * so a proposal carrying a field this list does not name is still shown. What
 * is *not* offered is deliberate: `entity` is the target, and a proposal that
 * repointed itself at another repository would be a different flow.
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
    .filter(([key, value]) => key !== "entity" && typeof value === "string")
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
  /** True while an action is in flight, so a button cannot be double-fired. */
  readonly busy: boolean;
  /** True once a read has answered — so "no flow yet" is not drawn while loading. */
  readonly loaded: boolean;
  /** Read the flow; propose one when `repoId` names where it would go. */
  load(repoId: string | null): Promise<void>;
  run(): Promise<void>;
  retry(stepId: number): Promise<void>;
  skip(stepId: number): Promise<void>;
  amend(stepId: number, payload: unknown): Promise<void>;
}

export function createStartWork(entityId: string): StartWork {
  const state = $state<{
    steps: StartWorkStep[];
    error: string | null;
    busy: boolean;
    loaded: boolean;
  }>({ steps: [], error: null, busy: false, loaded: false });

  async function load(repoId: string | null): Promise<void> {
    try {
      state.steps = await startWorkFlow(entityId, repoId);
      state.error = null;
    } catch (error) {
      // The steps are left where they were rather than emptied: a failed read
      // is not evidence the flow is gone, and a stepper that blinked to nothing
      // is the thing story 16 exists to prevent.
      state.error = message(error);
    } finally {
      state.loaded = true;
    }
  }

  /**
   * Run one action and take what it answers.
   *
   * Every one of these commands returns the flow as it now stands, so there is
   * no second read to disagree with the first — and a failure leaves the rows
   * alone, because an action that was refused did not change them.
   */
  async function act(run: () => Promise<StartWorkStep[]>): Promise<void> {
    if (state.busy) return;
    state.busy = true;
    try {
      state.steps = await run();
      state.error = null;
    } catch (error) {
      state.error = message(error);
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
    load,
    run: () => act(() => startWorkRun(entityId)),
    retry: (stepId) => act(() => startWorkRetry(stepId)),
    skip: (stepId) => act(() => startWorkSkip(stepId)),
    amend: (stepId, payload) => act(() => startWorkAmend(stepId, payload)),
  };
}

/** Whatever a rejection was, as something displayable. */
function message(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const text = (error as { message: unknown }).message;
    if (typeof text === "string") return text;
  }
  return error instanceof Error ? error.message : String(error);
}
