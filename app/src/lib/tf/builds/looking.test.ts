import { afterEach, describe, expect, it } from "vitest";

import { buildsLooked, forgetLooking, startLooking } from "./looking.svelte";
import type { LookingAgain } from "../../ipc/sources";

/** A bridge whose `looking:again` the test tells by hand. */
function bridge() {
  const handlers: ((event: { payload: unknown }) => void)[] = [];
  return {
    listen: (event: string, handler: (event: { payload: unknown }) => void) => {
      if (event === "looking:again") handlers.push(handler);
      return Promise.resolve(() => {});
    },
    tell: (payload: LookingAgain) => handlers.forEach((handler) => handler({ payload })),
  };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

afterEach(() => forgetLooking());

describe("what knobas looks at again after a branch moved (#2585)", () => {
  it("starts from what the backend looks at now, so a page opened during a schedule says so", async () => {
    const { listen } = bridge();
    startLooking(listen, () => Promise.resolve([{ scope: "forge:repo:tide/pay", code: true, builds: true, items: [] }]));
    await settle();
    expect(buildsLooked("forge:repo:tide/pay")).toBe(true);
    expect(buildsLooked("forge:repo:tide/other")).toBe(false);
  });

  it("follows each start, stopped side and end the event tells", async () => {
    const { listen, tell } = bridge();
    startLooking(listen, () => Promise.resolve([]));
    await settle();
    expect(buildsLooked("forge:repo:tide/pay")).toBe(false);
    tell({ scope: "forge:repo:tide/pay", code: true, builds: true, items: [] });
    expect(buildsLooked("forge:repo:tide/pay")).toBe(true);
    tell({ scope: "forge:repo:tide/pay", code: true, builds: false, items: [] });
    expect(buildsLooked("forge:repo:tide/pay")).toBe(false);
    tell({ scope: "forge:repo:tide/pay", code: false, builds: false, items: [] });
    expect(buildsLooked("forge:repo:tide/pay")).toBe(false);
  });

  it("an event heard before the first read answers is not undone by that read", async () => {
    const { listen, tell } = bridge();
    let answer: (rows: LookingAgain[]) => void = () => {};
    startLooking(listen, () => new Promise((resolve) => (answer = resolve)));
    tell({ scope: "forge:repo:tide/pay", code: false, builds: false, items: [] });
    answer([{ scope: "forge:repo:tide/pay", code: true, builds: true, items: [] }]);
    await settle();
    expect(buildsLooked("forge:repo:tide/pay"), "the schedule ended after the read was asked").toBe(false);
  });
});
