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
#[derive(Debug, Clone, PartialEq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct InstalledFont {
    /// The family name, as `font-family` takes it.
    pub family: String,
    /// A face of the family says it is fixed pitch (its `post` table's
    /// `isFixedPitch`), or draws `i`, `M`, `0`, `.` and a space at one
    /// advance: the picker lists these first.
    pub monospace: bool,
    /// The family's upright regular face is classed as a serif face (its
    /// `OS/2` table's family class, else its PANOSE serif style): *Reading*
    /// lists these first (Settings › Fonts, 2026-11-17).
    pub serif: bool,
    /// The family can draw narrow: a face of it has a `wdth` variation axis,
    /// or is a condensed member (its `OS/2` width class under normal), so
    /// `font-stretch` narrows it as `"wdth"` narrows Archivo. *Headings* lists
    /// these first and marks the rest "normal width only".
    pub narrows: bool,
    /// The upright regular face's x-height over its em (Archivo's is about
    /// .52), for *Even out*: the size that draws its lower case as tall as
    /// the bundled face's. `None` when the face does not say.
    pub x_height: Option<f32>,
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
    // Each family's face nearest an upright 400: the one its class and x-height are read from.
    let mut regular: std::collections::HashMap<String, (u32, fontdb::ID)> =
        std::collections::HashMap::new();
    let mut faces = Vec::new();
    for face in db.faces() {
        let Some((name, _)) = face.families.first() else {
            continue;
        };
        let distance = u32::from(face.weight.0.abs_diff(400))
            + if face.style == fontdb::Style::Normal {
                0
            } else {
                1000
            };
        let best = regular
            .entry(name.to_lowercase())
            .or_insert((distance, face.id));
        if distance < best.0 {
            *best = (distance, face.id);
        }
        faces.push((
            name.clone(),
            face.monospaced,
            face.stretch < fontdb::Stretch::Normal,
        ));
    }
    let traits: std::collections::HashMap<String, Traits> = regular
        .into_iter()
        .map(|(name, (_, id))| {
            let traits = db.with_face_data(id, read_traits).unwrap_or_default();
            (name, traits)
        })
        .collect();
    families(faces.iter().map(|(name, monospace, condensed)| {
        let mut traits = traits
            .get(&name.to_lowercase())
            .copied()
            .unwrap_or_default();
        traits.narrows |= condensed;
        (name.as_str(), *monospace || traits.even, traits)
    }))
}

/// What one face says of its family, read from its tables (no glyph drawn).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Traits {
    /// `i`, `M`, `0`, `.` and a space at one advance ([`even_advances`]).
    even: bool,
    serif: bool,
    narrows: bool,
    x_height: Option<f32>,
}

fn read_traits(data: &[u8], index: u32) -> Traits {
    let Ok(face) = ttf_parser::Face::parse(data, index) else {
        return Traits::default();
    };
    let em = f32::from(face.units_per_em());
    let os2 = face.raw_face().table(ttf_parser::Tag::from_bytes(b"OS/2"));
    Traits {
        even: even_advances(&face),
        serif: os2.is_some_and(serif_class),
        narrows: face
            .variation_axes()
            .into_iter()
            .any(|axis| axis.tag == ttf_parser::Tag::from_bytes(b"wdth")),
        x_height: face
            .x_height()
            .filter(|height| *height > 0 && em > 0.0)
            .map(|height| f32::from(height) / em),
    }
}

/// Whether an `OS/2` table classes its face as a serif: `sFamilyClass`'s
/// class (offset 30) 1 to 5 or 7 (the oldstyle, transitional, modern,
/// clarendon, slab and freeform serifs), or, where the class is 0 (none
/// given), a Latin text PANOSE (offset 32) whose serif style is 2 to 10.
fn serif_class(os2: &[u8]) -> bool {
    match os2.get(30) {
        Some(1..=5 | 7) => true,
        Some(0) => os2.get(32) == Some(&2) && matches!(os2.get(33), Some(2..=10)),
        _ => false,
    }
}

/// Whether the face draws `i`, `M`, `0`, `.` and a space at one advance: a
/// coding font whose `post` table does not say it is fixed pitch (a Nerd Font
/// patch's proportional build of a monospace face, say).
fn even_advances(face: &ttf_parser::Face<'_>) -> bool {
    let mut advances = ['i', 'M', '0', '.', ' '].into_iter().map(|ch| {
        face.glyph_index(ch)
            .and_then(|glyph| face.glyph_hor_advance(glyph))
    });
    let Some(Some(first)) = advances.next() else {
        return false;
    };
    first > 0 && advances.all(|advance| advance == Some(first))
}

/// Whether a family's name says serif where its tables do not: Apple's own
/// faces (Times, Palatino, New York, Didot) give no class and no PANOSE. A
/// name with *Serif* in it (not *Sans Serif*), or one of the serif families
/// macOS and Windows install.
fn serif_name(name: &str) -> bool {
    const KNOWN: [&str; 22] = [
        "times",
        "palatino",
        "new york",
        "didot",
        "bodoni",
        "big caslon",
        "cochin",
        "hoefler text",
        "athelas",
        "baskerville",
        "charter",
        "georgia",
        "iowan old style",
        "garamond",
        "book antiqua",
        "cambria",
        "constantia",
        "sitka",
        "mincho",
        "songti",
        "minion",
        "caslon",
    ];
    let name = name.to_lowercase();
    (name.contains("serif") && !name.contains("sans"))
        || KNOWN.iter().any(|known| name.contains(known))
}

/// `faces` (a family name, whether that face is fixed pitch, and what its
/// family's regular face says) as one row a family: names compared without
/// case, the first spelling kept; a family is monospace when any face of it
/// is. macOS's hidden system families (a name starting with `.`, as
/// `.SF NS`) are left out: they are not for picking.
fn families<'a>(faces: impl Iterator<Item = (&'a str, bool, Traits)>) -> Vec<InstalledFont> {
    let mut by_name: std::collections::HashMap<String, InstalledFont> =
        std::collections::HashMap::new();
    for (name, monospace, traits) in faces {
        let name = name.trim();
        if name.is_empty() || name.starts_with('.') {
            continue;
        }
        by_name
            .entry(name.to_lowercase())
            .and_modify(|font| {
                font.monospace |= monospace;
                font.narrows |= traits.narrows;
            })
            .or_insert_with(|| InstalledFont {
                family: name.to_owned(),
                monospace,
                serif: traits.serif || serif_name(name),
                narrows: traits.narrows,
                x_height: traits.x_height,
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
            serif: false,
            narrows: false,
            x_height: None,
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
            families(
                faces
                    .into_iter()
                    .map(|(name, mono)| (name, mono, Traits::default()))
            ),
            vec![
                font("Fira Code", true),
                font("Menlo", true),
                font("archivo", false),
                font("Helvetica", false)
            ]
        );
    }

    #[test]
    fn a_family_carries_its_regular_face_s_class_axis_and_x_height() {
        let serif = Traits {
            serif: true,
            x_height: Some(0.45),
            ..Traits::default()
        };
        let narrow = Traits {
            narrows: true,
            ..Traits::default()
        };
        let found = families(
            [
                ("Source Serif 4", false, serif),
                ("Archivo", false, narrow),
                ("source serif 4", false, Traits::default()),
            ]
            .into_iter(),
        );
        assert_eq!(found.len(), 2);
        let archivo = &found[0];
        assert!(archivo.narrows && !archivo.serif && archivo.x_height.is_none());
        let source = &found[1];
        assert_eq!(
            (
                source.family.as_str(),
                source.serif,
                source.narrows,
                source.x_height
            ),
            ("Source Serif 4", true, false, Some(0.45))
        );
    }

    #[test]
    fn a_serif_is_told_by_its_family_class_then_its_panose() {
        let table = |class: u8, panose: [u8; 2]| {
            let mut os2 = vec![0u8; 42];
            os2[30] = class;
            os2[32] = panose[0];
            os2[33] = panose[1];
            os2
        };
        // Oldstyle, transitional, modern, clarendon, slab and freeform serifs.
        for class in [1, 2, 3, 4, 5, 7] {
            assert!(serif_class(&table(class, [0, 0])), "class {class}");
        }
        // Sans serif, ornamental, script, symbolic.
        for class in [8, 9, 10, 12] {
            assert!(!serif_class(&table(class, [2, 3])), "class {class}");
        }
        // No class: Latin text PANOSE, cove serifs to rounded, against sans (11 to 13).
        assert!(serif_class(&table(0, [2, 2])));
        assert!(serif_class(&table(0, [2, 10])));
        assert!(!serif_class(&table(0, [2, 11])));
        assert!(!serif_class(&table(0, [3, 2])));
        assert!(!serif_class(&[0u8; 20]));
    }

    #[test]
    fn a_serif_is_told_by_its_name_where_its_tables_say_nothing() {
        for name in [
            "Times",
            "Palatino",
            "New York",
            "Source Serif 4",
            "Noto Serif",
            "Hiragino Mincho ProN",
        ] {
            assert!(serif_name(name), "{name}");
        }
        for name in [
            "Helvetica",
            "Noto Sans",
            "Microsoft Sans Serif",
            "PT Sans",
            "Avenir",
        ] {
            assert!(!serif_name(name), "{name}");
        }
    }

    /// The real scan, timed: `cargo test -p knobas-app --lib code_font -- --ignored --nocapture`.
    #[test]
    #[ignore = "reads this machine's fonts"]
    fn the_scan_of_this_machine() {
        let started = std::time::Instant::now();
        let found = scan();
        eprintln!(
            "{} families, {} monospace, {} serif, {} that narrow, in {:?}",
            found.len(),
            found.iter().filter(|font| font.monospace).count(),
            found.iter().filter(|font| font.serif).count(),
            found.iter().filter(|font| font.narrows).count(),
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
        for (kind, test) in [
            (
                "serif",
                (|font: &InstalledFont| font.serif) as fn(&InstalledFont) -> bool,
            ),
            ("narrows", |font: &InstalledFont| font.narrows),
        ] {
            let names: Vec<&str> = found
                .iter()
                .filter(|font| test(font))
                .map(|font| font.family.as_str())
                .collect();
            eprintln!("{kind}: {names:?}");
        }
        assert!(!found.is_empty());
    }
}
