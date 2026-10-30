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
 * over as a bitmap) are two equal prints checked again by drawing both
 * pages and comparing pixels. Comparing text alone would miss a changed
 * image; drawing every page would be slow on a long document.
 *
 * Pages are aligned by the longest common run of equal prints, so a page
 * inserted in the middle is one added page and the pages after it still
 * pair with their old selves. Pages left between two matched pages pair up
 * in order as changed; the ones over are added or removed.
 */
import type { PdfDocument, PdfPrint } from "./port";

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

/** Each page's print, in page order. */
async function printsOf(pdf: PdfDocument): Promise<PdfPrint[]> {
  const prints: PdfPrint[] = [];
  for (let n = 1; n <= pdf.pages; n++) prints.push(await (await pdf.page(n)).print());
  return prints;
}

const samePixels = (a: Uint8ClampedArray, b: Uint8ClampedArray) => a.length === b.length && a.every((value, i) => value === b[i]);

/** The scale the pixel check draws at: a page about 400 px wide, enough to see a changed image. */
export const CHECK_SCALE = 0.5;

/** `before` and `after` compared page by page (see the module's comment for how). Either may be `null`: an added or deleted PDF. */
export async function comparePdfs(before: PdfDocument | null, after: PdfDocument | null): Promise<PageRow[]> {
  const [a, b] = await Promise.all([before ? printsOf(before) : [], after ? printsOf(after) : []]);
  const rows = alignPages(
    a.map((print) => print.key),
    b.map((print) => print.key),
  );
  for (const row of rows) {
    if (row.mark !== "same" || !(a[row.old! - 1]!.unsure || b[row.new! - 1]!.unsure)) continue;
    const [left, right] = await Promise.all([before!.page(row.old!), after!.page(row.new!)]);
    const [x, y] = await Promise.all([left.pixels(CHECK_SCALE), right.pixels(CHECK_SCALE)]);
    if (x !== null && y !== null && !samePixels(x, y)) row.mark = "changed";
  }
  return rows;
}
