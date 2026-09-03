/**
 * The publish-target section of settings — story 67's *other* half (issue
 * #289).
 *
 * The first publish's dialog is tested in
 * `lib/standup/ProtocolPanel.test.svelte.ts`; this is the surface a reader
 * meets on the day the team moves the parent page, and the two write the same
 * key. Three things are asserted here that the panel's own tests cannot see,
 * because they are claims this component makes on its own:
 *
 * * **it draws what is stored**, and *nothing stored* is a sentence rather
 *   than a blank;
 * * **a read that failed says so** instead of reading as *nothing stored* —
 *   the rule `PassiveSection` records, and the one the panel's
 *   `a target read that failed says so` covers on the other side;
 * * **with two Confluence sources it asks which**, exactly as the dialog does,
 *   because it is the same picker and a settings panel that quietly aimed at
 *   the first configured wiki would move the team's standup to the wrong
 *   instance.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { PublishTarget } from "../ipc/entity";
import StandupSection from "./StandupSection.svelte";

const CONFLUENCE = { adapter_kind: "confluence", write_ops: ["comment", "create_page"] };

function wiki(id: string) {
  return { id, display_name: id.toUpperCase(), adapter_kind: "confluence", enabled: true };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render(
  over: {
    stored?: PublishTarget | null;
    sources?: ReturnType<typeof wiki>[];
    /** Make the stored-target read fail, the way a database still coming up does. */
    readFails?: boolean;
  } = {},
) {
  const written: PublishTarget[] = [];
  let attempts = 0;
  app = mount(StandupSection, {
    target,
    props: {
      ports: {
        standupPublishTarget: () => {
          attempts += 1;
          return over.readFails && attempts === 1
            ? Promise.reject({ code: "not_ready", message: "the database is still starting" })
            : Promise.resolve(over.stored ?? null);
        },
        setStandupPublishTarget: (chosen: PublishTarget) => {
          written.push(chosen);
          return Promise.resolve(chosen);
        },
        listSources: () => Promise.resolve((over.sources ?? [wiki("wiki")]) as never),
        listAdapters: () => Promise.resolve([CONFLUENCE] as never),
        search: () =>
          Promise.resolve({
            interpreted: { text: "", prefix: null, filters: {}, unknown_tokens: [] },
            groups: [
              {
                kind: "page",
                hits: [
                  {
                    entity_id: "wiki2:98500",
                    kind: "page",
                    source_id: "wiki2",
                    updated_at: null,
                    synced_at: "2026-09-03T07:00:00Z",
                    title: "Standup protocols",
                    path: "Engineering",
                    rank: 1,
                    snippet: [],
                  },
                ],
              },
            ],
            total: 1,
            took_ms: 1,
            coverage: [],
          } as never),
      },
    },
  });
  flushSync();
  return { written };
}

/** Let the section's `Promise.all` resolve, then render. */
async function settle() {
  for (let tick = 0; tick < 10; tick += 1) await Promise.resolve();
  flushSync();
}

function text(): string {
  return (target.textContent ?? "").replace(/\s+/g, " ").trim();
}

function button(label: string): HTMLButtonElement | undefined {
  return [...target.querySelectorAll("button")].find(
    (candidate) => candidate.textContent?.trim() === label,
  );
}

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

test("a profile nobody has published from says so rather than showing a blank", async () => {
  render();
  await settle();

  expect(text()).toContain("Nothing is stored yet");
  // And there is nothing to save until somebody answers the picker.
  expect(button("Save")?.disabled).toBe(true);
});

test("the stored target is what the section draws", async () => {
  render({ stored: { source_id: "wiki", parent: "wiki:98400" } });
  await settle();

  expect(text()).toContain("wiki:98400");
  expect(text()).toContain("in wiki");
  expect(text()).not.toContain("Nothing is stored yet");
});

test("with two Confluence sources the section asks which, exactly as the dialog does", async () => {
  // Story 68 on the settings side. A preselected instance here is an answer
  // nobody gave, and the page it moves is the team's standup.
  render({ sources: [wiki("wiki"), wiki("wiki2")] });
  await settle();

  const select = target.querySelector<HTMLSelectElement>("select");
  expect(select?.value).toBe("");
  expect([...(select?.options ?? [])].map((option) => option.value)).toEqual(["", "wiki", "wiki2"]);
  expect(button("Save")?.disabled).toBe(true);
});

test("the target the picker settles on is what is written to the setting", async () => {
  const { written } = render({ sources: [wiki("wiki"), wiki("wiki2")] });
  await settle();

  const select = target.querySelector<HTMLSelectElement>("select")!;
  select.value = "wiki2";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();

  const box = target.querySelector<HTMLInputElement>("input[type=search]")!;
  box.value = "standup";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  [...target.querySelectorAll("button")]
    .find((candidate) => candidate.textContent?.includes("Standup protocols"))
    ?.click();
  await settle();

  button("Save")?.click();
  await settle();

  expect(written).toEqual([{ source_id: "wiki2", parent: "wiki2:98500" }]);
  // And what was written is what the section now says is stored.
  expect(text()).toContain("wiki2:98500");
});

test("a read that failed says so and offers Retry, rather than claiming nothing is stored", async () => {
  // "Nothing is stored" is a claim about the database, and a section that
  // could not ask has not earned it.
  render({ readFails: true, stored: { source_id: "wiki", parent: "wiki:98400" } });
  await settle();

  expect(text()).toContain("the database is still starting");
  expect(text()).not.toContain("Nothing is stored yet");
  expect(target.querySelector("select")).toBeNull();

  button("Retry")!.click();
  await settle();
  expect(text()).toContain("wiki:98400");
});
