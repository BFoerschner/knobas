/**
 * The ticket page's epic line (spec #2227, #2231): the page reads the nearest
 * parent's row through `get_entity` for its title, as the board fills the
 * preview's `parent_title`. The shell keeps one page per ticket, so only a
 * page drawn again with a new id can see a read the next id overtakes; this
 * mounts the page alone and moves its `id` to pin that such a read lands
 * nothing.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test } from "vitest";

import type { EntityDetail } from "../../ipc/entity";
import { until } from "../../shell/test-wait";
import TfTicket from "./TfTicket.svelte";

let target: HTMLDivElement | undefined;
let app: Record<string, unknown> | undefined;

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target?.remove();
});

/** A row as `get_entity` answers it, enough for the page to draw. */
function detail(entity_id: string, title: string, path: string | null): EntityDetail {
  return {
    row: { entity_id, kind: "ticket", source_id: "jira", title, path, updated_at: null, synced_at: null, state: null },
    author: null,
    status: "To Do",
    priority: null,
    body_text: "",
  } as unknown as EntityDetail;
}

function epicFact(): string | null {
  const terms = [...(target?.querySelectorAll(".tf-facts dt") ?? [])].map((dt) => dt.textContent?.trim());
  const at = terms.indexOf("Epic");
  return at < 0 ? null : (target!.querySelectorAll(".tf-facts dd")[at]?.textContent?.trim() ?? null);
}

test("an epic read the next ticket overtakes lands nothing on the next ticket's page", async () => {
  let release: () => void = () => {};
  const gate = new Promise<void>((resolve) => (release = resolve));
  const rows: Record<string, EntityDetail> = {
    "jira:A-1": detail("jira:A-1", "First ticket", "E-1"),
    "jira:E-1": detail("jira:E-1", "First epic", null),
    "jira:B-2": detail("jira:B-2", "Second ticket", "E-2"),
    "jira:E-2": detail("jira:E-2", "Second epic", null),
  };
  const asked: string[] = [];
  const getEntity = (id: string) => {
    asked.push(id);
    const found = Promise.resolve(rows[id]!);
    return id === "jira:E-1" ? gate.then(() => found) : found;
  };
  const never = () => new Promise<never>(() => {});
  const props = $state({
    id: "jira:A-1",
    ready: true,
    ports: { getEntity, listSources: () => Promise.resolve([]), listEntities: never, jobReach: never },
  });
  target = document.createElement("div");
  document.body.append(target);
  app = mount(TfTicket, { target, props });

  await until(() => asked.includes("jira:E-1"), "the first epic was never read");
  props.id = "jira:B-2";
  flushSync();
  await until(() => epicFact() === "E-2 Second epic", `the epic read ${epicFact()}`);
  release();
  await gate;
  await new Promise((resolve) => setTimeout(resolve, 20));
  flushSync();
  expect(epicFact()).toBe("E-2 Second epic");
});
