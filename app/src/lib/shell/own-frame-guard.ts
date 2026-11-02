/**
 * Imported first by every page the app builds (#2461): a page framed under
 * its own origin throws here, before any other module of it runs
 * (`own-frame.ts` says why).
 */
import { refuseOwnFrame } from "./own-frame";

refuseOwnFrame();
