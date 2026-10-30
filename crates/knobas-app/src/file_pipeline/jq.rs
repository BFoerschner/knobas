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
//! Not covered (#2314): a value nested deep at run time without recursion
//! (`reduce range(1e6) as $_ (0; [.])`) is still written, compared and
//! dropped by recursion below jaq, and can overflow the stack.

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

/// `filter` over every value in `input`, written to `out`.
///
/// Runs on a pipeline worker: the evaluation may take [`EVAL_STACK`] below
/// this call, which a smaller stack does not have.
pub(super) fn run(filter: &str, input: &[u8], out: &mut Capped<'_>) -> Result<(), Failure> {
    let floor = stack_here().saturating_sub(EVAL_STACK);
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
    let compiled = Compiler::default()
        .with_funs(funs())
        .compile(modules)
        .map_err(|errors| {
            let said: Vec<String> = errors
                .into_iter()
                .flat_map(|(_, undefined)| undefined)
                .map(|(name, kind)| format!("{} `{name}` is not defined", kind.as_str()))
                .collect();
            Failure::Error(said.join("; "))
        })?;
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
