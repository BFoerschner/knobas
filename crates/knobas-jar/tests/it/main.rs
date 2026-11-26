//! Real-shaped archives, written here by a small zip writer around real class files (`tests/fixtures/`, compiled by
//! `javac` 27 from `tests/fixtures/src/`: `--release 17 -parameters` for `com.example.demo`, `--release 8` for
//! `Legacy`): a plain JAR, a Spring Boot fat JAR, a WAR, a multi-release JAR, and corrupt and hostile ones.

use std::io::{Cursor, Write};

use knobas_jar::{Inner, ZipError, class, flag, index, inner, zip};

const FIX: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/");

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{FIX}{name}")).expect(name)
}

/// One entry for [`write_zip`]: a name, its bytes, and whether it is deflated.
struct Item {
    name: String,
    bytes: Vec<u8>,
    deflate: bool,
}

fn item(name: &str, bytes: impl Into<Vec<u8>>, deflate: bool) -> Item {
    Item {
        name: name.to_owned(),
        bytes: bytes.into(),
        deflate,
    }
}

#[derive(Default)]
struct Tweak {
    /// Bytes before the archive (a launch script), the offsets not counting them.
    prefix: Vec<u8>,
    /// Lie about the first entry's unpacked size.
    size_of_first: Option<u32>,
    /// Lie about the first entry's CRC.
    crc_of_first: Option<u32>,
}

fn write_zip(items: &[Item], tweak: &Tweak) -> Vec<u8> {
    let mut out = tweak.prefix.clone();
    let start = out.len();
    let mut central = Vec::new();
    for (i, it) in items.iter().enumerate() {
        let packed = if it.deflate {
            let mut e =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
            e.write_all(&it.bytes).unwrap();
            e.finish().unwrap()
        } else {
            it.bytes.clone()
        };
        let mut crc = crc32fast::hash(&it.bytes);
        let mut size = it.bytes.len() as u32;
        if i == 0 {
            crc = tweak.crc_of_first.unwrap_or(crc);
            size = tweak.size_of_first.unwrap_or(size);
        }
        let method: u16 = if it.deflate { 8 } else { 0 };
        let local = (out.len() - start) as u32;
        let name = it.name.as_bytes();
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0x0800u16.to_le_bytes());
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name);
        out.extend_from_slice(&packed);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&0x0800u16.to_le_bytes());
        central.extend_from_slice(&method.to_le_bytes());
        central.extend_from_slice(&[0; 4]);
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&[0; 8]);
        central.extend_from_slice(&[0; 4]);
        central.extend_from_slice(&local.to_le_bytes());
        central.extend_from_slice(name);
    }
    let cd_offset = (out.len() - start) as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(items.len() as u16).to_le_bytes());
    out.extend_from_slice(&(items.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

fn open(bytes: &[u8]) -> Result<(Cursor<&[u8]>, zip::Directory), ZipError> {
    let mut c = Cursor::new(bytes);
    let dir = zip::read_directory(&mut c, 0, bytes.len() as u64)?;
    Ok((c, dir))
}

const MANIFEST: &str = "Manifest-Version: 1.0\r\nCreated-By: Maven JAR Plugin 3.3.0\r\nBuild-Jdk-Spec: 17\r\nBuilt-By: mara\r\nImplementation-Title: demo\r\nImplementation-Version: 1.4.2\r\nMain-Class: com.example.demo.App\r\nClass-Path: lib/slf4j-api-2.0.9.jar lib/commons-lang3-3.13.0\r\n .jar\r\n\r\n";

fn plain_jar() -> Vec<u8> {
    write_zip(
        &[
            item("META-INF/MANIFEST.MF", MANIFEST, true),
            item(
                "META-INF/maven/com.example/demo/pom.properties",
                "artifactId=demo\ngroupId=com.example\nversion=1.4.2\n",
                true,
            ),
            item(
                "META-INF/maven/com.example/demo/pom.xml",
                "<project><groupId>com.example</groupId><artifactId>demo</artifactId><version>1.4.2</version><name>Demo shelf</name></project>",
                true,
            ),
            item(
                "META-INF/maven/org.shaded/inside/pom.properties",
                "groupId=org.shaded\nartifactId=inside\nversion=0.9\n",
                true,
            ),
            item("com/example/demo/App.class", fixture("App.class"), true),
            item("com/example/demo/Shelf.class", fixture("Shelf.class"), true),
            item(
                "com/example/demo/Shelf$Book.class",
                fixture("Shelf$Book.class"),
                true,
            ),
            item("module-info.class", fixture("module-info.class"), true),
            item("application.yml", "server:\n  port: 8080\n", true),
            item(
                "db/migration/V1__init.sql",
                "create table book (id int);\n",
                false,
            ),
            item(
                "logo.png",
                vec![0x89, b'P', b'N', b'G', 13, 10, 26, 10],
                false,
            ),
        ],
        &Tweak::default(),
    )
}

#[test]
fn a_plain_jar_lists_its_entries_manifest_maven_and_release() {
    let jar = plain_jar();
    let (mut c, dir) = open(&jar).unwrap();
    let ix = index(&mut c, &dir, "demo-1.4.2.jar");
    assert_eq!(ix.kind, "jar");
    let names: Vec<&str> = ix.names.split('\n').collect();
    assert_eq!(names.len(), 11);
    // Sorted by name, folders not listed.
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted);
    assert_eq!(ix.sizes.len(), 11);
    assert_eq!(ix.total, ix.sizes.iter().sum::<u64>());
    assert!(ix.packed < ix.total, "deflated entries pack smaller");
    let sql = names.iter().position(|n| n.ends_with(".sql")).unwrap();
    assert_eq!(ix.flags[sql] & flag::STORED, flag::STORED);
    assert!(
        ix.manifest
            .contains(&("Main-Class".into(), "com.example.demo.App".into()))
    );
    assert!(ix.manifest.contains(&(
        "Class-Path".into(),
        "lib/slf4j-api-2.0.9.jar lib/commons-lang3-3.13.0.jar".into()
    )));
    assert_eq!(ix.maven.len(), 2);
    assert_eq!(
        ix.maven[0].artifact, "demo",
        "the archive's own coordinates first"
    );
    assert_eq!(ix.maven[0].name.as_deref(), Some("Demo shelf"));
    assert_eq!(ix.maven[1].group, "org.shaded");
    assert_eq!(ix.release.as_deref(), Some("Java 17"));
    assert!(ix.libs.is_empty());
    assert!(ix.warnings.is_empty());
    assert_eq!(ix.prefix, 0);
}

#[test]
fn an_entry_reads_whole_and_checked() {
    let jar = plain_jar();
    let (mut c, dir) = open(&jar).unwrap();
    let yml = knobas_jar::entry(&dir, "application.yml").unwrap();
    assert_eq!(
        zip::read_entry(&mut c, &dir, yml, 1024).unwrap(),
        b"server:\n  port: 8080\n"
    );
    let sql = knobas_jar::entry(&dir, "db/migration/V1__init.sql").unwrap();
    assert_eq!(
        zip::read_entry(&mut c, &dir, sql, 1024).unwrap(),
        b"create table book (id int);\n"
    );
    // Past the cap: refused with its size, never cut.
    assert!(matches!(
        zip::read_entry(&mut c, &dir, yml, 4),
        Err(ZipError::TooLarge {
            size: 21,
            cap: 4,
            ..
        })
    ));
}

fn boot_jar() -> Vec<u8> {
    let lib_a = write_zip(
        &[
            item(
                "META-INF/MANIFEST.MF",
                "Manifest-Version: 1.0\r\nImplementation-Title: spring-core\r\n\r\n",
                true,
            ),
            item(
                "org/springframework/core/Thing.class",
                fixture("Legacy.class"),
                true,
            ),
        ],
        &Tweak::default(),
    );
    let lib_b = write_zip(
        &[item(
            "com/fasterxml/Jackson.class",
            fixture("App.class"),
            true,
        )],
        &Tweak::default(),
    );
    write_zip(
        &[
            item(
                "META-INF/MANIFEST.MF",
                "Manifest-Version: 1.0\r\nMain-Class: org.springframework.boot.loader.launch.JarLauncher\r\nStart-Class: com.example.demo.App\r\nSpring-Boot-Version: 3.2.0\r\nSpring-Boot-Classes: BOOT-INF/classes/\r\nSpring-Boot-Lib: BOOT-INF/lib/\r\n\r\n",
                true,
            ),
            item(
                "org/springframework/boot/loader/launch/JarLauncher.class",
                fixture("Legacy.class"),
                true,
            ),
            item(
                "BOOT-INF/classes/com/example/demo/App.class",
                fixture("App.class"),
                true,
            ),
            item(
                "BOOT-INF/classes/application.properties",
                "spring.application.name=demo\n",
                true,
            ),
            // Spring Boot stores its nested JARs, so they are read in place.
            item("BOOT-INF/lib/spring-core-6.1.2.jar", lib_a, false),
            item("BOOT-INF/lib/jackson-databind-2.15.3.jar", lib_b, false),
            item(
                "BOOT-INF/classpath.idx",
                "- \"BOOT-INF/lib/spring-core-6.1.2.jar\"\n",
                true,
            ),
        ],
        &Tweak {
            prefix:
                b"#!/bin/bash\n# a launch script before the archive\nexec java -jar \"$0\" \"$@\"\n"
                    .to_vec(),
            ..Tweak::default()
        },
    )
}

#[test]
fn a_spring_boot_fat_jar_lists_its_libraries_and_reads_them_in_place() {
    let jar = boot_jar();
    let (mut c, dir) = open(&jar).unwrap();
    assert!(dir.prefix > 0, "the launch script is a prefix");
    let ix = index(&mut c, &dir, "demo-0.0.1-SNAPSHOT.jar");
    assert_eq!(ix.kind, "boot-jar");
    let libs: Vec<(&str, Option<&str>)> = ix
        .libs
        .iter()
        .map(|l| (l.name.as_str(), l.version.as_deref()))
        .collect();
    assert_eq!(
        libs,
        vec![
            ("jackson-databind", Some("2.15.3")),
            ("spring-core", Some("6.1.2"))
        ]
    );
    assert!(
        ix.manifest
            .iter()
            .any(|(k, v)| k == "Start-Class" && v == "com.example.demo.App")
    );

    let core = knobas_jar::entry(&dir, "BOOT-INF/lib/spring-core-6.1.2.jar").unwrap();
    let Inner::InPlace { base, len } = inner(&mut c, &dir, core).unwrap() else {
        panic!("a stored JAR is read in place")
    };
    let nested = zip::read_directory(&mut c, base, len).unwrap();
    let nix = index(&mut c, &nested, "spring-core-6.1.2.jar");
    assert_eq!(
        nix.names,
        "META-INF/MANIFEST.MF\norg/springframework/core/Thing.class"
    );
    assert_eq!(nix.release.as_deref(), Some("Java 8"));
    let thing = knobas_jar::entry(&nested, "org/springframework/core/Thing.class").unwrap();
    let bytes = zip::read_entry(&mut c, &nested, thing, 1 << 20).unwrap();
    assert_eq!(
        class::outline(&bytes).unwrap().name,
        "com.example.old.Legacy"
    );
}

#[test]
fn a_war_packs_its_libraries_which_unpack_first() {
    let lib = write_zip(
        &[item("x/Y.class", fixture("App.class"), true)],
        &Tweak::default(),
    );
    let war = write_zip(
        &[
            item("WEB-INF/web.xml", "<web-app/>", true),
            item(
                "WEB-INF/classes/com/example/demo/App.class",
                fixture("App.class"),
                true,
            ),
            item("WEB-INF/lib/commons-lang3-3.13.0.jar", lib.clone(), true),
            item("index.html", "<h1>hi</h1>", true),
        ],
        &Tweak::default(),
    );
    let (mut c, dir) = open(&war).unwrap();
    let ix = index(&mut c, &dir, "shop.war");
    assert_eq!(ix.kind, "war");
    assert_eq!(ix.libs[0].name, "commons-lang3");
    let e = knobas_jar::entry(&dir, "WEB-INF/lib/commons-lang3-3.13.0.jar").unwrap();
    assert_eq!(inner(&mut c, &dir, e).unwrap(), Inner::Packed);
    let mut out = Vec::new();
    zip::copy_entry(&mut c, &dir, e, knobas_jar::NESTED_CAP, &mut out).unwrap();
    assert_eq!(out, lib);
    let (mut c2, d2) = open(&out).unwrap();
    assert_eq!(
        index(&mut c2, &d2, "commons-lang3-3.13.0.jar").names,
        "x/Y.class"
    );
}

#[test]
fn a_multi_release_jar_names_its_releases() {
    let jar = write_zip(
        &[
            item(
                "META-INF/MANIFEST.MF",
                "Manifest-Version: 1.0\r\nMulti-Release: true\r\n\r\n",
                true,
            ),
            item(
                "com/example/old/Legacy.class",
                fixture("Legacy.class"),
                true,
            ),
            item(
                "META-INF/versions/17/com/example/old/Legacy.class",
                fixture("App.class"),
                true,
            ),
            item(
                "META-INF/versions/11/com/example/old/Legacy.class",
                fixture("Legacy.class"),
                true,
            ),
            item(
                "META-INF/versions/17/module-info.class",
                fixture("module-info.class"),
                true,
            ),
        ],
        &Tweak::default(),
    );
    let (mut c, dir) = open(&jar).unwrap();
    let ix = index(&mut c, &dir, "multi.jar");
    assert_eq!(ix.releases, vec![11, 17]);
    // The base release is the classes' outside `META-INF/versions/`.
    assert_eq!(ix.release.as_deref(), Some("Java 8"));
}

#[test]
fn a_corrupt_archive_says_what_is_wrong() {
    let jar = plain_jar();
    // Cut: no end record.
    assert!(matches!(
        open(&jar[..jar.len() - 30]),
        Err(ZipError::NotZip | ZipError::Broken(_))
    ));
    // A wrong CRC.
    let bad = write_zip(
        &[item("a.txt", "hello hello hello", true)],
        &Tweak {
            crc_of_first: Some(1),
            ..Tweak::default()
        },
    );
    let (mut c, dir) = open(&bad).unwrap();
    assert!(matches!(
        zip::read_entry(&mut c, &dir, &dir.entries[0], 1024),
        Err(ZipError::Crc(_))
    ));
    // An entry that inflates past the size it declares is a bomb, stopped as it comes.
    let lying = write_zip(
        &[item("a.txt", vec![b'a'; 100_000], true)],
        &Tweak {
            size_of_first: Some(10),
            ..Tweak::default()
        },
    );
    let (mut c, dir) = open(&lying).unwrap();
    assert!(matches!(
        zip::read_entry(&mut c, &dir, &dir.entries[0], 1 << 20),
        Err(ZipError::Bomb(_))
    ));
    // An entry declared huge against its packed size is refused before inflating.
    let huge = write_zip(
        &[item("big.bin", vec![0u8; 1000], true)],
        &Tweak {
            size_of_first: Some(3_000_000_000),
            ..Tweak::default()
        },
    );
    let (mut c, dir) = open(&huge).unwrap();
    assert!(matches!(
        zip::read_entry(&mut c, &dir, &dir.entries[0], u64::MAX),
        Err(ZipError::Bomb(_))
    ));
    // Bytes that are no archive.
    assert!(matches!(
        open(b"just some text, no archive, padded to be long enough"),
        Err(ZipError::NotZip)
    ));
    // Garbage where the local header should be.
    let mut smashed = plain_jar();
    smashed[0] = b'X';
    let (mut c, dir) = open(&smashed).unwrap();
    let first = dir.entries.iter().find(|e| e.local == 0).unwrap();
    assert!(matches!(
        zip::read_entry(&mut c, &dir, first, 1 << 20),
        Err(ZipError::Broken(_))
    ));
}

#[test]
fn an_archive_that_points_or_measures_wrong_is_refused() {
    // An entry whose local header is said to be past the central directory.
    let mut far = write_zip(&[item("a.txt", "hello", false)], &Tweak::default());
    let cd = far
        .windows(4)
        .position(|w| w == [0x50, 0x4b, 0x01, 0x02])
        .unwrap();
    far[cd + 42..cd + 46].copy_from_slice(&5000u32.to_le_bytes());
    assert!(matches!(open(&far), Err(ZipError::Broken(m)) if m.contains("points outside")));
    // A stored entry whose two sizes differ.
    let stored = write_zip(
        &[item("a.txt", "hello", false)],
        &Tweak {
            size_of_first: Some(9),
            ..Tweak::default()
        },
    );
    let (mut c, dir) = open(&stored).unwrap();
    assert!(
        matches!(zip::read_entry(&mut c, &dir, &dir.entries[0], 1024), Err(ZipError::Broken(m)) if m.contains("stored"))
    );
    // A deflated entry shorter than it says, its CRC its own bytes'.
    let short = write_zip(
        &[item("a.txt", "hello hello", true)],
        &Tweak {
            size_of_first: Some(40),
            ..Tweak::default()
        },
    );
    let (mut c, dir) = open(&short).unwrap();
    assert!(
        matches!(zip::read_entry(&mut c, &dir, &dir.entries[0], 1024), Err(ZipError::Broken(m)) if m.contains("shorter"))
    );
}

#[test]
fn path_tricks_are_listed_and_never_opened() {
    let jar = write_zip(
        &[
            item("../../etc/passwd", "root", false),
            item("/abs.txt", "x", false),
            item("ok.txt", "fine", false),
            item("ok.txt", "again", false),
        ],
        &Tweak::default(),
    );
    let (mut c, dir) = open(&jar).unwrap();
    let ix = index(&mut c, &dir, "tricks.jar");
    let names: Vec<&str> = ix.names.split('\n').collect();
    let at = names.iter().position(|n| *n == "../../etc/passwd").unwrap();
    assert_eq!(ix.flags[at] & flag::UNSAFE, flag::UNSAFE);
    assert_eq!(
        ix.flags
            .iter()
            .filter(|f| **f & flag::DUPLICATE != 0)
            .count(),
        1
    );
    assert_eq!(ix.warnings.len(), 2, "{:?}", ix.warnings);
    let bad = dir
        .entries
        .iter()
        .find(|e| e.name.starts_with(".."))
        .unwrap();
    assert!(matches!(
        zip::read_entry(&mut c, &dir, bad, 1024),
        Err(ZipError::UnsafeName(_))
    ));
    assert_eq!(
        zip::read_entry(
            &mut c,
            &dir,
            knobas_jar::entry(&dir, "ok.txt").unwrap(),
            1024
        )
        .unwrap(),
        b"fine"
    );
}

#[test]
fn ten_thousand_entries_list_quickly() {
    let items: Vec<Item> = (0..10_000)
        .map(|i| {
            item(
                &format!("com/example/p{}/C{i}.class", i % 97),
                fixture("App.class"),
                false,
            )
        })
        .collect();
    let jar = write_zip(&items, &Tweak::default());
    let started = std::time::Instant::now();
    let (mut c, dir) = open(&jar).unwrap();
    let ix = index(&mut c, &dir, "big.jar");
    assert_eq!(ix.sizes.len(), 10_000);
    // Generous for a debug build on a loaded machine; the release build is a few milliseconds.
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
}

fn outline(name: &str) -> class::ClassOutline {
    class::outline(&fixture(name)).unwrap()
}

#[test]
fn a_class_outlines_as_javap_would() {
    let o = outline("Shelf.class");
    assert_eq!(o.release, "Java 17");
    assert_eq!(o.kind, "class");
    assert_eq!(o.modifiers, "public abstract");
    assert_eq!(o.name, "com.example.demo.Shelf");
    assert_eq!(o.type_params, "<T extends Comparable<T>>");
    assert_eq!(o.extends, None);
    assert_eq!(o.implements, vec!["Iterable<T>", "java.io.Serializable"]);
    assert!(o.deprecated);
    assert!(
        o.annotations
            .iter()
            .any(|a| a == "@Deprecated(since = \"2.0\", forRemoval = true)"),
        "{:?}",
        o.annotations
    );
    assert_eq!(o.source_file.as_deref(), Some("Shelf.java"));

    let field = |n: &str| o.fields.iter().find(|f| f.name == n).unwrap();
    assert_eq!(field("CAPACITY").value.as_deref(), Some("42"));
    assert_eq!(field("CAPACITY").modifiers, "public static final");
    assert_eq!(
        field("LABEL").value.as_deref(),
        Some("\"front \\\"row\\\"\"")
    );
    assert_eq!(
        field("byTag").ty,
        "java.util.Map<String, java.util.List<? extends T>>"
    );
    assert_eq!(field("byTag").modifiers, "protected transient volatile");
    assert_eq!(field("counts").ty, "long[]");
    // A long and a double take two slots of the constant pool each: read wrong, every index after them is off.
    assert_eq!(field("BIG").value.as_deref(), Some("1099511627776L"));
    assert_eq!(field("RATIO").value.as_deref(), Some("0.5"));

    let method = |n: &str| o.methods.iter().find(|m| m.name == n).unwrap();
    let find = method("find");
    assert_eq!(find.type_params, "<R extends T>");
    assert_eq!(find.ty, "java.util.List<R>");
    assert_eq!(find.params.as_deref(), Some("String query, int limit"));
    assert_eq!(
        find.throws,
        vec!["java.io.IOException", "InterruptedException"]
    );
    assert_eq!(find.modifiers, "public abstract");
    assert_eq!(method("sum").params.as_deref(), Some("int... values"));
    assert_eq!(method("sum").modifiers, "public static synchronized");
    let ctor = method("Shelf");
    assert_eq!(ctor.ty, "");
    assert_eq!(ctor.params.as_deref(), Some("String name, int size"));
    // The lambda's body is the compiler's.
    assert!(o.methods.iter().all(|m| !m.name.starts_with("lambda$")));
    assert!(o.synthetic >= 1);
}

#[test]
fn records_enums_sealed_interfaces_annotations_and_modules() {
    let book = outline("Shelf$Book.class");
    assert_eq!(book.kind, "record");
    assert_eq!(book.name, "com.example.demo.Shelf$Book");
    assert_eq!(
        book.modifiers, "public",
        "a nested record is static and final without saying so"
    );
    assert_eq!(book.components, vec!["String title", "int pages"]);
    // `@Deprecated` cannot target a component, so javac puts it on the accessor.
    assert!(
        book.methods
            .iter()
            .find(|m| m.name == "pages")
            .unwrap()
            .deprecated
    );
    assert_eq!(
        book.implements,
        vec!["Comparable<com.example.demo.Shelf.Book>"]
    );
    assert!(
        book.fields.is_empty(),
        "a record's fields are its components"
    );

    let colour = outline("Shelf$Colour.class");
    assert_eq!(colour.kind, "enum");
    let constants: Vec<&str> = colour
        .fields
        .iter()
        .filter(|f| f.constant)
        .map(|f| f.name.as_str())
        .collect();
    assert_eq!(constants, vec!["RED", "GREEN"]);
    assert!(
        colour
            .methods
            .iter()
            .all(|m| m.name != "values" && m.name != "valueOf")
    );
    assert!(colour.methods.iter().any(|m| m.name == "lower"));

    let shape = outline("Shelf$Shape.class");
    assert_eq!(shape.kind, "interface");
    assert_eq!(shape.modifiers, "public sealed");
    assert_eq!(
        shape.permits,
        vec![
            "com.example.demo.Shelf.Circle",
            "com.example.demo.Shelf.Square"
        ]
    );
    let m = |n: &str| {
        shape
            .methods
            .iter()
            .find(|x| x.name == n)
            .unwrap()
            .modifiers
            .clone()
    };
    assert_eq!(m("area"), "");
    assert_eq!(m("kind"), "default");
    assert_eq!(m("unit"), "static");

    // A generic inner class's constructor: its signature leaves out the outer instance `MethodParameters` names.
    let holder = outline("Shelf$Holder.class");
    assert_eq!(holder.methods[0].params.as_deref(), Some("V value, int n"));

    let circle = outline("Shelf$Circle.class");
    assert_eq!(circle.modifiers, "public static final");
    assert_eq!(
        circle
            .methods
            .iter()
            .find(|x| x.name == "Circle")
            .unwrap()
            .params
            .as_deref(),
        Some("double r")
    );

    let tag = outline("Shelf$Tag.class");
    assert_eq!(tag.kind, "@interface");
    assert!(tag.implements.is_empty());

    let module = outline("module-info.class");
    assert_eq!(module.kind, "module");
    assert_eq!(module.name, "com.example.demo");
    assert!(
        module
            .module
            .contains(&"requires transitive java.logging;".to_owned()),
        "{:?}",
        module.module
    );
    assert!(
        module
            .module
            .contains(&"requires static java.sql;".to_owned())
    );
    assert!(
        module
            .module
            .contains(&"exports com.example.demo;".to_owned())
    );
    assert!(
        module
            .module
            .contains(&"opens com.example.demo to java.logging;".to_owned())
    );
    assert!(module.module.contains(&"uses Runnable;".to_owned()));

    let legacy = outline("Legacy.class");
    assert_eq!(legacy.release, "Java 8");
    assert_eq!(
        legacy
            .methods
            .iter()
            .find(|x| x.name == "get")
            .unwrap()
            .params
            .as_deref(),
        Some("int"),
        "no names without -parameters"
    );
}

#[test]
fn every_cut_of_a_class_is_an_error_never_a_panic() {
    let bytes = fixture("Shelf.class");
    for cut in 0..bytes.len() {
        assert!(class::outline(&bytes[..cut]).is_err(), "cut at {cut}");
    }
    // Every single byte flipped: an answer or an error, never a panic or a hang.
    for at in 8..bytes.len() {
        let mut b = bytes.clone();
        b[at] ^= 0xFF;
        let _ = class::outline(&b);
    }
}
