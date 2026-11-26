//! What a command prints, read into lines (ADR-0060, section 14).
//!
//! One [`Reader`] a run; stdout and stderr each keep their own colour and
//! their own unfinished line, so a half line of one and a line of the other
//! never share a row. What it knows: SGR colours and weights (256-colour and
//! RGB values become the nearest of the eight names by hue, since a theme
//! colours names and no value a program prints reaches the screen), a
//! carriage return (the line is written again from its start, so a progress
//! line is one line), a backspace and a tab. **Every other escape is
//! dropped**: no cursor movement, no title, no OSC 8 link, no bell. An escape
//! or a UTF-8 sequence cut by a chunk's end waits for the next chunk; invalid
//! UTF-8 is replaced a character at a time.
//!
//! Positions (`from`, `to`) count UTF-16 code units, the window's string
//! index, so the page slices a line's text with them as they come.

use serde::Serialize;

use super::kept::Kept;

/// Which pipe a line came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "app.ts", rename = "OneOffStream")]
pub enum Stream {
    Out,
    Err,
}

/// One of the eight colours a program can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts", rename = "OneOffColour")]
pub enum Colour {
    #[serde(rename = "k")]
    Black,
    #[serde(rename = "r")]
    Red,
    #[serde(rename = "g")]
    Green,
    #[serde(rename = "y")]
    Yellow,
    #[serde(rename = "b")]
    Blue,
    #[serde(rename = "m")]
    Magenta,
    #[serde(rename = "c")]
    Cyan,
    #[serde(rename = "w")]
    White,
}

const NAMED: [Colour; 8] = [
    Colour::Black,
    Colour::Red,
    Colour::Green,
    Colour::Yellow,
    Colour::Blue,
    Colour::Magenta,
    Colour::Cyan,
    Colour::White,
];

/// How a run of characters is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts", rename = "OneOffStyle")]
pub struct Style {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fg: Option<Colour>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub bg: Option<Colour>,
    #[serde(skip_serializing_if = "is_false")]
    #[ts(as = "Option<bool>", optional)]
    pub bold: bool,
    #[serde(skip_serializing_if = "is_false")]
    #[ts(as = "Option<bool>", optional)]
    pub dim: bool,
    #[serde(skip_serializing_if = "is_false")]
    #[ts(as = "Option<bool>", optional)]
    pub italic: bool,
    #[serde(skip_serializing_if = "is_false")]
    #[ts(as = "Option<bool>", optional)]
    pub underline: bool,
    /// SGR 7: drawn with its colours swapped, never sent.
    #[serde(skip)]
    #[ts(skip)]
    pub inverse: bool,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_false(b: &bool) -> bool {
    !*b
}

impl Style {
    fn plain(&self) -> bool {
        *self == Self::default()
    }

    /// The style as drawn: an inverse one with its colours swapped.
    fn drawn(self) -> Self {
        if self.inverse {
            Self {
                fg: self.bg,
                bg: Some(self.fg.unwrap_or(Colour::White)),
                inverse: false,
                ..self
            }
        } else {
            self
        }
    }
}

/// A run of one style in a line, `from` to `to` in UTF-16 units.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts", rename = "OneOffRun")]
pub struct Run {
    pub from: u32,
    pub to: u32,
    pub style: Style,
}

/// A file of the checkout a line names, with a line and maybe a column.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts", rename = "OneOffPlace")]
pub struct Place {
    pub from: u32,
    pub to: u32,
    /// From the repository's top, with `/`.
    pub path: String,
    pub line: Option<u32>,
    pub col: Option<u32>,
}

/// What a program said about a place: a linter's error, a compiler's warning.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts", rename = "OneOffFinding")]
pub struct Finding {
    /// `error`, `warning`, `note`, or empty where the program wrote none.
    pub severity: String,
    pub said: String,
    pub rule: String,
}

/// One line of a run's output.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts", rename = "OneOffLine")]
pub struct Line {
    /// In the order the lines began, from 0.
    pub n: u32,
    pub stream: Stream,
    pub text: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<Run>>", optional)]
    pub runs: Vec<Run>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<Place>>", optional)]
    pub places: Vec<Place>,
    /// Web addresses, opened in the browser.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<super::places::Url>>", optional)]
    pub urls: Vec<super::places::Url>,
    /// The place the line is about, with what was said there.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub finding: Option<Finding>,
    /// Bytes past the line's cap, not kept.
    #[serde(skip_serializing_if = "is_zero")]
    #[ts(as = "Option<u32>", optional)]
    pub cut: u32,
    /// Still being written: no newline yet.
    #[serde(skip_serializing_if = "is_false")]
    #[ts(as = "Option<bool>", optional)]
    pub open: bool,
    /// Its length in UTF-16 units, the window's measure.
    #[serde(skip)]
    pub len16: u32,
    /// A carriage return came: the next character starts the line again.
    #[serde(skip)]
    pub ret: bool,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_zero(n: &u32) -> bool {
    *n == 0
}

impl Line {
    fn new(n: u32, stream: Stream) -> Self {
        Self {
            n,
            stream,
            text: String::new(),
            runs: Vec::new(),
            places: Vec::new(),
            urls: Vec::new(),
            finding: None,
            cut: 0,
            open: true,
            len16: 0,
            ret: false,
        }
    }

    /// Bytes this line holds in memory, roughly.
    #[must_use]
    pub fn weight(&self) -> usize {
        self.text.len()
            + self.runs.len() * 16
            + self.places.iter().map(|p| p.path.len() + 24).sum::<usize>()
            + 48
    }
}

/// `line`'s text with its colour runs as SGR codes again: *Download with the
/// colour codes*. Only what the reader kept: the eight names, bold, dim,
/// italic and underline.
#[must_use]
pub fn with_codes(line: &Line) -> String {
    let units: Vec<u16> = line.text.encode_utf16().collect();
    let text = |a: u32, b: u32| {
        String::from_utf16_lossy(
            &units[(a as usize).min(units.len())..(b as usize).min(units.len())],
        )
    };
    let mut out = String::new();
    let mut at = 0;
    for run in &line.runs {
        out.push_str(&text(at, run.from));
        let mut codes: Vec<String> = Vec::new();
        let index = |c: Colour| NAMED.iter().position(|n| *n == c).unwrap_or(7);
        if let Some(fg) = run.style.fg {
            codes.push((30 + index(fg)).to_string());
        }
        if let Some(bg) = run.style.bg {
            codes.push((40 + index(bg)).to_string());
        }
        for (on, code) in [
            (run.style.bold, "1"),
            (run.style.dim, "2"),
            (run.style.italic, "3"),
            (run.style.underline, "4"),
        ] {
            if on {
                codes.push(code.to_owned());
            }
        }
        out.push_str(&format!("\u{1b}[{}m", codes.join(";")));
        out.push_str(&text(run.from, run.to));
        out.push_str("\u{1b}[0m");
        at = run.to;
    }
    out.push_str(&text(
        at,
        line.len16
            .max(u32::try_from(units.len()).unwrap_or(u32::MAX)),
    ));
    out
}

/// The most characters a line keeps (64 KiB); the rest is counted in `cut`.
pub const LINE_CAP: usize = 64 * 1024;

#[derive(Default)]
struct StreamState {
    style: Style,
    /// Bytes of an escape or a UTF-8 sequence a chunk cut.
    pending: Vec<u8>,
    /// The number of this stream's unfinished line.
    open: Option<u32>,
}

/// One run's reader: chunks in, lines into a [`Kept`].
pub struct Reader {
    streams: [StreamState; 2],
    line_cap: usize,
}

impl Default for Reader {
    fn default() -> Self {
        Self::new(LINE_CAP)
    }
}

/// What a write did: the lines it finished, for the place finder.
#[derive(Debug, Default)]
pub struct Wrote {
    pub ended: Vec<u32>,
}

impl Reader {
    #[must_use]
    pub fn new(line_cap: usize) -> Self {
        Self {
            streams: [StreamState::default(), StreamState::default()],
            line_cap,
        }
    }

    const fn index(stream: Stream) -> usize {
        match stream {
            Stream::Out => 0,
            Stream::Err => 1,
        }
    }

    /// The stream's open line, made where there is none or it was dropped.
    fn line<'k>(&mut self, kept: &'k mut Kept, stream: Stream) -> &'k mut Line {
        let state = &mut self.streams[Self::index(stream)];
        let n = match state.open {
            Some(n) if kept.get(n).is_some() => n,
            _ => {
                let n = kept.push(Line::new(kept.next(), stream));
                state.open = Some(n);
                n
            }
        };
        kept.touch(n);
        kept.get_mut(n)
            .expect("the open line was just made or found")
    }

    fn end_line(&mut self, kept: &mut Kept, stream: Stream, wrote: &mut Wrote) {
        // A newline with no line open is an empty line of its own.
        let line = self.line(kept, stream);
        line.open = false;
        line.ret = false;
        let n = line.n;
        kept.ended(n);
        wrote.ended.push(n);
        self.streams[Self::index(stream)].open = None;
    }

    fn put(&mut self, kept: &mut Kept, stream: Stream, text: &str) {
        let style = self.streams[Self::index(stream)].style.drawn();
        let cap = self.line_cap;
        let line = self.line(kept, stream);
        let before = line.weight();
        if line.ret {
            line.text.clear();
            line.runs.clear();
            line.cut = 0;
            line.len16 = 0;
            line.ret = false;
        }
        let room = cap.saturating_sub(line.text.chars().count().min(cap));
        let (part, rest) = match text.char_indices().nth(room) {
            Some((at, _)) => text.split_at(at),
            None => (text, ""),
        };
        line.cut = line
            .cut
            .saturating_add(u32::try_from(rest.len()).unwrap_or(u32::MAX));
        if !part.is_empty() {
            let from = line.len16;
            let width = u32::try_from(part.encode_utf16().count()).unwrap_or(u32::MAX);
            line.text.push_str(part);
            line.len16 = from + width;
            if !style.plain() {
                match line.runs.last_mut() {
                    Some(last) if last.style == style && last.to == from => last.to += width,
                    _ => line.runs.push(Run {
                        from,
                        to: from + width,
                        style,
                    }),
                }
            }
        }
        let after = line.weight();
        kept.reweigh(before, after);
    }

    fn backspace(&mut self, kept: &mut Kept, stream: Stream) {
        let line = self.line(kept, stream);
        if let Some(ch) = line.text.pop() {
            let width = u32::try_from(ch.len_utf16()).unwrap_or(1);
            line.len16 = line.len16.saturating_sub(width);
            let end = line.len16;
            line.runs.retain_mut(|run| {
                run.to = run.to.min(end);
                run.from < run.to
            });
        }
    }

    /// Reads `bytes` of `stream` into `kept`.
    pub fn write(&mut self, kept: &mut Kept, stream: Stream, bytes: &[u8]) -> Wrote {
        let mut wrote = Wrote::default();
        let state = &mut self.streams[Self::index(stream)];
        let mut data = std::mem::take(&mut state.pending);
        data.extend_from_slice(bytes);
        let mut i = 0;
        let mut text = String::new();
        let flush = |this: &mut Self, kept: &mut Kept, text: &mut String| {
            if !text.is_empty() {
                this.put(kept, stream, text);
                text.clear();
            }
        };
        while i < data.len() {
            let b = data[i];
            match b {
                b'\n' => {
                    flush(self, kept, &mut text);
                    self.end_line(kept, stream, &mut wrote);
                    i += 1;
                }
                b'\r' => {
                    flush(self, kept, &mut text);
                    match data.get(i + 1) {
                        Some(b'\n') => {
                            i += 1;
                        }
                        None => {
                            self.streams[Self::index(stream)].pending = vec![b'\r'];
                            return wrote;
                        }
                        Some(_) => {
                            self.line(kept, stream).ret = true;
                            i += 1;
                        }
                    }
                }
                0x08 => {
                    flush(self, kept, &mut text);
                    self.backspace(kept, stream);
                    i += 1;
                }
                0x1b => {
                    flush(self, kept, &mut text);
                    let Some(end) = escape_end(&data, i) else {
                        self.streams[Self::index(stream)].pending = data[i..].to_vec();
                        return wrote;
                    };
                    if data.get(i + 1) == Some(&b'[') && data[end - 1] == b'm' {
                        let params = String::from_utf8_lossy(&data[i + 2..end - 1]).into_owned();
                        sgr(&mut self.streams[Self::index(stream)].style, &params);
                    }
                    i = end;
                }
                b'\t' => {
                    text.push('\t');
                    i += 1;
                }
                0..=0x1f | 0x7f => {
                    i += 1;
                }
                _ => {
                    // A run of text up to the next control byte, decoded.
                    let mut j = i;
                    while j < data.len() && (data[j] >= 0x20 && data[j] != 0x7f) {
                        j += 1;
                    }
                    let chunk = &data[i..j];
                    let (decoded, used) = decode(chunk, j == data.len());
                    text.push_str(&decoded);
                    if used < chunk.len() {
                        // A sequence the chunk cut: kept for the next one.
                        flush(self, kept, &mut text);
                        self.streams[Self::index(stream)].pending = data[i + used..].to_vec();
                        return wrote;
                    }
                    i = j;
                }
            }
        }
        flush(self, kept, &mut text);
        wrote
    }

    /// The run ended: every open line ends, and a pending `\r` or cut
    /// sequence is read as it is.
    pub fn finish(&mut self, kept: &mut Kept) -> Wrote {
        let mut wrote = Wrote::default();
        for stream in [Stream::Out, Stream::Err] {
            let pending = std::mem::take(&mut self.streams[Self::index(stream)].pending);
            if !pending.is_empty() && pending != b"\r" && pending[0] != 0x1b {
                let text = String::from_utf8_lossy(&pending).into_owned();
                self.put(kept, stream, &text);
            }
            if self.streams[Self::index(stream)].open.is_some() {
                let n = self.streams[Self::index(stream)].open;
                if n.is_some_and(|n| kept.get(n).is_some()) {
                    self.end_line(kept, stream, &mut wrote);
                }
                self.streams[Self::index(stream)].open = None;
            }
        }
        wrote
    }
}

/// `chunk` as text: valid UTF-8 as it is, each invalid byte a U+FFFD. At the
/// end of the data, an incomplete sequence is left (answering how much was
/// used) for the next chunk.
fn decode(chunk: &[u8], at_end: bool) -> (String, usize) {
    let mut out = String::with_capacity(chunk.len());
    let mut rest = chunk;
    let mut used = 0;
    loop {
        match std::str::from_utf8(rest) {
            Ok(text) => {
                out.push_str(text);
                used += rest.len();
                return (out, used);
            }
            Err(error) => {
                let good = error.valid_up_to();
                out.push_str(std::str::from_utf8(&rest[..good]).unwrap_or_default());
                used += good;
                match error.error_len() {
                    Some(bad) => {
                        out.push('\u{fffd}');
                        used += bad;
                        rest = &rest[good + bad..];
                    }
                    None if at_end => return (out, used),
                    None => {
                        out.push('\u{fffd}');
                        return (out, chunk.len());
                    }
                }
            }
        }
    }
}

/// Where the escape at `i` ends (exclusive), or `None` while it has not all
/// arrived. A CSI longer than 64 bytes or a string longer than 4,096 is cut
/// at its first two bytes, so a stray escape never swallows the output.
fn escape_end(data: &[u8], i: usize) -> Option<usize> {
    let kind = *data.get(i + 1)?;
    match kind {
        b'[' => {
            // Parameters and intermediates, then a final byte; a byte that
            // belongs to none ends the escape there, and a run of more than
            // 64 is a stray escape cut at its first two bytes.
            for (j, &c) in data.iter().enumerate().skip(i + 2) {
                if j - i > 64 {
                    return Some(i + 2);
                }
                match c {
                    0x40..=0x7e => return Some(j + 1),
                    0x20..=0x3f => {}
                    _ => return Some(j),
                }
            }
            (data.len() - i > 64).then_some(i + 2)
        }
        b']' | b'P' | b'_' | b'^' => {
            let mut j = i + 2;
            while j < data.len() {
                if data[j] == 0x07 {
                    return Some(j + 1);
                }
                if data[j] == 0x1b && data.get(j + 1) == Some(&b'\\') {
                    return Some(j + 2);
                }
                j += 1;
            }
            (data.len() - i > 4096).then_some(i + 2)
        }
        // `ESC ( B` and the like: intermediates, then one final byte.
        0x20..=0x2f => {
            for (j, &c) in data.iter().enumerate().skip(i + 2) {
                if !(0x20..=0x2f).contains(&c) {
                    return Some(j + 1);
                }
            }
            None
        }
        _ => Some(i + 2),
    }
}

/// An RGB value as the nearest named colour, by hue.
fn nearest(r: u8, g: u8, b: u8) -> Colour {
    let (r, g, b) = (f32::from(r), f32::from(g), f32::from(b));
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    if max - min < 28.0 {
        return if max < 96.0 {
            Colour::Black
        } else {
            Colour::White
        };
    }
    let d = max - min;
    #[allow(clippy::float_cmp)]
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    let h = (h * 60.0 + 360.0).rem_euclid(360.0);
    if !(20.0..335.0).contains(&h) {
        Colour::Red
    } else if h < 70.0 {
        Colour::Yellow
    } else if h < 160.0 {
        Colour::Green
    } else if h < 200.0 {
        Colour::Cyan
    } else if h < 265.0 {
        Colour::Blue
    } else {
        Colour::Magenta
    }
}

/// A 256-colour index as a named colour.
fn from_256(n: u32) -> Colour {
    const CUBE: [u8; 6] = [0, 95, 135, 175, 215, 255];
    match n {
        0..=7 => NAMED[n as usize],
        8 => Colour::Black,
        9..=15 => NAMED[n as usize - 8],
        16..=231 => {
            let c = (n - 16) as usize;
            nearest(CUBE[c / 36], CUBE[(c / 6) % 6], CUBE[c % 6])
        }
        232..=243 => Colour::Black,
        _ => Colour::White,
    }
}

fn byte(n: Option<&u32>) -> u8 {
    u8::try_from(n.copied().unwrap_or(0).min(255)).unwrap_or(255)
}

/// An SGR's parameters applied to `style`.
fn sgr(style: &mut Style, params: &str) {
    let p: Vec<u32> = if params.is_empty() {
        vec![0]
    } else {
        params
            .split([';', ':'])
            .map(|x| x.parse().unwrap_or(0))
            .collect()
    };
    let mut k = 0;
    while k < p.len() {
        match p[k] {
            0 => *style = Style::default(),
            1 => style.bold = true,
            2 => style.dim = true,
            3 => style.italic = true,
            4 => style.underline = true,
            7 => style.inverse = true,
            27 => style.inverse = false,
            22 => {
                style.bold = false;
                style.dim = false;
            }
            23 => style.italic = false,
            24 => style.underline = false,
            n @ 30..=37 => style.fg = Some(NAMED[(n - 30) as usize]),
            39 => style.fg = None,
            n @ 40..=47 => style.bg = Some(NAMED[(n - 40) as usize]),
            49 => style.bg = None,
            n @ 90..=97 => style.fg = Some(NAMED[(n - 90) as usize]),
            n @ 100..=107 => style.bg = Some(NAMED[(n - 100) as usize]),
            n @ (38 | 48) => {
                let colour = match p.get(k + 1) {
                    Some(5) => {
                        let c = from_256(p.get(k + 2).copied().unwrap_or(0));
                        k += 2;
                        Some(c)
                    }
                    Some(2) => {
                        let c = nearest(byte(p.get(k + 2)), byte(p.get(k + 3)), byte(p.get(k + 4)));
                        k += 4;
                        Some(c)
                    }
                    _ => None,
                };
                if let Some(c) = colour {
                    if n == 38 {
                        style.fg = Some(c);
                    } else {
                        style.bg = Some(c);
                    }
                }
            }
            _ => {}
        }
        k += 1;
    }
}

#[cfg(test)]
mod tests;
