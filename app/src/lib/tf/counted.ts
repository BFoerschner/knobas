/**
 * *1 board*, *4 boards*: a count with the noun in the number it earns. The
 * shell's one plural helper (#1097); prototype-6 draws "1 pages", this does
 * not (a recorded departure, as #871's "1 VM" was).
 */
export function counted(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

/**
 * A whole number with its thousands grouped by commas, as `toLocaleString("en-US")` draws one: `12,345` (the ux18 pass,
 * 2026-11-17). Without `Intl`: the first number a page formatted that way cost a repository's landing 19 ms at CPU 4x on
 * Björn's PC (the commit count), the locale's data loaded on the page's first frame.
 */
export function thousands(n: number): string {
  const whole = String(Math.trunc(n));
  return whole.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}
