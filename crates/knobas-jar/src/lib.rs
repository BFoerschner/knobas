//! A Java archive read for the Code page's JAR view (ux19,
//! `docs/decisions/2026-11-18-a-jar-is-drawn-as-its-contents.md`): a `.jar`, `.war`, `.ear` or `.aar` listed from its
//! zip central directory, what its manifest and Maven files say, its nested JARs with their versions, and any class
//! outlined as `javap` would, all without Java and without unpacking anything but the entries asked for.
//!
//! Pure and synchronous: it reads from any `Read + Seek` (a spooled file in the app, a cursor in a test) and writes
//! nothing but what a caller hands it.

pub mod class;
pub mod meta;
pub mod zip;

use std::collections::HashSet;
use std::io::{Read, Seek};

use serde::Serialize;

pub use class::{ClassOutline, Member};
pub use meta::Maven;
pub use zip::{Directory, Entry, ZipError};

/// The most a text entry read for the head (a manifest, a `pom.xml`) may unpack to.
const META_CAP: u64 = 1024 * 1024;
/// How many classes are sampled for the archive's Java release.
const RELEASE_SAMPLE: usize = 24;

/// Flags of an entry in [`ArchiveIndex::flags`].
pub mod flag {
    /// The name leaves the archive; listed, never opened.
    pub const UNSAFE: u8 = 1;
    /// Encrypted, or packed with a method knobas does not unpack.
    pub const UNREADABLE: u8 = 2;
    /// A second entry of a name already listed.
    pub const DUPLICATE: u8 = 4;
    /// Stored, not deflated: a nested JAR so stored is read in place.
    pub const STORED: u8 = 8;
}

/// An archive's contents and what it says about itself: the JAR view's first read.
///
/// The entries are columns, sorted by name, so 10,000 entries cross the bridge as one string and three arrays rather
/// than 10,000 objects. Folders are not listed: the view makes them from the names.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct ArchiveIndex {
    /// What later reads of this archive name it by (`archive_entry`, `archive_class`, a nested JAR's index).
    pub handle: String,
    /// `jar`, `war`, `ear`, `aar`, `boot-jar` (Spring Boot's fat JAR) or `boot-war`.
    pub kind: String,
    /// The archive's own size, in bytes.
    pub size: u64,
    /// Every entry's unpacked and packed sizes, summed.
    pub total: u64,
    pub packed: u64,
    /// The entries' names, one a line.
    pub names: String,
    pub sizes: Vec<u64>,
    pub packed_sizes: Vec<u64>,
    /// Each entry's CRC-32 as its directory declares it, aligned with `sizes`: what a comparison of two archives tells a
    /// changed entry by, with no entry read.
    pub crcs: Vec<u32>,
    /// [`flag`]s per entry.
    pub flags: Vec<u8>,
    /// The manifest's main section, in its order.
    pub manifest: Vec<(String, String)>,
    /// Maven's coordinates found inside, the archive's own first where one matches its name.
    pub maven: Vec<Maven>,
    /// The Java release most of its classes are compiled for (`Java 17`), from a sample; `null` with no classes.
    pub release: Option<String>,
    /// A multi-release JAR's versions (`META-INF/versions/<n>/`).
    pub releases: Vec<u16>,
    /// The JARs inside it (`BOOT-INF/lib/`, `WEB-INF/lib/`, an EAR's modules): its dependency list.
    pub libs: Vec<Lib>,
    /// Bytes before the archive (a launch script).
    pub prefix: u64,
    /// What the reading passed over: unsafe names, encrypted entries, duplicates.
    pub warnings: Vec<String>,
}

/// A JAR inside the archive.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct Lib {
    /// Its entry's name (`BOOT-INF/lib/spring-core-6.1.2.jar`).
    pub entry: String,
    /// From its file name: `spring-core`, `6.1.2`.
    pub name: String,
    pub version: Option<String>,
    pub size: u64,
}

fn is_archive_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".jar", ".war", ".ear", ".aar"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

/// Reads `name` from the archive as text, if it is there and small.
fn text_of<R: Read + Seek + ?Sized>(r: &mut R, dir: &Directory, entry: &Entry) -> Option<String> {
    let bytes = zip::read_entry(r, dir, entry, META_CAP).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// The archive listed in `dir`, read from `r`; `file_name` is its own name (`app.jar`), which picks its kind and its
/// own Maven coordinates among several.
pub fn index<R: Read + Seek + ?Sized>(r: &mut R, dir: &Directory, file_name: &str) -> ArchiveIndex {
    let mut order: Vec<usize> = (0..dir.entries.len())
        .filter(|&i| !dir.entries[i].is_dir())
        .collect();
    order.sort_by(|&a, &b| dir.entries[a].name.cmp(&dir.entries[b].name));

    let mut names =
        String::with_capacity(order.iter().map(|&i| dir.entries[i].name.len() + 1).sum());
    let mut sizes = Vec::with_capacity(order.len());
    let mut packed_sizes = Vec::with_capacity(order.len());
    let mut crcs = Vec::with_capacity(order.len());
    let mut flags = Vec::with_capacity(order.len());
    let mut seen: HashSet<&str> = HashSet::with_capacity(order.len());
    let (mut total, mut packed) = (0u64, 0u64);
    let (mut unsafe_count, mut unreadable, mut duplicates) = (0usize, 0usize, 0usize);
    let mut releases: Vec<u16> = Vec::new();
    let mut libs = Vec::new();
    let mut boot = false;
    for (n, &i) in order.iter().enumerate() {
        let e = &dir.entries[i];
        if n > 0 {
            names.push('\n');
        }
        names.extend(e.name.chars().map(|c| {
            if c == '\n' || c == '\r' {
                '\u{FFFD}'
            } else {
                c
            }
        }));
        sizes.push(e.size);
        packed_sizes.push(e.csize);
        crcs.push(e.crc);
        total = total.saturating_add(e.size);
        packed = packed.saturating_add(e.csize);
        let mut f = 0u8;
        if e.unsafe_name {
            f |= flag::UNSAFE;
            unsafe_count += 1;
        }
        if !e.readable() {
            f |= flag::UNREADABLE;
            unreadable += 1;
        }
        if !seen.insert(e.name.as_str()) {
            f |= flag::DUPLICATE;
            duplicates += 1;
        }
        if e.method == 0 {
            f |= flag::STORED;
        }
        flags.push(f);
        if let Some(rest) = e.name.strip_prefix("META-INF/versions/")
            && let Some(v) = rest.split('/').next().and_then(|v| v.parse::<u16>().ok())
            && !releases.contains(&v)
        {
            releases.push(v);
        }
        if e.name.starts_with("BOOT-INF/") || e.name.starts_with("org/springframework/boot/loader/")
        {
            boot = true;
        }
        if is_archive_name(&e.name) && !e.unsafe_name {
            let (name, version) = meta::name_version(&e.name);
            libs.push(Lib {
                entry: e.name.clone(),
                name,
                version,
                size: e.size,
            });
        }
    }
    releases.sort_unstable();

    let find = |name: &str| {
        dir.entries
            .iter()
            .find(|e| e.name == name && !e.unsafe_name)
    };
    let manifest = find("META-INF/MANIFEST.MF")
        .and_then(|e| text_of(r, dir, e))
        .map(|t| meta::manifest(&t))
        .unwrap_or_default();

    // Maven's files: every `pom.properties` (a shaded JAR holds its dependencies' too), and the `pom.xml` beside the
    // archive's own for its name and description.
    let (stem, _) = meta::name_version(file_name);
    let mut maven: Vec<Maven> = Vec::new();
    let poms: Vec<&Entry> = order
        .iter()
        .map(|&i| &dir.entries[i])
        .filter(|e| {
            !e.unsafe_name
                && e.name.contains("META-INF/maven/")
                && e.name.ends_with("/pom.properties")
        })
        .take(64)
        .collect();
    for e in poms {
        let folder = &e.name[..e.name.len() - "pom.properties".len()];
        if let Some(mut m) = text_of(r, dir, e).and_then(|t| meta::pom_properties(&t, folder)) {
            if m.artifact == stem || maven.is_empty() {
                let xml = format!("{folder}pom.xml");
                if let Some(x) = find(&xml)
                    .and_then(|x| text_of(r, dir, x))
                    .and_then(|t| meta::pom_xml(&t, folder))
                {
                    m.name = x.name;
                    m.description = x.description;
                    m.packaging = x.packaging;
                }
            }
            maven.push(m);
        }
    }
    if let Some(own) = maven.iter().position(|m| m.artifact == stem) {
        let m = maven.remove(own);
        maven.insert(0, m);
    }

    // The release: the newest class version among the first classes outside `META-INF/versions/`.
    let mut newest: Option<(u16, u16)> = None;
    for e in order
        .iter()
        .map(|&i| &dir.entries[i])
        .filter(|e| {
            e.name.ends_with(".class")
                && !e.name.starts_with("META-INF/")
                && e.readable()
                && !e.unsafe_name
        })
        .take(RELEASE_SAMPLE)
    {
        if let Ok(head) = zip::read_head(r, dir, e, 8)
            && let Some(v) = class::version(&head)
            && newest.is_none_or(|n| v.0 > n.0)
        {
            newest = Some(v);
        }
    }

    let lower = file_name.to_ascii_lowercase();
    let war = lower.ends_with(".war") || dir.entries.iter().any(|e| e.name.starts_with("WEB-INF/"));
    let kind = match (boot, war) {
        (true, true) => "boot-war",
        (true, false) => "boot-jar",
        (false, true) => "war",
        _ if lower.ends_with(".ear") => "ear",
        _ if lower.ends_with(".aar") => "aar",
        _ => "jar",
    };

    let mut warnings = Vec::new();
    if unsafe_count > 0 {
        warnings.push(format!("{unsafe_count} entr{} with a name that leaves the archive (an absolute path or `..`): listed, not opened", if unsafe_count == 1 { "y" } else { "ies" }));
    }
    if unreadable > 0 {
        warnings.push(format!(
            "{unreadable} entr{} encrypted or packed in a way knobas does not read",
            if unreadable == 1 { "y" } else { "ies" }
        ));
    }
    if duplicates > 0 {
        warnings.push(format!(
            "{duplicates} name{} listed twice; the first is opened",
            if duplicates == 1 { "" } else { "s" }
        ));
    }

    ArchiveIndex {
        handle: String::new(),
        kind: kind.to_owned(),
        size: dir.len,
        total,
        packed,
        names,
        sizes,
        packed_sizes,
        crcs,
        flags,
        manifest,
        maven,
        release: newest.map(|(major, minor)| class::release(major, minor)),
        releases,
        libs,
        prefix: dir.prefix,
        warnings,
    }
}

/// The entry named `name`, the first where two share it.
#[must_use]
pub fn entry<'a>(dir: &'a Directory, name: &str) -> Option<&'a Entry> {
    dir.entries.iter().find(|e| e.name == name)
}

/// The most a nested archive packed (deflated) inside another may unpack to; one stored is read in place, any size.
pub const NESTED_CAP: u64 = 512 * 1024 * 1024;

/// Where a nested archive's bytes are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inner {
    /// Stored: its bytes sit in the outer archive's file, `len` of them at `base`, and are read in place.
    InPlace { base: u64, len: u64 },
    /// Deflated: unpacked into a file of its own first ([`zip::copy_entry`], at most [`NESTED_CAP`]).
    Packed,
}

/// Where the archive at `entry` inside `dir` is.
///
/// # Errors
///
/// As [`zip::check`] at [`NESTED_CAP`], and [`ZipError::Broken`] where its local header is not where it says.
pub fn inner<R: Read + Seek + ?Sized>(
    r: &mut R,
    dir: &Directory,
    entry: &Entry,
) -> Result<Inner, ZipError> {
    zip::check(entry, NESTED_CAP)?;
    if entry.method == 0 {
        let start = zip::data_start(r, dir, entry)?;
        Ok(Inner::InPlace {
            base: dir.base + start,
            len: entry.csize,
        })
    } else {
        Ok(Inner::Packed)
    }
}
