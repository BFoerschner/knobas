/**
 * Bytes as base64 (review of spec #2234, Standards 2): the one encoder the
 * frame (an SVG's data URL), a command renderer's bytes input and anything
 * else that hands bytes on as text share. It works in chunks, so a long file
 * does not overflow `String.fromCharCode`'s argument list.
 */
export function base64Of(bytes: Uint8Array): string {
  let binary = "";
  for (let at = 0; at < bytes.length; at += 0x8000) binary += String.fromCharCode(...bytes.subarray(at, at + 0x8000));
  return btoa(binary);
}
