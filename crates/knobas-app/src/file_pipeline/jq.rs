//! jq through `jaq` (#2264): the filter is parsed, compiled and run in
//! process, over every JSON value in the input in turn, as `jq` reads a
//! stream. Each output is written as `jq` writes it by default: two-space
//! indent, one value a line group.
//!
//! # Depth (#2314)
//!
//! A filter is typed by a person and may be shared, so it must never take
//! the app down. jaq evaluates by recursion on the worker's stack and has no
//! depth limit of its own, and a Rust stack overflow aborts the process. So
//! the filter runs with [`Watched`] data: jaq asks it for the lookup table at
//! every term it evaluates, and at that point it checks how much of the
//! worker's stack the evaluation has taken ([`EVAL_STACK`] at most). Past
//! it, it unwinds the evaluation ([`Refused`]), which [`run`] catches and
//! answers as the step's failure. JSON is parsed by
//! recursion too: the input and every `fromjson` string are refused past
//! [`NESTING`] levels, jq's own parse limit.
//!
//! A value nested deep at run time without recursion
//! (`reduce range(1e6) as $_ (0; [.])`) is still written, compared and
//! dropped by recursion below jaq, and can overflow the stack. That is why
//! the jq step runs in a child process (`super::child`, #2314): the
//! overflow ends the child, not the app.

use std::io::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use jaq_core::box_iter::box_once;
use jaq_core::load::{Arena, File, Loader};
use jaq_core::native::{Fun, v};
use jaq_core::{
    Compiler, Ctx, Cv, DataT, Exn, Lut, RunPtr, ValXs, Vars, data::HasLut, unwrap_valr,
};
use jaq_json::Val;
use jaq_json::write::{Pp, write};

use super::steps::{Capped, Failure};

/// How much of the worker's stack a filter's evaluation may take. The rest
/// of the worker's stack (`super::WORKER_STACK`) is left for what runs
/// below it without coming back to jaq: writing, comparing or dropping a
/// deeply nested value.
pub(super) const EVAL_STACK: usize = 64 * 1024 * 1024;
/// The deepest JSON that is parsed, as `jq`'s parser allows.
pub(super) const NESTING: usize = 10_000;

/// Why an evaluation was unwound.
enum Refused {
    /// It took [`EVAL_STACK`].
    TooDeep,
    /// `fromjson` was given JSON past [`NESTING`].
    Nested,
}

/// A filter's data kind: JSON values, and the [`Watched`] lookup table.
struct Guarded;

impl DataT for Guarded {
    type V<'a> = Val;
    type Data<'a> = Watched<'a>;
}

/// The lookup table, handed out only while the evaluation is within its
/// stack.
#[derive(Clone)]
struct Watched<'a> {
    lut: &'a Lut<Guarded>,
    /// The lowest stack address the evaluation may reach (the stack grows
    /// down on every target knobas builds for).
    floor: usize,
}

impl<'a> HasLut<'a, Guarded> for Watched<'a> {
    fn lut(&self) -> &'a Lut<Guarded> {
        if stack_here() < self.floor {
            // `resume_unwind`, not `panic!`: no panic hook runs, so a
            // refused filter is not logged as a crash.
            resume_unwind(Box::new(Refused::TooDeep));
        }
        self.lut
    }
}

#[inline(never)]
fn stack_here() -> usize {
    let probe = 0u8;
    std::hint::black_box(&raw const probe) as usize
}

/// jaq's functions, with `fromjson` swapped for one that refuses deep JSON.
fn funs() -> impl Iterator<Item = Fun<Guarded>> {
    jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs().filter(|(name, _, _)| *name != "fromjson"))
        .chain([jaq_core::native::run::<Guarded>((
            "fromjson",
            v(0),
            fromjson as RunPtr<Guarded>,
        ))])
}

fn fromjson<'a>(cv: Cv<'a, Guarded>) -> ValXs<'a, <Guarded as DataT>::V<'a>> {
    let text = match cv.1.try_as_utf8_bytes_owned() {
        Ok(text) => text,
        Err(error) => return box_once(Err(Exn::from(error))),
    };
    if nesting(&text) > NESTING {
        resume_unwind(Box::new(Refused::Nested));
    }
    let values: Vec<_> = jaq_json::read::parse_many(&text)
        .map(|value| {
            value.map_err(|error| {
                Exn::from(jaq_json::Error::str(format_args!(
                    "cannot parse {} as JSON: {error}",
                    cv.1
                )))
            })
        })
        .collect();
    Box::new(values.into_iter())
}

/// How deep the arrays and objects in `json` nest, strings skipped.
pub(super) fn nesting(json: &[u8]) -> usize {
    let (mut depth, mut deepest, mut in_string, mut escaped) = (0usize, 0, false, false);
    for &byte in json {
        if in_string {
            match byte {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'[' | b'{' => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}

fn too_nested() -> Failure {
    Failure::Error(format!("the input nests deeper than {NESTING} levels"))
}

fn layout() -> Pp {
    Pp {
        indent: Some("  ".to_owned()),
        sep_space: true,
        ..Pp::default()
    }
}

/// `filter` parsed and compiled with knobas's functions.
fn compile(filter: &str) -> Result<jaq_core::Filter<Guarded>, Failure> {
    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs());
    let arena = Arena::default();
    let modules = Loader::new(defs)
        .load(
            &arena,
            File {
                code: filter,
                path: (),
            },
        )
        .map_err(|errors| Failure::Error(parse_errors(filter, errors)))?;
    Compiler::default()
        .with_funs(funs())
        .compile(modules)
        .map_err(|errors| {
            let said: Vec<String> = errors
                .into_iter()
                .flat_map(|(_, undefined)| undefined)
                .map(|(name, kind)| format!("{} `{name}` is not defined", kind.as_str()))
                .collect();
            Failure::Error(said.join("; "))
        })
}

/// `filter` over every value in `input`, written to `out`.
///
/// Runs on the step child's thread (`super::child`): the evaluation may take
/// [`EVAL_STACK`] below this call, which a smaller stack does not have.
pub(super) fn run(filter: &str, input: &[u8], out: &mut Capped<'_>) -> Result<(), Failure> {
    let floor = stack_here().saturating_sub(EVAL_STACK);
    let compiled = compile(filter)?;
    if nesting(input) > NESTING {
        return Err(too_nested());
    }
    let pp = layout();
    let ran = catch_unwind(AssertUnwindSafe(|| {
        for value in jaq_json::read::parse_many(input) {
            let value =
                value.map_err(|error| Failure::Error(format!("the input is not JSON: {error}")))?;
            let data = Watched {
                lut: &compiled.lut,
                floor,
            };
            let ctx = Ctx::<Guarded>::new(data, Vars::new([]));
            for output in compiled.id.run((ctx, value)).map(unwrap_valr) {
                let output =
                    output.map_err(|error| Failure::Error(format!("jq: error: {error}")))?;
                write_value(out, &pp, &output)?;
            }
        }
        Ok(())
    }));
    ran.unwrap_or_else(|unwound| match unwound.downcast::<Refused>() {
        Ok(refused) => Err(match *refused {
            Refused::TooDeep => Failure::Error(format!(
                "the filter recursed too deep: it took {} MiB of stack",
                EVAL_STACK / (1024 * 1024)
            )),
            Refused::Nested => Failure::Error(format!(
                "fromjson was given JSON that nests deeper than {NESTING} levels"
            )),
        }),
        Err(other) => resume_unwind(other),
    })
}

fn write_value(out: &mut Capped<'_>, pp: &Pp, value: &Val) -> Result<(), Failure> {
    write(out, pp, 0, value)
        .and_then(|()| out.write_all(b"\n"))
        .map_err(|error| super::steps::io_failure(&error))
}

/// Every JSON value in `input`, laid out as `jq .` lays it out.
pub(super) fn pretty(input: &[u8], out: &mut Capped<'_>) -> Result<(), Failure> {
    if nesting(input) > NESTING {
        return Err(too_nested());
    }
    let pp = layout();
    for value in jaq_json::read::parse_many(input) {
        let value = value.map_err(|error| Failure::Error(format!("not JSON: {error}")))?;
        write_value(out, &pp, &value)?;
    }
    Ok(())
}

/// The loader's errors as one line: what was expected, and where in the
/// filter (a 1-based column, or its end).
fn parse_errors(code: &str, errors: jaq_core::load::Errors<&str, ()>) -> String {
    use jaq_core::load::Error;
    let at = |part: &str| {
        if part.is_empty() {
            "the end of the filter".to_owned()
        } else {
            let start = jaq_core::load::span(code, part).start;
            format!("column {}", code[..start].chars().count() + 1)
        }
    };
    let mut said = Vec::new();
    for (_, error) in errors {
        match error {
            Error::Io(errors) => {
                said.extend(
                    errors
                        .into_iter()
                        .map(|(path, error)| format!("{path}: {error}")),
                );
            }
            Error::Lex(errors) => said.extend(
                errors
                    .into_iter()
                    .map(|(expect, part)| format!("expected {} at {}", expect.as_str(), at(part))),
            ),
            Error::Parse(errors) => said.extend(
                errors
                    .into_iter()
                    .map(|(expect, part)| format!("expected {} at {}", expect.as_str(), at(part))),
            ),
        }
    }
    format!("the filter does not parse: {}", said.join("; "))
}

// ---- A session (the XML view's design, section 26): the document read once, a filter a key ----

/// The most values an answer carries, the most paths, and the most bytes of values.
pub const SESSION_VALUES: usize = 500;
pub const SESSION_PATHS: usize = 5_000;
const SESSION_VALUE_BYTES: usize = 4 * 1024 * 1024;

/// What a filter answered in a session.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionAnswer {
    /// The first values, compact, at most [`SESSION_VALUES`] and 4 MiB.
    pub values: Vec<String>,
    /// How many values the filter gave in all.
    pub count: u64,
    /// The paths of what it picked, each a list of keys and indexes, at most
    /// [`SESSION_PATHS`]; `None` where it makes new values (jq: *Invalid path
    /// expression*).
    pub paths: Option<Vec<Vec<serde_json::Value>>>,
    /// The filter's error, where it has one: nothing else is answered.
    pub error: Option<String>,
}

/// The document's values, read once for a session.
///
/// # Errors
///
/// The document is not JSON, or nests past [`NESTING`].
pub(super) fn session_inputs(input: &[u8]) -> Result<Vec<Val>, Failure> {
    if nesting(input) > NESTING {
        return Err(too_nested());
    }
    jaq_json::read::parse_many(input)
        .map(|value| {
            value.map_err(|error| Failure::Error(format!("the document is not JSON: {error}")))
        })
        .collect()
}

/// `filter` over the session's values: the first values and their count, then
/// the paths of what it picks (`path(filter)`), each evaluation under the
/// stack watch.
pub(super) fn session_ask(filter: &str, inputs: &[Val]) -> SessionAnswer {
    let floor = stack_here().saturating_sub(EVAL_STACK);
    let failed = |error: Failure| SessionAnswer {
        error: Some(match error {
            Failure::Error(said) => said,
            Failure::TooLarge => "the answer is too large".to_owned(),
            Failure::Stopped | Failure::Busy => "the filter was stopped".to_owned(),
        }),
        ..SessionAnswer::default()
    };
    let values = match compile(filter) {
        Ok(compiled) => compiled,
        Err(error) => return failed(error),
    };
    let compact = Pp::default();
    let ran = catch_unwind(AssertUnwindSafe(
        || -> Result<(Vec<String>, u64), Failure> {
            let mut out = Vec::new();
            let mut bytes = 0usize;
            let mut count = 0u64;
            for input in inputs {
                let ctx = Ctx::<Guarded>::new(
                    Watched {
                        lut: &values.lut,
                        floor,
                    },
                    Vars::new([]),
                );
                for output in values.id.run((ctx, input.clone())).map(unwrap_valr) {
                    let output =
                        output.map_err(|error| Failure::Error(format!("jq: error: {error}")))?;
                    count += 1;
                    if out.len() < SESSION_VALUES && bytes < SESSION_VALUE_BYTES {
                        let mut text = Vec::new();
                        write(&mut text, &compact, 0, &output)
                            .map_err(|error| super::steps::io_failure(&error))?;
                        bytes += text.len();
                        out.push(String::from_utf8_lossy(&text).into_owned());
                    }
                }
            }
            Ok((out, count))
        },
    ));
    let (values, count) = match ran {
        Ok(Ok(answered)) => answered,
        Ok(Err(error)) => return failed(error),
        Err(unwound) => return failed(refusal(unwound)),
    };
    // What it picked, by path: a filter that makes values has none.
    let paths = compile(&format!("path({filter})"))
        .ok()
        .and_then(|compiled| {
            catch_unwind(AssertUnwindSafe(
                || -> Option<Vec<Vec<serde_json::Value>>> {
                    let mut out = Vec::new();
                    for input in inputs {
                        let ctx = Ctx::<Guarded>::new(
                            Watched {
                                lut: &compiled.lut,
                                floor,
                            },
                            Vars::new([]),
                        );
                        for path in compiled.id.run((ctx, input.clone())).map(unwrap_valr) {
                            let path = path.ok()?;
                            if out.len() < SESSION_PATHS {
                                let mut text = Vec::new();
                                write(&mut text, &Pp::default(), 0, &path).ok()?;
                                out.push(serde_json::from_slice(&text).ok()?);
                            }
                        }
                    }
                    Some(out)
                },
            ))
            .ok()
            .flatten()
        });
    SessionAnswer {
        values,
        count,
        paths,
        error: None,
    }
}

/// What an unwound evaluation means.
fn refusal(unwound: Box<dyn std::any::Any + Send>) -> Failure {
    match unwound.downcast::<Refused>() {
        Ok(refused) => match *refused {
            Refused::TooDeep => Failure::Error(format!(
                "the filter recursed too deep: it took {} MiB of stack",
                EVAL_STACK / (1024 * 1024)
            )),
            Refused::Nested => Failure::Error(format!(
                "fromjson was given JSON that nests deeper than {NESTING} levels"
            )),
        },
        Err(other) => resume_unwind(other),
    }
}
