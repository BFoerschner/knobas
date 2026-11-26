//! **A schema at an address, downloaded on a press** (the XSD design,
//! 2026-11-23, section 17): one `GET` with no cookie and no credential, kept
//! in the profile's `schemas/` under its SHA-256, read back and forgotten; an
//! answer that is no schema refused with what it was; a redirect to another
//! origin not followed.

use knobas_app::api::schemas;
use sha2::{Digest, Sha256};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const XSD: &str = r#"<?xml version="1.0"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:a"><xs:element name="a"/></xs:schema>
"#;

#[tokio::test]
async fn one_get_kept_under_its_sha256_read_back_and_forgotten() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/a.xsd"))
        .respond_with(ResponseTemplate::new(200).set_body_string(XSD))
        .expect(1)
        .mount(&server)
        .await;
    let profile = tempfile::tempdir().expect("a profile");
    let url = format!("{}/a.xsd", server.uri());
    let got = schemas::download(profile.path(), &url)
        .await
        .expect("downloaded");
    let sha = format!("{:x}", Sha256::digest(XSD.as_bytes()));
    assert_eq!(
        (got.url.as_str(), got.sha256.as_str()),
        (url.as_str(), sha.as_str())
    );
    assert_eq!(got.size, XSD.len() as u64);
    assert_eq!(got.at.len(), 10);
    let kept = profile.path().join("schemas").join(format!("{sha}.xsd"));
    assert_eq!(std::fs::read_to_string(&kept).expect("kept"), XSD);
    // No half-written file is left beside it.
    assert_eq!(
        std::fs::read_dir(profile.path().join("schemas"))
            .expect("the folder")
            .count(),
        1
    );
    let asked: Vec<Request> = server.received_requests().await.expect("recorded");
    for name in ["cookie", "authorization"] {
        assert!(!asked[0].headers.contains_key(name), "{name} was sent");
    }
    assert_eq!(
        schemas::downloaded(profile.path(), &sha)
            .await
            .expect("read"),
        Some(XSD.to_owned())
    );
    schemas::forget_download(profile.path(), &sha)
        .await
        .expect("forgotten");
    assert!(!kept.exists());
    assert_eq!(
        schemas::downloaded(profile.path(), &sha)
            .await
            .expect("read"),
        None
    );
    // Forgetting what is not kept is no error; a name that is no SHA-256 is.
    schemas::forget_download(profile.path(), &sha)
        .await
        .expect("nothing to forget");
    assert!(
        schemas::downloaded(profile.path(), "../secret")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn an_answer_that_is_no_schema_is_refused_by_name_and_nothing_is_kept() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("<!doctype html><html><body>sign in</body></html>")
                .insert_header("content-type", "text/html"),
        )
        .mount(&server)
        .await;
    let profile = tempfile::tempdir().expect("a profile");
    let error = schemas::download(profile.path(), &format!("{}/b.xsd", server.uri()))
        .await
        .expect_err("no schema");
    assert!(error.message.contains("an HTML page"), "{}", error.message);
    assert!(!profile.path().join("schemas").exists());
}

#[tokio::test]
async fn a_redirect_to_another_origin_is_not_followed_and_a_missing_file_says_its_status() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/c.xsd"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("location", "https://elsewhere.example/c.xsd"),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/d.xsd"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let profile = tempfile::tempdir().expect("a profile");
    let error = schemas::download(profile.path(), &format!("{}/c.xsd", server.uri()))
        .await
        .expect_err("not followed");
    assert!(
        error.message.contains("elsewhere.example"),
        "{}",
        error.message
    );
    assert!(
        error.message.contains("the schema's own site"),
        "{}",
        error.message
    );
    let error = schemas::download(profile.path(), &format!("{}/d.xsd", server.uri()))
        .await
        .expect_err("not found");
    assert!(error.message.contains("404"), "{}", error.message);
    let error = schemas::download(profile.path(), "file:///etc/hosts")
        .await
        .expect_err("refused");
    assert!(
        error.message.contains("http and https"),
        "{}",
        error.message
    );
}
