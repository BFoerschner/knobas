/**
 * A paste event, as a browser delivers one — test support, shared because
 * five suites ask the same question of five different text fields.
 *
 * The question is spec #491 story 13's: a URL pasted into a comment, a section
 * edit, the standup protocol or a worklog draft stays text, and only the note
 * body turns one into a `[[ref]]` (story 12). Answering it needs a real paste
 * to dispatch, and jsdom gives no way to build one:
 *
 * * `DataTransfer` is not constructible there, so `new ClipboardEvent(…)`
 *   cannot be handed the text it is supposed to carry;
 * * jsdom performs **no default paste at all**, so a field that correctly
 *   leaves the event alone inserts nothing, and a test that read the field
 *   afterwards would be measuring the absence of jsdom rather than the
 *   presence of a rule.
 *
 * So a suite checks the two things a field could do about a paste, and neither
 * on its own is enough:
 *
 * 1. **`defaultPrevented`** — did anything take the event over? That catches a
 *    handler that cancels and writes something of its own, which is what the
 *    note body does.
 * 2. **the field's own `value`, before against after** — a handler that
 *    rewrote the field *without* cancelling would sail past (1), and this is
 *    what catches it. In jsdom the platform inserts nothing, so a field that
 *    left the paste alone is one whose value did not move.
 *
 * And then, where the field sends its value somewhere, that what goes out
 * carries the URL verbatim — which is the claim story 13 is actually about.
 *
 * Prefixed `test-` like `test-setup.ts`: it is imported by suites, never by
 * the app, and nothing in a bundle reaches it.
 */

/** A cancelable `paste` carrying `text` as its `text/plain` flavour. */
export function pasteEvent(text: string): Event {
  const event = new Event("paste", { bubbles: true, cancelable: true });
  Object.defineProperty(event, "clipboardData", {
    value: { getData: () => text },
  });
  return event;
}

/**
 * Dispatch a paste of `text` into `field`, and answer whether anything
 * handled it.
 *
 * `false` is the story-13 answer: the platform's own paste stands.
 */
export function paste(field: HTMLElement, text: string): boolean {
  const event = pasteEvent(text);
  field.dispatchEvent(event);
  return event.defaultPrevented;
}
