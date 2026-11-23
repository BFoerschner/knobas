//! **An image at an address, loaded on a press** (the XML view's design,
//! 2026-11-23, section 14): one `GET`, with no cookie and no credential; an
//! answer that is no image refused with what it was; a redirect to another
//! origin not followed.

use knobas_app::api::image;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

#[tokio::test]
async fn one_get_with_no_cookie_and_no_credential() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/a.png"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(PNG)
                .insert_header("content-type", "image/jpeg"),
        )
        .expect(1)
        .mount(&server)
        .await;
    let url = format!("{}/a.png", server.uri());
    let got = image::fetch(&url).await.expect("the image");
    // The bytes decide, not the label.
    assert_eq!(got.mime, "image/png");
    assert_eq!(got.size, PNG.len() as u64);
    // Kept in memory: asked again, the server is not asked again (`expect(1)`).
    image::fetch(&url).await.expect("the kept image");
    let asked: Vec<Request> = server.received_requests().await.expect("recorded");
    assert_eq!(asked.len(), 1);
    for name in ["cookie", "authorization"] {
        assert!(!asked[0].headers.contains_key(name), "{name} was sent");
    }
}

#[tokio::test]
async fn an_answer_that_is_no_image_is_refused_by_name() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("<!doctype html><html><body>login</body></html>")
                .insert_header("content-type", "image/png"),
        )
        .mount(&server)
        .await;
    let error = image::fetch(&format!("{}/b.png", server.uri()))
        .await
        .expect_err("no image");
    assert!(error.message.contains("an HTML page"), "{}", error.message);
}

#[tokio::test]
async fn a_redirect_to_another_origin_is_not_followed() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("location", "https://elsewhere.example/c.png"),
        )
        .mount(&server)
        .await;
    let error = image::fetch(&format!("{}/c.png", server.uri()))
        .await
        .expect_err("not followed");
    assert!(
        error.message.contains("elsewhere.example"),
        "{}",
        error.message
    );
}

#[tokio::test]
async fn only_http_and_https_addresses() {
    let error = image::fetch("file:///etc/passwd")
        .await
        .expect_err("refused");
    assert!(
        error.message.contains("http and https"),
        "{}",
        error.message
    );
}
