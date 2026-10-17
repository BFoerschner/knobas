import { expect, test } from "vitest";

import { HOME, homeIfBlank } from "./home";

function at(hash: string) {
  const replaced: string[] = [];
  return {
    hash,
    replaced,
    replace(url: string) {
      replaced.push(url);
    },
  };
}

test("a window opened on no address lands on Tickets, at #/tickets (#881)", () => {
  for (const blank of ["", "#", "#/"]) {
    const place = at(blank);
    homeIfBlank(place);
    expect(place.replaced).toEqual([HOME]);
  }
  expect(HOME).toBe("#/tickets");
});

test("an address the window was opened on is kept", () => {
  for (const hash of ["#/job/PAY-231", "#/settings/backup", "#/tickets/mine"]) {
    const place = at(hash);
    homeIfBlank(place);
    expect(place.replaced).toEqual([]);
  }
});
