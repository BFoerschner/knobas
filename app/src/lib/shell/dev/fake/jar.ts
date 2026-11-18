/**
 * `?fake-jar` (ux19): the JAR view's fixture (`render/jar-fixture.ts`) in the payout repository's `build/libs/`:
 * `views/repos.ts` lists the archives, and these answer the reads the view makes of them. `?fake-jar=nojava` finds no
 * Java, `?fake-jar=oldjava` a Java 11.
 */

import { JAR_FILES, jarAnswers } from "../../../render/jar-fixture";
import type { Handlers } from "../fake-tauri";

export { JAR_FILES };

/** A fake JAR's size, where the path is one and `?fake-jar` is on. */
export function jarSize(path: string): number | undefined {
  return new URLSearchParams(globalThis.location?.search).has("fake-jar") ? JAR_FILES[path] : undefined;
}

export function handlers(params: URLSearchParams = new URLSearchParams()): Handlers {
  return { ...jarAnswers(params.get("fake-jar")) };
}
