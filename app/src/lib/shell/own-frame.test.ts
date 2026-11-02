import { expect, test } from "vitest";

import { framedByOwnOrigin, refuseOwnFrame } from "./own-frame";

// A window chain: each entry is an origin, the first the top; `null` is a
// window the browser keeps unreadable (another origin's).
function chain(...origins: (string | null)[]): Window {
  let parent: Window | null = null;
  let w!: Window;
  for (const origin of origins) {
    const node = {} as { parent: Window; location: { origin: string } };
    node.parent = parent ?? (node as unknown as Window);
    Object.defineProperty(node, "location", {
      get: () => {
        if (origin === null) throw new DOMException("Blocked a frame from accessing a cross-origin frame.", "SecurityError");
        return { origin };
      },
    });
    parent = node as unknown as Window;
    w = parent;
  }
  return w;
}

test("the app refuses to run inside a frame of its own origin, at any depth (#2461)", () => {
  const app = "http://tauri.localhost";
  // A URL embed navigated to the app's origin: the app's window is its parent.
  expect(framedByOwnOrigin(chain(app, app))).toBe(true);
  // ...or a frame of its own the embed's page made: the app's window is further up, past the page.
  expect(framedByOwnOrigin(chain(app, null, app))).toBe(true);
  expect(framedByOwnOrigin(chain("tauri://localhost", null, null, "tauri://localhost"))).toBe(true);
  expect(() => refuseOwnFrame(chain(app, app))).toThrow(/own origin/);
});

test("the app runs as a window of its own, under another origin's frame, and D2's frame runs under the app (#2461)", () => {
  expect(framedByOwnOrigin(chain("http://localhost:1420"))).toBe(false);
  // A browser tool that frames the dev server: its window is another origin's.
  expect(framedByOwnOrigin(chain(null, "http://localhost:1420"))).toBe(false);
  // D2's frame: an opaque origin under the app, which it cannot read.
  expect(framedByOwnOrigin(chain(null, "null"))).toBe(false);
  expect(() => refuseOwnFrame(chain("http://localhost:1420"))).not.toThrow();
});
