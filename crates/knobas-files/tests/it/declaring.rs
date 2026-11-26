//! The schema files that declare a namespace (the XSD design, 2026-11-23,
//! section 16, step 6): what another repository answers by itself.

use super::{index, put};

const OPENTRANS: &str = r#"<?xml version="1.0"?>
<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns="http://www.opentrans.org/XMLSchema/2.1" targetNamespace="http://www.opentrans.org/XMLSchema/2.1">
  <xsd:import namespace="http://www.bmecat.org/bmecat/2005" schemaLocation="bmecat_2005.xsd"/>
</xsd:schema>
"#;
const BMECAT: &str = r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema" targetNamespace="http://www.bmecat.org/bmecat/2005"/>"#;

#[test]
fn the_files_that_declare_a_namespace_and_not_those_that_import_it() {
    let (_dir, index) = index();
    put(
        &index,
        &[
            (1, "schemas/opentrans/opentrans_2_1.xsd", Some(OPENTRANS)),
            (1, "schemas/opentrans/bmecat_2005.xsd", Some(BMECAT)),
            (2, "lib/bmecat.xsd", Some(BMECAT)),
            // Not a schema: its text names the namespace as a target too.
            (3, "notes/bmecat.md", Some(BMECAT)),
            // Nor in a folder named for schemas.
            (3, "xsd/bmecat-notes.md", Some(BMECAT)),
            // A schema whose text was not read is not found.
            (4, "bmecat_2005.xsd", None),
        ],
    );
    let found = index.declaring("http://www.bmecat.org/bmecat/2005", 50);
    let at: Vec<(u64, &str)> = found
        .files
        .iter()
        .map(|file| (file.no, file.path.as_str()))
        .collect();
    // The shortest path first; the openTRANS file that imports it is not one.
    assert_eq!(
        at,
        [
            (2, "lib/bmecat.xsd"),
            (1, "schemas/opentrans/bmecat_2005.xsd")
        ]
    );
    assert!(found.whole);
    assert_eq!(
        index
            .declaring("http://www.opentrans.org/XMLSchema/2.1", 50)
            .files
            .len(),
        1
    );
    assert!(index.declaring("", 50).files.is_empty());
    assert!(index.declaring("urn:nothing-here", 50).files.is_empty());
    // At most as many as asked.
    assert_eq!(
        index
            .declaring("http://www.bmecat.org/bmecat/2005", 1)
            .files
            .len(),
        1
    );
}

#[test]
fn past_the_read_bound_the_answer_says_it_is_not_whole() {
    let (_dir, index) = index();
    // More schemas that import BMEcat, and so hold its words, than are read to tell.
    let importing = r#"<xsd:schema targetNamespace="urn:other"><xsd:import namespace="http://www.bmecat.org/bmecat/2005"/></xsd:schema>"#;
    let paths: Vec<String> = (0..=knobas_files::DECLARING_READ)
        .map(|i| format!("a/{i}.xsd"))
        .collect();
    let files: Vec<(u64, &str, Option<&str>)> = paths
        .iter()
        .map(|path| (1, path.as_str(), Some(importing)))
        .collect();
    put(&index, &files);
    let found = index.declaring("http://www.bmecat.org/bmecat/2005", 50);
    assert!(found.files.is_empty());
    assert!(!found.whole);
}
