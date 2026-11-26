/**
 * *Point to a file…* (the XSD design, section 17): its filter, which has the app rank the schema files (#2535), keeps
 * what fits first, marks the letters the ranking matched, and draws nothing until its first answer.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test, vi } from "vitest";

// The filter ranks through the app's ranking (#2535): here a letters-in-order stand-in.
vi.mock("../../../ipc/search", async (actual) => ({ ...(await actual<object>()), ...(await import("../../filter/test-fuzzy")).fuzzyIpc() }));

import TfXsdPoint from "./TfXsdPoint.svelte";
import { ranked } from "../../filter/test-fuzzy";
import type { Candidate } from "./xsd-point";

let view: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (view) unmount(view);
  view = null;
});

const here = (path: string, declares = "urn:pay"): Candidate => ({ repo: "r", repoName: null, path, declares });
const CANDIDATES = [here("schemas/common/types.xsd"), here("schemas/payment-order.xsd"), here("schemas/payout.xsd", "urn:other")];
const names = () => [...document.querySelectorAll(".tf-xsd-pointrow .tf-xsd-pointname")].map((el) => el.textContent);

test("the picker has the app rank the files by the location's name, marks the matched letters, and draws nothing before the answer (#2535)", async () => {
  view = mount(TfXsdPoint, { target: document.body, props: { namespace: "urn:pay", location: "pmtord.xsd", candidates: CANDIDATES, onpick: () => {}, onclose: () => {} } });
  flushSync();
  // The location's name is typed as it opens: no row until the ranking answers, so none moves when it does.
  expect(names()).toEqual([]);
  await ranked();
  // No substring holds "pmtord.xsd"; its letters stand in order in "payment-order.xsd".
  expect(names()[0]).toBe("payment-order.xsd");
  const first = document.querySelector(".tf-xsd-pointrow .tf-xsd-pointname")!;
  expect([...first.querySelectorAll("mark")].map((m) => m.textContent).join("").toLowerCase()).toBe("pmtord.xsd");

  // A name nothing holds lists every file, what fits first.
  const field = document.querySelector<HTMLInputElement>("input[aria-label='Find a schema file']")!;
  field.value = "zzz";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  await ranked();
  expect(names()).toEqual(["types.xsd", "payment-order.xsd", "payout.xsd"]);
});
