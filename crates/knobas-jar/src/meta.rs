//! What an archive says about itself: `META-INF/MANIFEST.MF`, Maven's `pom.properties` and `pom.xml`, and a nested
//! JAR's name and version from its file name.

use serde::Serialize;

/// The manifest's main section, in its order: `Main-Class`, `Implementation-Version`, `Class-Path`, all of it.
/// Continuation lines (a line starting with one space) are joined, as the JAR spec says.
#[must_use]
pub fn manifest(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() {
            // The main section ends at the first blank line; the rest are per-entry sections.
            if !out.is_empty() {
                break;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix(' ') {
            if let Some(last) = out.last_mut() {
                last.1.push_str(rest);
            }
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            out.push((
                key.trim().to_owned(),
                value
                    .strip_prefix(' ')
                    .unwrap_or(value)
                    .trim_end()
                    .to_owned(),
            ));
        }
        if out.len() >= 500 {
            break;
        }
    }
    out
}

/// One artifact's Maven coordinates, found inside the archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct Maven {
    pub group: String,
    pub artifact: String,
    pub version: String,
    /// `pom.xml`'s `<name>`, where it has one.
    pub name: Option<String>,
    pub description: Option<String>,
    pub packaging: Option<String>,
    /// The folder it was found in (`META-INF/maven/org.x/y/`).
    pub path: String,
}

/// `pom.properties`: `groupId`, `artifactId`, `version`.
#[must_use]
pub fn pom_properties(text: &str, path: &str) -> Option<Maven> {
    let mut group = None;
    let mut artifact = None;
    let mut version = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with('!') {
            continue;
        }
        let Some((key, value)) = line.split_once('=').or_else(|| line.split_once(':')) else {
            continue;
        };
        let value = value.trim().to_owned();
        match key.trim() {
            "groupId" => group = Some(value),
            "artifactId" => artifact = Some(value),
            "version" => version = Some(value),
            _ => {}
        }
    }
    Some(Maven {
        group: group?,
        artifact: artifact?,
        version: version.unwrap_or_default(),
        name: None,
        description: None,
        packaging: None,
        path: path.to_owned(),
    })
}

/// The text of `<tag>` among `<project>`'s own children, skipping `<parent>`, `<dependencies>` and every other
/// nested element. A tiny scan, not an XML parser: comments and CDATA are skipped, entities left as written.
fn top_level(xml: &str, wanted: &[&str]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut rest = xml;
    let mut open: Option<(String, usize)> = None;
    while let Some(lt) = rest.find('<') {
        let text_before = &rest[..lt];
        rest = &rest[lt..];
        if rest.starts_with("<!--") {
            rest = rest.find("-->").map_or("", |end| &rest[end + 3..]);
            continue;
        }
        if rest.starts_with("<?") || rest.starts_with("<!") {
            rest = rest.find('>').map_or("", |end| &rest[end + 1..]);
            continue;
        }
        let Some(gt) = rest.find('>') else { break };
        let tag = &rest[1..gt];
        rest = &rest[gt + 1..];
        if let Some(name) = tag.strip_prefix('/') {
            if let Some((open_name, at)) = &open
                && *at == depth
                && open_name == name.trim()
            {
                out.push((open_name.clone(), text_before.trim().to_owned()));
                open = None;
            }
            depth = depth.saturating_sub(1);
        } else if tag.ends_with('/') {
            // An empty element.
        } else {
            depth += 1;
            let name = tag.split_whitespace().next().unwrap_or("").to_owned();
            // `<project>` is depth 1, its children depth 2.
            if depth == 2 && wanted.contains(&name.as_str()) {
                open = Some((name, depth));
            }
        }
        if out.len() > 32 {
            break;
        }
    }
    out
}

/// `pom.xml`'s coordinates; a group or version left to `<parent>` is taken from it.
#[must_use]
pub fn pom_xml(xml: &str, path: &str) -> Option<Maven> {
    let own = top_level(
        xml,
        &[
            "groupId",
            "artifactId",
            "version",
            "name",
            "description",
            "packaging",
        ],
    );
    let get = |k: &str| {
        own.iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .filter(|v| !v.is_empty())
    };
    let parent = xml.find("<parent>").and_then(|at| {
        let end = xml[at..].find("</parent>")? + at;
        Some(&xml[at..end])
    });
    let from_parent = |k: &str| {
        let p = parent?;
        let open = format!("<{k}>");
        let at = p.find(&open)? + open.len();
        let end = p[at..].find('<')? + at;
        Some(p[at..end].trim().to_owned())
    };
    Some(Maven {
        group: get("groupId").or_else(|| from_parent("groupId"))?,
        artifact: get("artifactId")?,
        version: get("version")
            .or_else(|| from_parent("version"))
            .unwrap_or_default(),
        name: get("name"),
        description: get("description").map(|d| d.split_whitespace().collect::<Vec<_>>().join(" ")),
        packaging: get("packaging"),
        path: path.to_owned(),
    })
}

/// A JAR's name and version from its file name: `spring-core-6.1.2.jar` is `spring-core` at `6.1.2`; the version
/// starts at the first `-` followed by a digit. A name with no such `-` has no version.
#[must_use]
pub fn name_version(file: &str) -> (String, Option<String>) {
    let base = file.rsplit('/').next().unwrap_or(file);
    let stem = [".jar", ".war", ".ear", ".aar", ".zip"]
        .iter()
        .find_map(|ext| {
            base.strip_suffix(ext).or_else(|| {
                base.to_ascii_lowercase()
                    .ends_with(ext)
                    .then(|| &base[..base.len() - ext.len()])
            })
        })
        .unwrap_or(base);
    let bytes = stem.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'-' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit) && i > 0 {
            return (stem[..i].to_owned(), Some(stem[i + 1..].to_owned()));
        }
    }
    (stem.to_owned(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_joins_its_continuations_and_stops_at_the_main_section() {
        let text = "Manifest-Version: 1.0\r\nMain-Class: org.springframework.boot.loader.launch.JarLaun\r\n cher\r\nClass-Path: a.jar b\r\n .jar\r\n\r\nName: x/y\r\nSealed: true\r\n";
        assert_eq!(
            manifest(text),
            vec![
                ("Manifest-Version".into(), "1.0".into()),
                (
                    "Main-Class".into(),
                    "org.springframework.boot.loader.launch.JarLauncher".into()
                ),
                ("Class-Path".into(), "a.jar b.jar".into()),
            ]
        );
    }

    #[test]
    fn maven_coordinates_from_properties_and_xml() {
        let p = pom_properties(
            "#Generated by Maven\ngroupId=com.example\nartifactId=demo\nversion=1.2.3\n",
            "META-INF/maven/com.example/demo/",
        )
        .unwrap();
        assert_eq!(
            (p.group.as_str(), p.artifact.as_str(), p.version.as_str()),
            ("com.example", "demo", "1.2.3")
        );
        let xml = "<?xml version=\"1.0\"?>\n<!-- a <comment> -->\n<project><modelVersion>4.0.0</modelVersion><parent><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-parent</artifactId><version>3.2.0</version></parent><artifactId>demo</artifactId><name>Demo</name><description>A  demo\n app</description><dependencies><dependency><groupId>x</groupId><artifactId>y</artifactId><version>9</version></dependency></dependencies></project>";
        let m = pom_xml(xml, "p/").unwrap();
        assert_eq!(m.group, "org.springframework.boot");
        assert_eq!(m.artifact, "demo");
        assert_eq!(m.version, "3.2.0");
        assert_eq!(m.name.as_deref(), Some("Demo"));
        assert_eq!(m.description.as_deref(), Some("A demo app"));
        assert!(pom_xml("<project><groupId>g</groupId></project>", "").is_none());
    }

    #[test]
    fn a_jars_version_comes_from_its_name() {
        let cases = [
            (
                "BOOT-INF/lib/spring-core-6.1.2.jar",
                "spring-core",
                Some("6.1.2"),
            ),
            (
                "jackson-datatype-jsr310-2.15.3.jar",
                "jackson-datatype-jsr310",
                Some("2.15.3"),
            ),
            ("guava-32.1.3-jre.jar", "guava", Some("32.1.3-jre")),
            ("HdrHistogram-2.1.12.jar", "HdrHistogram", Some("2.1.12")),
            ("tools.jar", "tools", None),
            ("x-y.JAR", "x-y", None),
        ];
        for (file, name, version) in cases {
            assert_eq!(
                name_version(file),
                (name.to_owned(), version.map(str::to_owned)),
                "{file}"
            );
        }
    }
}
