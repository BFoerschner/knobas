/** The insets of the shown site page's rectangle, or the guess before one is drawn (`insets.ts`). */
import type { SiteInsets } from "../../ipc/site";
import { measure, siteInsetsGuess } from "./insets";

export function measureShown(): SiteInsets {
  const room = document.querySelector(".tf-kept:not([hidden]) .tf-site-room");
  return room === null ? siteInsetsGuess() : measure(room);
}
