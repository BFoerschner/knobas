/**
 * A spec drawn small (ux31): its filter, which has the app rank every endpoint and schema (#2535) and marks the
 * letters the ranking matched in each row's path and summary.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

// The filter ranks through the app's ranking (#2535): here a letters-in-order stand-in.
vi.mock("../../ipc/search", async (actual) => ({ ...(await actual<object>()), ...(await import("../filter/test-fuzzy")).fuzzyIpc() }));

import TfOasInline from "./TfOasInline.svelte";
import { localEngine } from "../../render/openapi-port";
import { PETS_30_YAML } from "../../render/openapi-fixture";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

async function settle() {
  for (let round = 0; round < 8; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const rowPaths = () => [...target.querySelectorAll(".tf-oas-irow .tf-oas-rpath")].map((el) => el.textContent);
const spelled = (el: Element) => [...el.querySelectorAll("mark")].map((m) => m.textContent).join("").toLowerCase();

test("the filter has the app rank the endpoints and schemas, letters in order, and marks them in the row (#2535)", async () => {
  view = mount(TfOasInline, { target, props: { text: PETS_30_YAML, engine: localEngine() } });
  await settle();
  expect(rowPaths().length).toBeGreaterThan(1);
  const field = target.querySelector<HTMLInputElement>("input[aria-label='Filter endpoints and schemas']")!;
  // No substring holds "lstpts"; its letters stand in order in "List all pets", the summary of GET /pets.
  field.value = "lstpts";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  const rows = [...target.querySelectorAll(".tf-oas-irow")];
  expect(rows.length).toBeGreaterThan(0);
  expect(rows[0]!.querySelector(".tf-oas-rsum")?.textContent).toBe("List all pets");
  expect(spelled(rows[0]!)).toBe("lstpts");
});
