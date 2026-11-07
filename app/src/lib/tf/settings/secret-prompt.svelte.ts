/**
 * **The credential prompt over the page you are on** (F-048, F-638): a press on *Enter token…* or *Re-enter password…*
 * from the status bar, the Inbox or a build's banner asks here, and the shell mounts `TfSecretPromptHost` over whatever
 * is on screen. The address does not move, so *Cancel* leaves the reader where they were. The links keep their
 * `…/reenter` address, which still opens Settings' page with the prompt up when it is opened raw.
 */
export const secretPrompt = $state<{ source: string | null; returnTo: HTMLElement | null }>({ source: null, returnTo: null });

/** Back to no prompt: a test's reset. */
export function resetSecretPrompt(): void {
  secretPrompt.source = null;
  secretPrompt.returnTo = null;
}

/** The source a `…/reenter` address asks for, or `null` for any other address. */
export function reenterSourceOf(hash: string): string | null {
  const found = /^#\/settings\/source\/([^/]+)\/reenter$/u.exec(hash);
  return found ? decodeURIComponent(found[1]!) : null;
}

/** Ask for `source`'s credential, focus going back to `returnTo` when the prompt shuts. */
export function askForSecret(source: string, returnTo: HTMLElement | null = null): void {
  secretPrompt.source = source;
  secretPrompt.returnTo = returnTo;
}

/** A press in the window on a `…/reenter` link: the prompt, instead of the address. A modified press stays the browser's. */
export function catchReenterPress(event: MouseEvent): void {
  if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
  const link = event.target instanceof Element ? event.target.closest<HTMLAnchorElement>("a[href]") : null;
  const source = link === null ? null : reenterSourceOf(link.getAttribute("href") ?? "");
  if (link === null || source === null) return;
  event.preventDefault();
  // Stops the row around the link following the press as well.
  event.stopPropagation();
  askForSecret(source, link);
}
