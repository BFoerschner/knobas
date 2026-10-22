/**
 * The navigation the Assets view's tests hand it: `location.hash` read into a
 * state, and `go` that writes it and reads it back at once. The shell's
 * `TfAssetTree` does the same over a derived address; the view only asks for
 * `route` and `go`.
 */
import { parseHash, type Route } from "../shell/router";
import type { AssetsNav } from "./tree";

export function createTestNav(): AssetsNav {
  const state = $state<{ route: Route }>({ route: parseHash(location.hash) });
  return {
    get route() {
      return state.route;
    },
    go(hash: string) {
      if (location.hash !== hash) location.hash = hash;
      state.route = parseHash(location.hash);
    },
  };
}
