/**
 * *1 board*, *4 boards*: a count with the noun in the number it earns. The
 * shell's one plural helper (#1097); prototype-6 draws "1 pages", this does
 * not (a recorded departure, as #871's "1 VM" was).
 */
export function counted(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}
