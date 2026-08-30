/**
 * The stepper, from the reader's side (issue #44).
 *
 * **The failure paths first**, for the reason the backend's battery gives: a
 * stepper whose happy path draws is close to vacuous. What matters is that a
 * stopped flow *says where it stopped*, that the steps behind it are visibly
 * untouched, that a queued step reads as neither a success nor a failure, and
 * that the actions which could run a step over an earlier failure are not
 * offered at all.
 *
 * Every assertion is on what is on screen or on which command was invoked --
 * never on the store's internals.
 */
import { beforeEach, expect, test, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";

import type {
  StartWorkOutcome,
  StartWorkStep,
  StartWorkStepKind,
} from "../ipc/entity";

const calls: { command: string; args: unknown }[] = [];
let flow: StartWorkStep[] = [];
/** Set to make the next command reject, so the error path is reachable. */
let reject: unknown = null;

function record<T>(command: string, args: unknown, value: T): Promise<T> {
  calls.push({ command, args });
  if (reject !== null) {
    const error = reject;
    reject = null;
    return Promise.reject(error);
  }
  return Promise.resolve(value);
}

vi.mock("../ipc/entity", () => ({
  // The write the ticket detail's status select queues (#179). Not what this
  // file is about, so it refuses.
  submitWrite: () => Promise.reject(new Error("no write in this test")),
  getEntity: () =>
    record("get_entity", null, {
      row: {
        entity_id: "jira:PAY-231",
        kind: "ticket",
        source_id: "jira",
        title: "payout dashboard latency",
        updated_at: null,
        synced_at: "2026-08-29T09:00:00Z",
      },
    }),
  // The Tickets tile's read (#178), reached through the room this flow
  // opens over. Not what this file is about, so it answers with nothing.
  miniBoard: () => Promise.resolve({ columns: [], sources: [] }),
  listEntities: () =>
    record("list_entities", null, {
      rows: [
        {
          entity_id: "gitea:tidewater/payout-service",
          kind: "repo",
          source_id: "gitea",
          title: "tidewater/payout-service",
          updated_at: null,
          synced_at: "2026-08-29T09:00:00Z",
        },
      ],
      total: 1,
    }),
  startWorkFlow: (entityId: string, repoId: string | null) =>
    record("start_work_flow", { entityId, repoId }, flow),
  startWorkRun: (entityId: string) =>
    record("start_work_run", { entityId }, flow),
  startWorkRetry: (stepId: number) =>
    record("start_work_retry", { stepId }, flow),
  startWorkSkip: (stepId: number) =>
    record("start_work_skip", { stepId }, flow),
  startWorkAmend: (stepId: number, payload: unknown) =>
    record("start_work_amend", { stepId, payload }, flow),
}));

const { default: StartWork } = await import("./StartWork.svelte");
const { demandOf, edited, fieldsOf } = await import("./start-work.svelte");

/** One step, with the payload the plan would have stored. */
function step(
  id: number,
  kind: StartWorkStepKind,
  outcome: StartWorkOutcome,
  detail: string | null = null,
): StartWorkStep {
  const payloads: Record<StartWorkStepKind, unknown> = {
    create_branch: {
      CreateBranch: {
        entity: "gitea:tidewater/payout-service",
        name: "feature/PAY-231-payout-dashboard-latency",
        from_ref: "main",
      },
    },
    create_pull_request: {
      CreatePullRequest: {
        entity: "gitea:tidewater/payout-service",
        title: "WIP: payout dashboard latency",
        body: "PAY-231",
        head: "feature/PAY-231-payout-dashboard-latency",
        base: "main",
      },
    },
    link_pull_request: { relation: "implements" },
    transition: {
      Transition: { entity: "jira:PAY-231", status: "In Progress" },
    },
  };
  return {
    id,
    ticket_id: "jira:PAY-231",
    step: kind,
    position: id - 1,
    outcome,
    payload: payloads[kind],
    write_id: null,
    detail,
    updated_at: "2026-08-29T09:00:00Z",
  };
}

/** The whole sequence, with the outcomes given. */
function sequence(
  outcomes: [
    StartWorkOutcome,
    StartWorkOutcome,
    StartWorkOutcome,
    StartWorkOutcome,
  ],
  details: (string | null)[] = [],
) {
  const kinds: StartWorkStepKind[] = [
    "create_branch",
    "create_pull_request",
    "link_pull_request",
    "transition",
  ];
  return kinds.map((kind, index) =>
    step(index + 1, kind, outcomes[index]!, details[index] ?? null),
  );
}

function render() {
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(StartWork, {
    target,
    props: {
      entityId: "jira:PAY-231",
      onnavigate: () => {},
      onclose: () => {},
    },
  });
  flushSync();
  return {
    text: () => (target.textContent ?? "").replace(/\s+/g, " "),
    buttons: () =>
      [...target.querySelectorAll<HTMLButtonElement>("button")].map((b) =>
        (b.textContent ?? "").trim(),
      ),
    button: (label: string) =>
      [...target.querySelectorAll<HTMLButtonElement>("button")].find(
        (b) => (b.textContent ?? "").trim() === label,
      ) ?? null,
    /** The steps, as `<li>` elements, in the order they are drawn. */
    steps: () => [...target.querySelectorAll<HTMLElement>(".sw-step")],
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

/** Let the mocked reads land. */
async function settle() {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
  flushSync();
}

beforeEach(() => {
  calls.length = 0;
  flow = [];
  reject = null;
  document.body.replaceChildren();
});

// -- the failure paths -----------------------------------------------------

/**
 * Story 16. A flow that stopped part way says *where*, in the source's own
 * words, and the steps after it are still visibly waiting -- so the reader
 * knows what is left to finish by hand.
 */
test("a stopped flow says what the source said and leaves the rest visibly untouched", async () => {
  flow = sequence(
    ["succeeded", "failed", "pending", "pending"],
    [null, "a pull request already exists for this head"],
  );
  const screen = render();
  await settle();

  expect(screen.text()).toContain(
    "a pull request already exists for this head",
  );
  const steps = screen.steps();
  expect(steps[0]!.className).toContain("succeeded");
  expect(steps[1]!.className).toContain("failed");
  expect(steps[2]!.className).toContain("pending");
  expect(steps[3]!.className).toContain("pending");
  screen.done();
});

/**
 * Story 12, arriving through the UI. *Retry* and *Skip* belong to the step the
 * flow stopped at and to no other: a stepper that offered them everywhere would
 * invite running the transition over a branch that was never created.
 */
test("retry and skip are offered on the stopped step alone", async () => {
  flow = sequence(["succeeded", "failed", "pending", "pending"], [null, "no"]);
  const screen = render();
  await settle();

  const steps = screen.steps();
  const retries = steps.map((li) =>
    [...li.querySelectorAll("button")].some((b) =>
      b.textContent?.includes("Retry"),
    ),
  );
  expect(retries, "retry belongs to the stopped step and to no other").toEqual([
    false,
    true,
    false,
    false,
  ]);
  const skips = steps.map((li) =>
    [...li.querySelectorAll("button")].some(
      (b) => b.textContent?.trim() === "Skip",
    ),
  );
  expect(skips, "so does skip").toEqual([false, true, false, false]);
  screen.done();
});

/**
 * A queued step is neither. Drawn as a failure it invites a retry of a write
 * already on its way; drawn as a success it tells the reader work happened that
 * has not.
 */
test("a queued step is drawn as neither a success nor a failure", async () => {
  flow = sequence(["queued", "pending", "pending", "pending"]);
  const screen = render();
  await settle();

  const first = screen.steps()[0]!;
  expect(first.className).toContain("queued");
  expect(first.textContent).toContain("it will go on its own");
  // The word the state carries is what the reader's eye lands on, and it is
  // driven by `demandOf` -- so it, not merely the row's outcome class, is what
  // this pins.
  const state = first.querySelector(".sw-state")!;
  expect(
    state.className,
    "a queued write needs the network, not the reader",
  ).not.toContain("decide");
  expect(state.className, "and it has not happened yet either").not.toContain(
    "done",
  );
  expect(state.className).toContain("waiting");
  expect(
    [...first.querySelectorAll("button")].some((b) =>
      b.textContent?.includes("Retry"),
    ),
    "there is nothing to retry -- the write is with the source's queue",
  ).toBe(false);
  screen.done();
});

/** A refusal from the backend is shown, and the rows on screen are left alone. */
test("a refused action is reported without emptying the stepper", async () => {
  flow = sequence(["succeeded", "failed", "pending", "pending"], [null, "no"]);
  const screen = render();
  await settle();

  reject = { message: "the create_branch step is where this flow stopped" };
  screen.button("Retry this step")?.click();
  await settle();

  expect(screen.text()).toContain("where this flow stopped");
  expect(screen.steps(), "the flow is still on screen").toHaveLength(4);
  screen.done();
});

// -- nothing happens until the reader says so ------------------------------

/**
 * The whole point of a stepper. Opening the address proposes and shows; it does
 * not dispatch.
 */
test("opening the address shows the sequence and runs nothing", async () => {
  flow = sequence(["pending", "pending", "pending", "pending"]);
  const screen = render();
  await settle();

  expect(calls.map((call) => call.command)).not.toContain("start_work_run");
  expect(screen.text()).toContain("Nothing has happened yet");
  expect(screen.button("Run the sequence")).not.toBeNull();
  screen.done();
});

test("pressing Run is what dispatches, and it names the ticket", async () => {
  flow = sequence(["pending", "pending", "pending", "pending"]);
  const screen = render();
  await settle();

  screen.button("Run the sequence")?.click();
  await settle();

  const run = calls.find((call) => call.command === "start_work_run");
  expect(run?.args).toEqual({ entityId: "jira:PAY-231" });
  screen.done();
});

/**
 * Story 4 and 7: a bad automatic name is not a commitment. The edited value is
 * what is sent, under the step's own id.
 */
test("an edited proposal is what the amend carries", async () => {
  flow = sequence(["pending", "pending", "pending", "pending"]);
  const screen = render();
  await settle();

  screen.steps()[0]!.querySelector<HTMLButtonElement>("button")?.click();
  flushSync();
  const input = screen.steps()[0]!.querySelector<HTMLInputElement>("input");
  expect(input?.value).toBe("feature/PAY-231-payout-dashboard-latency");
  input!.value = "knobas-PAY-231";
  input!.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  screen.button("Save")?.click();
  await settle();

  const amend = calls.find((call) => call.command === "start_work_amend");
  expect(amend?.args).toEqual({
    stepId: 1,
    payload: {
      CreateBranch: {
        entity: "gitea:tidewater/payout-service",
        name: "knobas-PAY-231",
        from_ref: "main",
      },
    },
  });
  screen.done();
});

/**
 * Story 5. A ticket does not say which repository its work belongs in, so the
 * reader chooses -- and nothing is proposed until they have.
 */
test("with no flow yet the reader is asked which repository, and nothing is proposed", async () => {
  flow = [];
  const screen = render();
  await settle();

  expect(screen.text()).toContain("Choose the repository");
  const proposed = calls.filter(
    (call) =>
      call.command === "start_work_flow" &&
      (call.args as { repoId: string | null }).repoId !== null,
  );
  expect(
    proposed,
    "a flow was proposed before the reader picked a repository",
  ).toHaveLength(0);

  screen.button("Propose the sequence")?.click();
  await settle();
  expect(
    calls.some(
      (call) =>
        call.command === "start_work_flow" &&
        (call.args as { repoId: string | null }).repoId ===
          "gitea:tidewater/payout-service",
    ),
  ).toBe(true);
  screen.done();
});

// -- the pure parts --------------------------------------------------------

/** The three demands a reader can act on, and the one that means "finished". */
test("the finished footer claims the ticket moved only when the transition succeeded", async () => {
  flow = sequence(["succeeded", "succeeded", "succeeded", "skipped"]);
  const page = render();
  await settle();
  expect(page.text()).toContain("Every step is settled. The ticket was not moved.");
  expect(page.text()).not.toContain("The ticket is In Progress.");
  page.done();

  flow = sequence(["succeeded", "succeeded", "succeeded", "succeeded"]);
  const moved = render();
  await settle();
  expect(moved.text()).toContain("Every step is finished. The ticket is In Progress.");
  moved.done();
});

test("an outcome asks the reader for exactly one thing", () => {
  expect(demandOf("succeeded")).toBe("done");
  expect(demandOf("skipped")).toBe("done");
  expect(demandOf("failed")).toBe("decide");
  expect(demandOf("queued")).toBe("waiting");
  expect(demandOf("pending")).toBe("waiting");
  expect(demandOf("running")).toBe("running");
});

/**
 * The target is not editable. A proposal that repointed itself at another
 * repository would be a different flow, and the queue would refuse it as an
 * amendment anyway.
 */
test("a step's editable fields never include its target", () => {
  const fields = fieldsOf(step(1, "create_branch", "pending"));
  expect(fields.map((field) => field.key).sort()).toEqual(["from_ref", "name"]);
});

/** The link step carries a relation rather than an op, so it offers no editor. */
test("the link step has nothing to edit", () => {
  expect(fieldsOf(step(3, "link_pull_request", "pending"))).toEqual([]);
});

/** An edit replaces one field and leaves the op's tag and its target alone. */
test("editing one field keeps the op it is a field of", () => {
  const before = step(2, "create_pull_request", "pending");
  const after = edited(before, "title", "payout dashboard latency") as Record<
    string,
    Record<string, string>
  >;
  expect(Object.keys(after)).toEqual(["CreatePullRequest"]);
  expect(after.CreatePullRequest!.title).toBe("payout dashboard latency");
  expect(after.CreatePullRequest!.entity).toBe(
    "gitea:tidewater/payout-service",
  );
  expect(after.CreatePullRequest!.head).toBe(
    "feature/PAY-231-payout-dashboard-latency",
  );
});
