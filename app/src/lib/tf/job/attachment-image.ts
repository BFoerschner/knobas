/**
 * The ticket page's inline images (#2406): the Jira wiki's `!name.png!`,
 * which the converter resolves against the ticket's attachment list to
 * `attachment:<id>`, drawn by the Markdown component from a `data:` URL.
 *
 * The bytes are #2405's: its read fills this module's one seam,
 * {@link fillAttachmentImages}, with a function from the ticket and one of its
 * attachments to the image's `data:image/…` URL, or a refusal. Until it is
 * filled, every inline image is refused and drawn as the placeholder.
 */
import type { Attachment } from "../../ipc/entity";
import type { AttachmentImages } from "../../render/markdown";

/** An attachment's image as a `data:image/…` URL, or a refusal when it cannot be fetched. */
export type AttachmentImageRead = (entityId: string, attachment: Attachment) => Promise<string>;

const unfilled: AttachmentImageRead = () => Promise.reject(new Error("attachment images are not read yet"));
let read: AttachmentImageRead = unfilled;

/** Gives the page its attachment image read; returns the undo (tests, the dev fake). */
export function fillAttachmentImages(fill: AttachmentImageRead): () => void {
  read = fill;
  return () => {
    if (read === fill) read = unfilled;
  };
}

/** The ticket's attachments as its description's inline images: only an id it lists is read. */
export function ticketImages(entityId: string, files: readonly Attachment[], source: string, webUrl: string | null): AttachmentImages {
  return {
    load: (id) => {
      const file = files.find((candidate) => candidate.id === id);
      return file === undefined ? Promise.reject(new Error(`the ticket lists no attachment ${id}`)) : read(entityId, file);
    },
    missing: (name) => ({
      text: `${name}, not reachable in ${source} right now.`,
      link: webUrl === null ? null : { href: webUrl, label: `Open in ${source}` },
    }),
  };
}
