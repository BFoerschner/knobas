/// <reference lib="webworker" />
/** The highlighting worker (#2254): `highlight-core.ts` on the worker's own thread. */
import { createHighlightCore } from "./highlight-core";
import type { HighlightMessage } from "./port";

const core = createHighlightCore({ post: (reply) => self.postMessage(reply) });
self.addEventListener("message", (event: MessageEvent<HighlightMessage>) => core.handle(event.data));
