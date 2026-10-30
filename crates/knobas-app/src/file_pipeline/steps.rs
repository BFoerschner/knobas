//! The built-in steps' work: plain functions over bytes, run on a pipeline
//! worker thread (`super::on_worker`). Every one writes through [`Capped`], so
//! the size cap is checked as the output grows, never after it has grown, and
//! the worker's stop flag is seen at every write.

use std::io::{self, Read as _, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use base64::Engine as _;
use base64::engine::DecodePaddingMode;
use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig, STANDARD};

use super::{PipeFormat, PipeStep};

/// Why a step gave no output.
#[derive(Debug)]
pub(super) enum Failure {
    /// The step's own error, as its reader would say it.
    Error(String),
    /// Its output passed the cap.
    TooLarge,
    /// Its worker was told to stop (the step's deadline passed).
    Stopped,
    /// No worker was free before the step's deadline.
    Busy,
}

/// Raised from [`Capped::write`] to stop a decoder mid-stream.
#[derive(Debug)]
struct OverCap;
impl std::fmt::Display for OverCap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("over the size cap")
    }
}
impl std::error::Error for OverCap {}

#[derive(Debug)]
struct Halted;
impl std::fmt::Display for Halted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("stopped")
    }
}
impl std::error::Error for Halted {}

/// An output buffer that refuses to grow past `cap` and stops when told to.
pub(super) struct Capped<'a> {
    pub(super) bytes: Vec<u8>,
    cap: u64,
    stop: &'a AtomicBool,
}

impl<'a> Capped<'a> {
    pub(super) fn new(cap: u64, stop: &'a AtomicBool) -> Self {
        Self {
            bytes: Vec::new(),
            cap,
            stop,
        }
    }
}

impl Write for Capped<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.stop.load(Ordering::Relaxed) {
            return Err(io::Error::other(Halted));
        }
        if self.bytes.len() as u64 + buf.len() as u64 > self.cap {
            return Err(io::Error::other(OverCap));
        }
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// An I/O error as a step's failure: the cap and the stop flag are ours,
/// anything else is the step's reader speaking.
pub(super) fn io_failure(error: &io::Error) -> Failure {
    match error.get_ref() {
        Some(inner) if inner.is::<OverCap>() => Failure::TooLarge,
        Some(inner) if inner.is::<Halted>() => Failure::Stopped,
        _ => Failure::Error(error.to_string()),
    }
}

/// `step`'s output for `input`, at most `cap` bytes.
pub(super) fn run(
    step: &PipeStep,
    input: &[u8],
    cap: u64,
    stop: &AtomicBool,
) -> Result<Vec<u8>, Failure> {
    let mut out = Capped::new(cap, stop);
    match step {
        PipeStep::Base64Decode => base64_decode(input, &mut out)?,
        PipeStep::Base64Encode => {
            let text = STANDARD.encode(input);
            out.write_all(text.as_bytes()).map_err(|e| io_failure(&e))?;
        }
        PipeStep::Gunzip => copy(flate2::read::MultiGzDecoder::new(input), &mut out)?,
        PipeStep::Unzstd => unzstd(input, &mut out)?,
        PipeStep::Bunzip2 => copy(bzip2::read::MultiBzDecoder::new(input), &mut out)?,
        PipeStep::Unxz => {
            let mut reader = input;
            lzma_rs::xz_decompress(&mut reader, &mut out).map_err(|error| match error {
                lzma_rs::error::Error::IoError(error) => io_failure(&error),
                other => Failure::Error(other.to_string()),
            })?;
        }
        PipeStep::Jq { filter } => super::jq::run(filter, input, &mut out)?,
        PipeStep::Pretty { format } => {
            let text = pretty(*format, input)?;
            out.write_all(text.as_bytes()).map_err(|e| io_failure(&e))?;
        }
        PipeStep::Convert { from, to } => {
            let text = convert(*from, *to, input)?;
            out.write_all(text.as_bytes()).map_err(|e| io_failure(&e))?;
        }
        PipeStep::HexDump => hex_dump(input, &mut out)?,
    }
    Ok(out.bytes)
}

fn copy(mut reader: impl io::Read, out: &mut Capped<'_>) -> Result<(), Failure> {
    io::copy(&mut reader, out).map_err(|e| io_failure(&e))?;
    Ok(())
}

/// Every frame in `input`, one after another, as `zstd -d` reads them.
fn unzstd(mut input: &[u8], out: &mut Capped<'_>) -> Result<(), Failure> {
    if input.is_empty() {
        return Err(Failure::Error("there are no zstd frames in it".to_owned()));
    }
    while !input.is_empty() {
        let mut frame = ruzstd::decoding::StreamingDecoder::new(&mut input)
            .map_err(|error| Failure::Error(format!("not zstd data: {error}")))?;
        let mut chunk = [0u8; 64 * 1024];
        loop {
            let read = frame.read(&mut chunk).map_err(|e| io_failure(&e))?;
            if read == 0 {
                break;
            }
            out.write_all(&chunk[..read]).map_err(|e| io_failure(&e))?;
        }
    }
    Ok(())
}

/// Base64 in either alphabet, padded or not, with line breaks and spaces
/// anywhere (a MIME body, a YAML block scalar).
fn base64_decode(input: &[u8], out: &mut Capped<'_>) -> Result<(), Failure> {
    let compact: Vec<u8> = input
        .iter()
        .copied()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    let config =
        GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent);
    let url_safe = compact.iter().any(|b| matches!(b, b'-' | b'_'));
    let alphabet = if url_safe {
        &base64::alphabet::URL_SAFE
    } else {
        &base64::alphabet::STANDARD
    };
    // Its length is known before decoding: refuse it whole rather than grow.
    if (compact.len() as u64 / 4) * 3 > out.cap {
        return Err(Failure::TooLarge);
    }
    let decoded = GeneralPurpose::new(alphabet, config)
        .decode(&compact)
        .map_err(|error| Failure::Error(format!("not base64: {error}")))?;
    out.write_all(&decoded).map_err(|e| io_failure(&e))
}

fn text(input: &[u8]) -> Result<&str, Failure> {
    std::str::from_utf8(input)
        .map_err(|error| Failure::Error(format!("the input is not UTF-8 text: {error}")))
}

/// `input` in `format`, laid out again. JSON goes through jaq's reader and
/// writer, so key order and every number's digits stay as written.
fn pretty(format: PipeFormat, input: &[u8]) -> Result<String, Failure> {
    match format {
        PipeFormat::Json => super::jq::pretty(input),
        PipeFormat::Yaml => {
            let value: serde_norway::Value = serde_norway::from_str(text(input)?)
                .map_err(|error| Failure::Error(format!("not YAML: {error}")))?;
            serde_norway::to_string(&value).map_err(|error| Failure::Error(error.to_string()))
        }
        PipeFormat::Toml => {
            let value: toml::Table = toml::from_str(text(input)?)
                .map_err(|error| Failure::Error(format!("not TOML: {error}")))?;
            toml::to_string_pretty(&value).map_err(|error| Failure::Error(error.to_string()))
        }
    }
}

/// `input`, read as `from` and written as `to`. The value between them is a
/// YAML one, whose maps keep their order; a TOML date-time crosses as its
/// text.
fn convert(from: PipeFormat, to: PipeFormat, input: &[u8]) -> Result<String, Failure> {
    let input = text(input)?;
    let value: serde_norway::Value = match from {
        PipeFormat::Json => serde_json::from_str(input)
            .map_err(|error| Failure::Error(format!("not JSON: {error}")))?,
        PipeFormat::Yaml => serde_norway::from_str(input)
            .map_err(|error| Failure::Error(format!("not YAML: {error}")))?,
        PipeFormat::Toml => {
            let table: toml::Table = toml::from_str(input)
                .map_err(|error| Failure::Error(format!("not TOML: {error}")))?;
            from_toml(toml::Value::Table(table))
        }
    };
    let written = match to {
        PipeFormat::Json => serde_json::to_string_pretty(&value).map_err(|e| e.to_string()),
        PipeFormat::Yaml => serde_norway::to_string(&value).map_err(|e| e.to_string()),
        PipeFormat::Toml => {
            toml::to_string_pretty(&value).map_err(|e| format!("TOML cannot hold it: {e}"))
        }
    };
    written
        .map(|mut text| {
            if !text.ends_with('\n') {
                text.push('\n');
            }
            text
        })
        .map_err(Failure::Error)
}

fn from_toml(value: toml::Value) -> serde_norway::Value {
    use serde_norway::Value as Y;
    match value {
        toml::Value::String(s) => Y::String(s),
        toml::Value::Integer(n) => Y::Number(n.into()),
        toml::Value::Float(n) => Y::Number(n.into()),
        toml::Value::Boolean(b) => Y::Bool(b),
        toml::Value::Datetime(at) => Y::String(at.to_string()),
        toml::Value::Array(items) => Y::Sequence(items.into_iter().map(from_toml).collect()),
        toml::Value::Table(table) => Y::Mapping(
            table
                .into_iter()
                .map(|(key, value)| (Y::String(key), from_toml(value)))
                .collect(),
        ),
    }
}

/// Sixteen bytes a line: the offset, the bytes in two groups of eight, and
/// the printable ones as text (`xxd`'s layout, `hexdump -C`'s groups).
fn hex_dump(input: &[u8], out: &mut Capped<'_>) -> Result<(), Failure> {
    use std::fmt::Write as _;
    let mut line = String::with_capacity(80);
    for (row, chunk) in input.chunks(16).enumerate() {
        line.clear();
        let _ = write!(line, "{:08x} ", row * 16);
        for (i, byte) in chunk.iter().enumerate() {
            if i == 8 {
                line.push(' ');
            }
            let _ = write!(line, " {byte:02x}");
        }
        for i in chunk.len()..16 {
            if i == 8 {
                line.push(' ');
            }
            line.push_str("   ");
        }
        line.push_str("  |");
        line.extend(chunk.iter().map(|&b| {
            if b.is_ascii_graphic() || b == b' ' {
                char::from(b)
            } else {
                '.'
            }
        }));
        line.push_str("|\n");
        out.write_all(line.as_bytes()).map_err(|e| io_failure(&e))?;
    }
    Ok(())
}
