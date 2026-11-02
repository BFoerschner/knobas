/**
 * The ticket page's inline images (#2406): the Jira wiki's `!name.png!`,
 * which the converter resolves against the ticket's attachment list to
 * `attachment:<id>`, drawn by the Markdown component from a `data:` URL.
 *
 * The bytes are the page's attachment previews (#2405,
 * `attachment-previews.svelte.ts`): an inline image is fetched once it is in
 * view (`watch`), and shares its answer with the Attachments panel's row for
 * the same file. One that cannot be fetched is the placeholder, saying why
 * and linking the ticket in its source.
 */
import type { AttachmentImages } from "../../render/markdown";
import type { Preview, Previews } from "./attachment-previews.svelte";

/** Why a file's bytes are missing, as an image's placeholder and the attachment preview (#2415) say it. */
export function missingWords(name: string, source: string, error: unknown): string {
  const preview = (typeof error === "object" && error !== null ? error : {}) as Partial<Preview>;
  if (preview.state === "too-large") return `${name} is over the 5 MB preview cap.`;
  if (preview.state === "failed" && preview.reason === "forbidden") return `${name}, not permitted in ${source}.`;
  return `${name}, not reachable in ${source} right now.`;
}

/**
 * The ticket's attachments as its prose's inline images: only an id it lists
 * is read. `open` previews a file (#2415), which an attachment link or image
 * does when clicked.
 */
export function ticketImages(previews: Previews, source: string, webUrl: string | null, open?: (id: string) => void): AttachmentImages {
  return {
    ...(open ? { open } : {}),
    watch: (node, id) => previews.watch(node, id),
    load: (id) => previews.when(id),
    missing: (name, error) => ({
      text: missingWords(name, source, error),
      link: webUrl === null ? null : { href: webUrl, label: `Open in ${source}` },
    }),
  };
}
