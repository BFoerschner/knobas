/**
 * `?fake-jar` (ux19): the JAR view's fixture (`render/jar-fixture.ts`) in the payout repository's `build/libs/`:
 * `views/repos.ts` lists the archives, and these answer the reads the view makes of them. `?fake-jar=nojava` finds no
 * Java, `?fake-jar=oldjava` a Java 11.
 */

import { JAR_FILES, jarAnswers } from "../../../render/jar-fixture";
import type { JavaRuntime } from "../../../ipc/app";
import type { Handlers } from "../fake-tauri";

export { JAR_FILES };

/** A fake JAR's size, where the path is one and `?fake-jar` is on. */
export function jarSize(path: string): number | undefined {
  return new URLSearchParams(globalThis.location?.search).has("fake-jar") ? JAR_FILES[path] : undefined;
}

/** `?fake-long-paths`' Java (the renderers-columns pass, 2026-11-20): deep under a profile with spaces, as Windows installs one. */
const LONG_JAVA = String.raw`C:\Users\someone with a long name\AppData\Local\Programs\Eclipse Adoptium\jdk-21.0.4.7-hotspot\bin\java.exe`;

export function handlers(params: URLSearchParams = new URLSearchParams()): Handlers {
  const answers = jarAnswers(params.get("fake-jar"));
  if (!params.has("fake-long-paths")) return { ...answers };
  const long = (rt: JavaRuntime): JavaRuntime => (rt.java === null ? rt : { ...rt, java: { ...rt.java, path: LONG_JAVA } });
  return {
    ...answers,
    java_runtime: (args: Record<string, unknown>) => long(answers.java_runtime(args)),
    set_java_path: (args: Record<string, unknown>) => long(answers.set_java_path(args)),
  };
}
