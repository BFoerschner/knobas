/** A name for a text's content: its blob where the source gave one, else its length and a hash of a sample of it. */
export function contentOf(text: string, sha: string | null): string {
  if (sha !== null && sha !== "") return sha;
  let h = 0x811c9dc5;
  const step = Math.max(1, Math.floor(text.length / 65536));
  for (let i = 0; i < text.length; i += step) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return `${text.length}:${h.toString(16)}`;
}
