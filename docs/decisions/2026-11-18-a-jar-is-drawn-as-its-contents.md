# 2026-11-18: A Java archive is drawn as its contents (ux19)

**Asked (Björn, on PR #2479, through the orchestrator):** "Is it possible to display jar files with the render view in
a useful way", and yes to: the contents as a tree with sizes and a filter, resources opening with the normal viewers;
the manifest and Maven's files on top; classes outlined as `javap` would, without Java; nested JARs as a dependency
list with versions; decompiled source only where Java is installed, decided with Fable.

## What it is

A built-in renderer, **JAR** (`render/registry.ts`), for `*.jar`, `*.war`, `*.ear` and `*.aar`, on by default. A
binary file, so *Source* is not offered; the view offers *Save to Downloads* beside the page's *Open in…*.

- **The head:** the artifact's name (Maven's `artifactId`, else `Implementation-Title`, else the file's name) and its
  version, the version in the method column's cut (Archivo width 62, weight 800), the view's one loud thing; the kind
  (*JAR*, *Spring Boot JAR*, *WAR*, *Spring Boot WAR*, *EAR*, *Android library*) and `group:artifact`. Then the facts:
  the Java release the classes are compiled for (the newest class file version among the first 24 classes) and a
  multi-release JAR's releases; the application class (`Start-Class`) and the launcher, or the main class; the module;
  who built it with which JDK; Maven's description; its size, a launch script before it, its entries and their size
  unpacked. The whole manifest, its class path, other Maven coordinates found inside (a shaded JAR's) and what the
  reading passed over are folds.
- **The band**, stuck to the top of the page's scroller: *Filter entries* (every word against the entry's path, the
  open nested JARs' entries too), the count holding its room, and *Contents* / *Dependencies* where it holds JARs.
- **Contents:** the tree, a folder with only one folder in it drawn as one row with it (`com/tidewater/ledger/`), every
  row its size. An entry opens in place, once its read is in, so it draws whole: a class to its outline; a text
  resource (`.yml`, `.properties`, `.xml`, `.sql`, any text) to Source's numbered, highlighted lines in a box of its
  own; Markdown drawn; an image in a frame of fixed height; a PDF in the PDF viewer; other bytes named with *Save to
  Downloads*. A nested JAR opens to its own tree one level in.
- **Dependencies:** the nested JARs (`BOOT-INF/lib/`, `WEB-INF/lib/`, an EAR's modules) by name, their versions from
  their file names in a column (`spring-core-6.1.2.jar` is `spring-core` at `6.1.2`: the version starts at the first
  `-` before a digit), each opening to its tree in place.
- **A class's outline** (`tf/render/TfJarClass.svelte`): its release and source file, its annotations and declaration
  (`public abstract class Shelf<T extends Comparable<T>>`, a record's components in it), what it extends, implements
  and permits, then its enum constants, fields (a constant's value), constructors and methods with generic signatures,
  parameter names where the compiler kept them (`-parameters`) and what they throw; a module's requires, exports and
  opens. What the compiler made (bridges, lambdas' bodies, `values()`) is counted, not listed.

## The two decisions

**1. The archive and the class files are read by knobas's own code** (`crates/knobas-jar`, about 2,000 lines, no new
crate from outside the tree: `flate2` and `crc32fast` were in it). The `zip` crate (MIT) would bring its own
dependency tree and hides the two things this view needs: reading a nested JAR in place at an offset inside the outer
file, and checking every declared size and offset before believing it. The class-file crates (`cafebabe`, `classfile`,
`noak`, MIT or Apache-2.0) parse the pool and members but none writes generic signatures, annotation values and
`MethodParameters` the way source reads them; that writing is most of the work either way.

- **Read once, then an entry at a time.** `archive_open` reads the archive from the clone at the file's commit
  (ADR-0041), else from the source, past the 50 MiB byte cap up to 1 GiB and outside the read cache; lists it from its
  zip central directory (the end record, then the directory; zip64 read; a launch script before it allowed for) and
  answers the listing as columns (one string of names and three arrays), then writes the bytes to a spool file behind
  the answer and drops them from memory. What stays in memory per archive is its listing. Eight archives are kept, the
  oldest dropped with its nested ones and its file; an archive read at a whole sha opens again with no read.
- **A nested JAR** stored (Spring Boot's) is read in place inside the outer file; one deflated (a WAR's) is unpacked
  to a spool file first, at most 512 MiB.
- **Refused**, never cut: an entry whose name leaves the archive (absolute, a drive, `..`, a backslash, a control
  character: listed, marked, not opened), an encrypted one or one packed with another method, one over 50 MiB, one
  that declares over 1,000 times its packed size past 16 MiB (a zip bomb), one that inflates past what it declares
  (stopped as it comes), one whose CRC does not match, a stored one whose sizes differ, a record pointing outside the
  archive, more than 500,000 entries.

**2. *Decompile*: Vineflower, bundled, run with a Java found here** (Fable, xhigh; its answer as given, one point
changed as noted).

- **Vineflower 1.12.0** (Apache-2.0, 1.6 MB) ships as a resource beside the binary
  (`crates/knobas-app/decompiler/`, staged at build time by `scripts/fetch-vineflower.sh` and `release.yml` from Maven
  Central, pinned by SHA-256 in `vineflower.sha256`; knobas checks the pin again before running it). No configured
  decompiler, no download at run time: each is a second path to test. The cache key names the decompiler, so another
  one later needs no migration.
- **The Java floor is read from the jar**: its main class's class file version (61: Java 17). No Java: no button. A
  Java too old: *Decompile* disabled with the reason beside it (*Java 11 found; Decompile needs Java 17*).
- **Java is looked for once**: the path Settings names (`knobas.setting` `java_path`, `set_java_path`), `JAVA_HOME`,
  macOS's `/usr/libexec/java_home`, `PATH`, then the usual install folders (the app started from the Finder or the
  Start menu sees little of a shell's `PATH`). A `java` counts only where `java -version` exits 0 and names a version
  within two seconds, which turns away macOS's stub; `Picked up …` lines are skipped. Fable ruled the look at start;
  it runs on the first archive opened instead, behind its answer, so a session that opens none starts no JVM; the
  answer is in before a class can be opened.
- **Settings › Renderers › *Decompiler*** (`tf/settings/TfDecompilerSetting.svelte`): one line, *Java 21.0.4 at
  /usr/bin/java, with Vineflower 1.12.0 (bundled).*, or that none is found and the JAR view shows outlines, or why the
  one found cannot run it; *Look again*; and a Java of one's own (a `java` or a Java home), *Use this Java*, cleared to
  look by itself.
- **A run:** the class and its nested classes (`Outer$*.class`) written by base name into a fresh folder, Vineflower
  writing into another, `-Xmx512m -Djava.awt.headless=true`, `JAVA_TOOL_OPTIONS`, `_JAVA_OPTIONS` and
  `JDK_JAVA_OPTIONS` left out of its environment, no shell, its process group (Windows: its tree) killed at 20 s;
  8 MiB in and 2 MiB out at most, past either an error, never a cut. One JVM at a time: a new run stops the last. Only
  a plain file inside the output folder is read back, no link followed; both folders are removed after. An inner class
  decompiles as its top-level class.
- **Kept** on disk in the app's cache folder (`decompiled/`) by a SHA-256 over the classes' names and bytes, the
  decompiler and its options; 200 MB at most, the oldest dropped. A failure is not kept.
- **Drawn** as Source draws (`TfJarCode`), read only, Java highlighted, under *It is not the original source: names of
  locals and comments are gone*. Vineflower takes about 0.7 s cold for a class here; the pane holds its height while
  it works.

## How it stays light

- **Nothing at start-up.** The view, its CSS and its model are one chunk (29 KB, 10 KB gzipped; CSS 8 KB) loaded with
  the first archive opened. Start-up holds the registry's entry and the ports' few lines.
- **The listing is columns**, unpacked once into a tree in one pass over the sorted names; the rows read plain copies
  of what is open, taken once a pass.
- **Windowed:** one `Windowed` list of folders, entries, nested trees and opened entries; 10,000 entries draw what the
  screen shows. A text box windows its own lines.
- **Nothing moves once drawn:** an entry opens once its read is in; a nested JAR once it is listed; an image's frame and
  a text box's height are set before they draw; the count holds its room; the *Outline* / *Source* switch sits at the
  end of the class's head.

## Timings

**The view** (`testenv/perf/jar.mjs`: the built app under `?fake-ipc&fake-jar`, whose archive reads answer at once,
so this is the view's own cost), headless Chrome on Björn's PC (its WSL side, driven over a tunnel), the CPU
throttled, five runs a case, the median against the PC's lines (a page 100 / 200 / 300 ms, a press 100 / 100 / 200 ms).
In ms at 1x / 4x / 6x:

| Case | 1440 x 868 @ 1 | 2560 x 1440 @ 1.25 |
|---|---|---|
| A library JAR opened from the README | 30 / 47 / 62 | 31 / 50 / 64 |
| The 10,000-entry fat JAR opened from the README | 29 / 84 / 115 | 30 / 83 / 116 |
| The fat JAR, the page's first visit (from the Inbox) | 28 / 80 / 123 | 30 / 85 / 126 |
| A filter key on 10,000 entries | 15 / 40 / 65 | 13 / 56 / 80 |
| A folder of 9,000 entries opened | 11 / 15 / 16 | 12 / 19 / 15 |
| A class opened to its outline | 13 / 14 / 16 | 15 / 22 / 16 |
| *Dependencies* of the fat JAR (150 JARs) | 16 / 70 / 114 | 17 / 62 / 109 |
| A library opened in the dependency list | 7 / 28 / 41 | 17 / 29 / 45 |
| Scrolling the 9,800 entries a filter lists (fps, no task over 60 ms) | 61 / 61 / 61 | 61 / 61 / 61 |

Nothing the view drew moved after it drew, in any case. The Code page's other files open as before: `Cargo.toml` 29 /
29 / 43 at 1440 on the PC. The Markdown README's open reports a move on this build and on the base (`origin/ux-polish`
at `ecc33edd`), the same on both, on the Mac (base 39 / 35 / 53 ms, this 45 / 38 / 56, at its half-lines): the
README's second line growing that the OpenAPI note recorded, left to the app-wide movement audit.

**The backend** (`crates/knobas-jar/examples/big_jar.rs`, release, the Mac): a Spring Boot fat JAR of 209 MB and
9,801 entries (150 stored libraries of 1.3 MB) is listed in 3.4 ms (the directory, the manifest, the Maven files and the
release sample), 690 KB of listing across the bridge; a class read and outlined in 0.03 ms; a nested JAR listed in
place in 0.02 ms; the spool file written behind the answer in 73 ms. The read of the archive itself is the clone's
blob read or the forge's download, as for any file. *Decompile* with Vineflower 1.12.0 and the Mac's Java 27: 421 ms
cold for `Shelf` and its eight nested classes, 0.4 ms from the cache.
