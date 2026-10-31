//! The address rule shared by every user-named record that gets an id from its
//! name (ticket views, link sets): lowercase letters and digits joined by
//! single hyphens, at most [`MAX_ID_CHARS`] long.

/// How long an id may grow before the slug stops, so an address stays short.
pub(crate) const MAX_ID_CHARS: usize = 48;

/// Whether `id` is already an address: non-empty, lowercase letters, digits
/// and single hyphens, none at either end.
pub(crate) fn is_slug(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('-')
        && !id.ends_with('-')
        && !id.contains("--")
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A name as an address: lowercase letters and digits joined by single
/// hyphens, `fallback` for a name with neither.
pub(crate) fn slug(name: &str, fallback: &str) -> String {
    let mut out = String::new();
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
        if out.trim_end_matches('-').len() >= MAX_ID_CHARS {
            break;
        }
    }
    let slug = out.trim_matches('-').to_owned();
    if slug.is_empty() {
        fallback.to_owned()
    } else {
        slug
    }
}

/// The first free id for `base`: `base` itself, else `base-2`, `base-3`, ...
/// `taken` holds the stored ids equal to `base` or starting `base-`.
pub(crate) fn first_free(base: &str, taken: &[String]) -> String {
    let mut candidate = base.to_owned();
    let mut suffix = 1_u32;
    while taken.contains(&candidate) {
        suffix += 1;
        candidate = format!("{base}-{suffix}");
    }
    candidate
}

/// A saved list's id (user renderers, command steps): `given` when it is a
/// free slug, else one made from `name` (`fallback` for a name with no
/// letters), `-2` and on for a clash with `taken`.
pub(crate) fn settled_id(
    given: Option<&str>,
    name: &str,
    fallback: &str,
    taken: &[&str],
) -> String {
    let given = given.map(str::trim).unwrap_or_default();
    if is_slug(given) && !taken.contains(&given) {
        return given.to_owned();
    }
    let base = slug(name, fallback);
    let mut id = base.clone();
    let mut n = 2;
    while taken.contains(&id.as_str()) {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slug_is_an_address_whatever_the_name_was() {
        for (name, expected) in [
            ("My work", "my-work"),
            ("  Payments   backlog ", "payments-backlog"),
            ("...", "view"),
            ("\u{4e2d}\u{6587}", "view"),
        ] {
            let made = slug(name, "view");
            assert_eq!(made, expected, "{name:?}");
            assert!(is_slug(&made), "{made:?}");
        }
        assert_eq!(slug("...", "set"), "set");
        assert!(slug(&"a".repeat(200), "view").len() <= MAX_ID_CHARS);
    }

    /// A saved list's id (renderers, command steps): the draft's own when it
    /// is a free slug, else one from the name, counted up past a clash.
    #[test]
    fn a_settled_id_keeps_a_free_given_one_and_counts_up_a_made_one() {
        assert_eq!(settled_id(Some(" kept "), "Cat", "renderer", &[]), "kept");
        assert_eq!(
            settled_id(Some("kept"), "Cat", "renderer", &["kept"]),
            "cat"
        );
        assert_eq!(
            settled_id(Some("Not A Slug"), "Cat", "renderer", &[]),
            "cat"
        );
        assert_eq!(
            settled_id(None, "Cat", "renderer", &["cat", "cat-2"]),
            "cat-3"
        );
        assert_eq!(settled_id(None, "...", "step", &[]), "step");
    }

    #[test]
    fn the_first_free_id_counts_up_from_two() {
        let taken = |ids: &[&str]| ids.iter().map(|id| (*id).to_owned()).collect::<Vec<_>>();
        assert_eq!(first_free("mine", &[]), "mine");
        assert_eq!(first_free("mine", &taken(&["mine"])), "mine-2");
        assert_eq!(first_free("mine", &taken(&["mine", "mine-2"])), "mine-3");
        assert_eq!(first_free("mine", &taken(&["mine-2"])), "mine");
    }
}
