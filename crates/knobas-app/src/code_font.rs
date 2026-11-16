//! *Code font*, Settings › Appearance (2026-11-16): the font families
//! installed on this machine, for the picker that chooses the face code is
//! drawn in.
//!
//! The webview cannot list them: WKWebView has no `queryLocalFonts`, and
//! WebView2's asks the person for a permission first. So the backend reads the
//! system's font folders with `fontdb` (the faces' name and `post` tables,
//! memory-mapped, no glyph decoded) once, on a blocking thread, the first time
//! the picker asks, and keeps the answer for the run: a font installed while
//! knobas runs shows after a restart. Settings asks as it opens, so the list
//! is in hand by the time the picker is.
//!
//! The choice itself is not stored here: it is the webview's
//! (`app/src/lib/tf/code-face.svelte.ts`), read before the first draw so code
//! never draws in one face and then another, and it belongs to this machine,
//! whose fonts it names, not to the profile a backup carries.

use tokio::sync::OnceCell;

use crate::IpcError;

/// One installed font family.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct InstalledFont {
    /// The family name, as `font-family` takes it.
    pub family: String,
    /// A face of the family says it is fixed pitch (its `post` table's
    /// `isFixedPitch`), or draws `i`, `M`, `0`, `.` and a space at one
    /// advance: the picker lists these first.
    pub monospace: bool,
}

static FONTS: OnceCell<Vec<InstalledFont>> = OnceCell::const_new();

/// The installed families, monospace ones first, each part by name; read
/// once a run.
///
/// # Errors
///
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the blocking
/// read panics.
pub async fn installed() -> Result<Vec<InstalledFont>, IpcError> {
    FONTS
        .get_or_try_init(|| async {
            tokio::task::spawn_blocking(scan).await.map_err(|error| {
                IpcError::internal(format!("the installed fonts could not be read: {error}"))
            })
        })
        .await
        .cloned()
}

/// Every face in the system's font folders, as families.
fn scan() -> Vec<InstalledFont> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    let mut weighed: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
    let mut faces = Vec::new();
    for face in db.faces() {
        let Some((name, _)) = face.families.first() else {
            continue;
        };
        // A face's advances are read once a family, and only where no face said fixed pitch yet.
        let monospace = face.monospaced
            || *weighed
                .entry(name.to_lowercase())
                .or_insert_with(|| db.with_face_data(face.id, even_advances).unwrap_or(false));
        faces.push((name.clone(), monospace));
    }
    families(
        faces
            .iter()
            .map(|(name, monospace)| (name.as_str(), *monospace)),
    )
}

/// Whether the face draws `i`, `M`, `0`, `.` and a space at one advance: a
/// coding font whose `post` table does not say it is fixed pitch (a Nerd Font
/// patch's proportional build of a monospace face, say).
fn even_advances(data: &[u8], index: u32) -> bool {
    let Ok(face) = ttf_parser::Face::parse(data, index) else {
        return false;
    };
    let mut advances = ['i', 'M', '0', '.', ' '].into_iter().map(|ch| {
        face.glyph_index(ch)
            .and_then(|glyph| face.glyph_hor_advance(glyph))
    });
    let Some(Some(first)) = advances.next() else {
        return false;
    };
    first > 0 && advances.all(|advance| advance == Some(first))
}

/// `faces` (a family name and whether that face is fixed pitch) as one row a
/// family: names compared without case, the first spelling kept; a family is
/// monospace when any face of it is. macOS's hidden system families (a name
/// starting with `.`, as `.SF NS`) are left out: they are not for picking.
fn families<'a>(faces: impl Iterator<Item = (&'a str, bool)>) -> Vec<InstalledFont> {
    let mut by_name: std::collections::HashMap<String, InstalledFont> =
        std::collections::HashMap::new();
    for (name, monospace) in faces {
        let name = name.trim();
        if name.is_empty() || name.starts_with('.') {
            continue;
        }
        by_name
            .entry(name.to_lowercase())
            .and_modify(|font| font.monospace |= monospace)
            .or_insert_with(|| InstalledFont {
                family: name.to_owned(),
                monospace,
            });
    }
    let mut found: Vec<InstalledFont> = by_name.into_values().collect();
    found.sort_by(|a, b| {
        b.monospace
            .cmp(&a.monospace)
            .then_with(|| a.family.to_lowercase().cmp(&b.family.to_lowercase()))
            .then_with(|| a.family.cmp(&b.family))
    });
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font(family: &str, monospace: bool) -> InstalledFont {
        InstalledFont {
            family: family.to_owned(),
            monospace,
        }
    }

    #[test]
    fn a_family_is_one_row_monospace_first_then_by_name() {
        let faces = [
            ("Helvetica", false),
            ("Menlo", true),
            ("Menlo", true),
            ("archivo", false),
            ("Fira Code", false),
            ("Fira Code", true),
            (".SF NS Mono", true),
            ("  ", false),
            ("FIRA CODE", false),
        ];
        assert_eq!(
            families(faces.into_iter()),
            vec![
                font("Fira Code", true),
                font("Menlo", true),
                font("archivo", false),
                font("Helvetica", false)
            ]
        );
    }

    /// The real scan, timed: `cargo test -p knobas-app --lib code_font -- --ignored --nocapture`.
    #[test]
    #[ignore = "reads this machine's fonts"]
    fn the_scan_of_this_machine() {
        let started = std::time::Instant::now();
        let found = scan();
        eprintln!(
            "{} families, {} monospace, in {:?}",
            found.len(),
            found.iter().filter(|font| font.monospace).count(),
            started.elapsed()
        );
        eprintln!(
            "{:?}",
            found
                .iter()
                .filter(|font| font.monospace)
                .map(|font| &font.family)
                .collect::<Vec<_>>()
        );
        assert!(!found.is_empty());
    }
}
