/**
 * A source's write in the dialog (`TfEntityAction`): a `create_pull_request`
 * it queues lands on the new pull request's page, as Compare's does (#2584).
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test } from "vitest";

import type { ViewAction } from "../ipc/entity";
import type { QueuedWrite, WriteOpPayload } from "../ipc/sources";
import { creatingOf } from "./pr/creating.svelte";
import TfEntityAction from "./TfEntityAction.svelte";

const REPO = "gitea:tidewater/payout-service";
let view: ReturnType<typeof mount> | null = null;

afterEach(() => {
  if (view) unmount(view);
  view = null;
  document.body.replaceChildren();
});

const settle = async () => {
  for (let i = 0; i < 5; i++) await Promise.resolve();
  flushSync();
};

test("a pull request created from a source's dialog lands on its page, addressed by its write (#2584)", async () => {
  const action = {
    write_op: "create_pull_request",
    label: "Open pull request…",
    fields: [{ name: "title", label: "Title", kind: { type: "text" }, required: true, placeholder: null }],
    confirm: null,
    primary: true,
    danger: false,
    disabled: null,
    with_settings: false,
    values: { head: "spike/sepa-instant", base: "main" },
    entity: REPO,
    line_comments: "none",
  } as unknown as ViewAction;
  const writes: WriteOpPayload[] = [];
  location.hash = "#/branch/x";
  view = mount(TfEntityAction, {
    target: document.body,
    props: {
      action,
      entityId: `${REPO}@spike/sepa-instant`,
      title: "Open pull request",
      what: "open PR",
      submitWrite: async (payload) => {
        writes.push(payload);
        return { id: 5 } as QueuedWrite;
      },
      onclose: () => {},
    },
  });
  await settle();
  const title = document.querySelector<HTMLInputElement>("input[aria-label=Title]")!;
  title.value = "Instant SEPA spike";
  title.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  document.querySelector<HTMLButtonElement>("button[type=submit]")!.click();
  await settle();
  expect(writes).toEqual([{ CreatePullRequest: { entity: REPO, title: "Instant SEPA spike", body: "", head: "spike/sepa-instant", base: "main" } }]);
  expect(location.hash).toBe("#/pr/new/5");
  expect(creatingOf(5)).toMatchObject({ repo: REPO, title: "Instant SEPA spike", head: "spike/sepa-instant", base: "main" });
});
