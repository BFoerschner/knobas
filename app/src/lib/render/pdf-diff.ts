/**
 * Two PDFs compared page by page (#2282, spec #2234 step 6e): which pages
 * are the same, which changed, which were added or removed.
 *
 * How a page is told from another: its print (`PdfPage.print`, in
 * `pdf-core.ts`), a digest of its size and its operator list, made
 * comparable across documents: the text it shows and where, its paths and
 * colours, and each image's size and pixels. That reads every page without
 * drawing one, and it does not depend on the fonts the machine has. Only
 * where a print could not read something it paints (an image pdf.js hands
 * over as a bitmap) is the page drawn too, and the drawing's digest joins
 * its key (`keysOf`). Comparing text alone would miss a changed image;
 * drawing every page would be slow on a long document.
 *
 * Pages are aligned by the longest common run of equal keys, so a page
 * inserted in the middle is one added page and the pages after it still
 * pair with their old selves. Pages left between two matched pages pair up
 * in order as changed; the ones over are added or removed.
 */
import type { PdfDocument } from "./port";

export type PageMark = "same" | "changed" | "added" | "removed";

/** One row of the diff: a page before and its page after (1-based), either absent for an added or removed page. */
export interface PageRow {
  old: number | null;
  new: number | null;
  mark: PageMark;
}

/** The pages of `before` and `after` (keys, one per page) aligned: equal keys matched by their longest common subsequence. */
export function alignPages(before: readonly string[], after: readonly string[]): PageRow[] {
  const [n, m] = [before.length, after.length];
  // run[i][j]: the longest common subsequence of before[i..] and after[j..].
  const run = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1));
  for (let i = n - 1; i >= 0; i--)
    for (let j = m - 1; j >= 0; j--)
      run[i]![j] = before[i] === after[j] ? run[i + 1]![j + 1]! + 1 : Math.max(run[i + 1]![j]!, run[i]![j + 1]!);
  const rows: PageRow[] = [];
  const gap = (olds: number[], news: number[]) => {
    const paired = Math.min(olds.length, news.length);
    for (let k = 0; k < paired; k++) rows.push({ old: olds[k]!, new: news[k]!, mark: "changed" });
    for (const old of olds.slice(paired)) rows.push({ old, new: null, mark: "removed" });
    for (const nu of news.slice(paired)) rows.push({ old: null, new: nu, mark: "added" });
  };
  let [i, j] = [0, 0];
  let olds: number[] = [];
  let news: number[] = [];
  while (i < n || j < m) {
    if (i < n && j < m && before[i] === after[j]) {
      gap(olds, news);
      [olds, news] = [[], []];
      rows.push({ old: i + 1, new: j + 1, mark: "same" });
      i++;
      j++;
    } else if (j >= m || (i < n && run[i + 1]![j]! >= run[i]![j + 1]!)) {
      olds.push(++i);
    } else {
      news.push(++j);
    }
  }
  gap(olds, news);
  return rows;
}

/** The scale an unsure page is drawn at for its key: a page about 400 px wide, enough to see a changed image. */
const CHECK_SCALE = 0.5;

/** A 53-bit hash of `bytes` (cyrb53 over the bytes themselves), as hex. */
function pixelDigest(bytes: Uint8ClampedArray): string {
  let h1 = 0xdeadbeef;
  let h2 = 0x41c6ce57;
  for (let i = 0; i < bytes.length; i++) {
    h1 = Math.imul(h1 ^ bytes[i]!, 2654435761);
    h2 = Math.imul(h2 ^ bytes[i]!, 1597334677);
  }
  h1 = Math.imul(h1 ^ (h1 >>> 16), 2246822507) ^ Math.imul(h2 ^ (h2 >>> 13), 3266489909);
  h2 = Math.imul(h2 ^ (h2 >>> 16), 2246822507) ^ Math.imul(h1 ^ (h1 >>> 13), 3266489909);
  return (4294967296 * (2097151 & h2) + (h1 >>> 0)).toString(16);
}

/**
 * Each page's key, in page order: its print, and where the print is unsure
 * (an image pdf.js handed over as a bitmap) a digest of the page drawn as
 * well, before the pages are aligned. Otherwise every page of a scan would
 * print alike (`img:WxH`), and one page inserted into a scan would pair
 * every page after it with its neighbour.
 *
 * The drawing's digest is exact, no tolerance. Both sides are drawn by the
 * same pdf.js in the same browser at the same scale, on a canvas read back
 * in software (`willReadFrequently`): anti-aliasing and font hinting come
 * out the same both times, so two pages that hold the same draw the same
 * bytes, and any byte that differs is a difference in what the page holds.
 * A tolerance would let a small changed image through. Where nothing draws
 * (jsdom) the print alone is the key.
 */
async function keysOf(pdf: PdfDocument): Promise<string[]> {
  const keys: string[] = [];
  for (let n = 1; n <= pdf.pages; n++) {
    const page = await pdf.page(n);
    const print = await page.print();
    const drawn = print.unsure ? await page.pixels(CHECK_SCALE) : null;
    keys.push(drawn === null ? print.key : `${print.key}:${pixelDigest(drawn)}`);
  }
  return keys;
}

/** `before` and `after` compared page by page (see the module's comment for how). Either may be `null`: an added or deleted PDF. */
export async function comparePdfs(before: PdfDocument | null, after: PdfDocument | null): Promise<PageRow[]> {
  const [a, b] = await Promise.all([before ? keysOf(before) : [], after ? keysOf(after) : []]);
  return alignPages(a, b);
}
