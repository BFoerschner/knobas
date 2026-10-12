//! Ids and names that go into a request path, checked or escaped in one
//! place for the view's reads and the parameter write alike.

use knobas_source::SourceError;

/// A configuration id as TeamCity issues them -- letters, digits and `_` --
/// and so nothing that could leave the path segment or the locator it goes in.
pub(crate) fn config_id<'k>(entity: &str, id: &'k str) -> Result<&'k str, SourceError> {
    let ok = !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    ok.then_some(id).ok_or_else(|| {
        SourceError::protocol(format!(
            "teamcity: {entity:?} does not name a build configuration TeamCity could have issued"
        ))
    })
}

/// A file or parameter name as a URL path segment: everything but the
/// unreserved characters percent-encoded.
pub(crate) fn path_escaped(name: &str) -> String {
    name.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// A parameter's name as the one path segment of its resource.
///
/// Escaped like any other name; and refused when empty or `.` / `..`, which
/// survive the escaping and are dot-segments to a URL -- they would aim the
/// request at another resource, and no parameter is called that.
///
/// # Errors
///
/// [`SourceError::Protocol`] for those three names.
pub(crate) fn parameter_segment(name: &str) -> Result<String, SourceError> {
    if matches!(name, "" | "." | "..") {
        return Err(SourceError::protocol(format!(
            "teamcity: {name:?} is not a parameter name"
        )));
    }
    Ok(path_escaped(name))
}
