/**
 * The launcher row's **path** segment (#284).
 *
 * A Confluence page's title is not enough on its own: five spaces can each
 * hold a *Runbook*, and the thing that tells them apart is where the page
 * sits. The row therefore draws an ancestor path under the title when it is
 * given one — and, per ADR-0007's failure direction, nothing at all when it is
 * not, which is every ticket, build and commit in the mirror.
 *
 * Mounted directly rather than through `Launcher`, because what is under test
 * is the row's own contract: given a path it draws one, given none it draws
 * none, and the string is drawn as **text** whatever a page title contains
 * (roadmap §4 gotcha 7).
 */
import { flushSync, mount, unmount } from "svelte";
import { expect, test, vi } from "vitest";

import Row from "./Row.svelte";

function render(over: Record<string, unknown> = {}) {
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(Row, {
    target,
    props: {
      entityId: "confluence:98307",
      kind: "page",
      sourceId: "confluence",
      title: "SEPA payout retry design",
      syncedAt: "2026-08-22T14:30:00Z",
      sources: [],
      selected: false,
      onopen: vi.fn(),
      onhover: vi.fn(),
      ...over,
    },
  });
  flushSync();
  return {
    target,
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

test("a row given an ancestor path draws it under the title", () => {
  const screen = render({ path: "Engineering › Payments" });
  expect(screen.target.querySelector(".pa")?.textContent).toBe(
    "Engineering › Payments",
  );
  // The title is still the title: the path is an addition, not a replacement.
  expect(screen.target.textContent).toContain("SEPA payout retry design");
  screen.done();
});

test("a row with no path draws none rather than an empty line", () => {
  // Both spellings of "no path": the prop omitted, and the miss `null` that
  // `ancestorPath` answers for a record with no readable ancestors.
  for (const props of [{}, { path: null }, { path: "" }]) {
    const screen = render(props);
    expect(
      screen.target.querySelector(".pa"),
      JSON.stringify(props),
    ).toBeNull();
    screen.done();
  }
});

/**
 * A path is source text — a page title somebody typed, `<script>` included.
 * It is interpolated and Svelte escapes it; a row that rendered it as markup
 * would be gotcha 7 arriving through a field nobody was watching.
 */
test("a path that looks like markup is drawn as text", () => {
  const screen = render({ path: "Engineering › <img src=x onerror=alert(1)>" });
  const path = screen.target.querySelector(".pa");
  expect(path?.textContent).toBe("Engineering › <img src=x onerror=alert(1)>");
  expect(path?.querySelector("img")).toBeNull();
  screen.done();
});
