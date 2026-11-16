/**
 * A filesystem path split where `TfPath` cuts it: everything up to and with the last separator, and the folder or file
 * after it. A path that ends in a separator keeps it on its end, so the folder it names is still the end.
 */
export function pathParts(path: string): { head: string; tail: string } {
  const trimmed = path.replace(/[\\/]+$/, "");
  const at = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  if (at < 0 || at === trimmed.length - 1) return { head: "", tail: path };
  return { head: path.slice(0, at + 1), tail: path.slice(at + 1) };
}
