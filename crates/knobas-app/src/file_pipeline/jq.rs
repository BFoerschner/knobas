//! jq through `jaq` (#2264): the filter is parsed, compiled and run in
//! process, over every JSON value in the input in turn, as `jq` reads a
//! stream. Each output is written as `jq` writes it by default: two-space
//! indent, one value a line group.

use std::io::Write as _;

use jaq_core::load::{Arena, File, Loader};
use jaq_core::{Compiler, Ctx, Vars, data, unwrap_valr};
use jaq_json::Val;
use jaq_json::write::{Pp, write};

use super::steps::{Capped, Failure};

fn layout() -> Pp {
    Pp {
        indent: Some("  ".to_owned()),
        sep_space: true,
        ..Pp::default()
    }
}

/// `filter` over every value in `input`, written to `out`.
pub(super) fn run(filter: &str, input: &[u8], out: &mut Capped<'_>) -> Result<(), Failure> {
    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs());
    let funs = jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs());
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
        .with_funs(funs)
        .compile(modules)
        .map_err(|errors| {
            let said: Vec<String> = errors
                .into_iter()
                .flat_map(|(_, undefined)| undefined)
                .map(|(name, kind)| format!("{} `{name}` is not defined", kind.as_str()))
                .collect();
            Failure::Error(said.join("; "))
        })?;
    let pp = layout();
    for value in jaq_json::read::parse_many(input) {
        let value =
            value.map_err(|error| Failure::Error(format!("the input is not JSON: {error}")))?;
        let ctx = Ctx::<data::JustLut<Val>>::new(&compiled.lut, Vars::new([]));
        for output in compiled.id.run((ctx, value)).map(unwrap_valr) {
            let output = output.map_err(|error| Failure::Error(format!("jq: error: {error}")))?;
            write_value(out, &pp, &output)?;
        }
    }
    Ok(())
}

fn write_value(out: &mut Capped<'_>, pp: &Pp, value: &Val) -> Result<(), Failure> {
    write(out, pp, 0, value)
        .and_then(|()| out.write_all(b"\n"))
        .map_err(|error| super::steps::io_failure(&error))
}

/// Every JSON value in `input`, laid out as `jq .` lays it out.
pub(super) fn pretty(input: &[u8]) -> Result<String, Failure> {
    let pp = layout();
    let mut out = Vec::new();
    for value in jaq_json::read::parse_many(input) {
        let value = value.map_err(|error| Failure::Error(format!("not JSON: {error}")))?;
        write(&mut out, &pp, 0, &value)
            .and_then(|()| out.write_all(b"\n"))
            .map_err(|error| Failure::Error(error.to_string()))?;
    }
    String::from_utf8(out).map_err(|error| Failure::Error(error.to_string()))
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
                said.extend(errors.into_iter().map(|(path, error)| format!("{path}: {error}")));
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
