/** A sentence with `code` in backticks, as parts to draw: a piece of words, then a piece of code, and so on. */
export function coded(text: string): { text: string; code: boolean }[] {
  return text.split("`").map((piece, i) => ({ text: piece, code: i % 2 === 1 })).filter((p) => p.text !== "");
}
