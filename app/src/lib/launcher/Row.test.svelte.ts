/**
 * The launcher row's **path** segment (#284).
 *
 * A Confluence page's title is not enough on its own: five spaces can each
 * hold a *Runbook*, and the thing that tells them apart is where the page
 * sits. The row draws an ancestor path under the title when the search hit
 * carries one — and, per ADR-0007's failure direction, nothing at all when it
 * does not, which is every ticket, build and commit in the mirror.
 *
 * The string itself is joined in SQL by `knobas_core::ancestor_path_read!` and
 * arrives on `EntityRow.path`, so what is under test here is the row's own
 * contract: given a path it draws one, given none it draws none, and it draws
 * the string as **text** whatever a page title contains (roadmap §4 gotcha 7).
 *
 * Mounted directly rather than through `Launcher`, because the two wiring
 * lines that feed it (`Results.svelte`, `Board.svelte`) are one field access
 * each and the rendering rule is what can actually be got wrong.
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

test("a row whose hit carries an ancestor path draws it under the title", () => {
  const screen = render({ path: "Engineering › Payments" });
  expect(screen.target.querySelector(".pa")?.textContent).toBe("Engineering › Payments");
  // The title is still the title: the path is an addition, not a replacement.
  expect(screen.target.textContent).toContain("SEPA payout retry design");
  screen.done();
});

test("a row with no path draws none rather than an empty line", () => {
  // Every spelling of "no path" a hit can arrive with: the field absent (an
  // older stored row, which `#[serde(default)]` decodes), the ADR-0007 miss
  // `null`, and the empty string a joined path can never actually be —
  // `string_agg` over no rows is null — but which a future caller might pass.
  for (const props of [{}, { path: null }, { path: "" }]) {
    const screen = render(props);
    expect(screen.target.querySelector(".pa"), JSON.stringify(props)).toBeNull();
    screen.done();
  }
});

/**
 * A path is source text — page titles somebody typed, `<script>` included.
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
