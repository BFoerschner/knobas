//! A Jira that answers an attachment the account may not see with its
//! *Access Denied* page (#2410).
//!
//! Jira Data Center 10.3's attachment servlet does not answer 403 to a
//! signed-in user without *Browse Projects* on the issue's project. It answers
//! **200** with `text/html` and the product's "Access Denied" page, at the
//! file's own address and with no redirect. Measured on the seeded Jira on
//! 2026-11-02. A real file always carries `Content-Disposition` (`inline` for
//! an image, `attachment` for the rest, an HTML file included), and that page
//! carries none. Read as the file, the page would be previewed or saved under
//! the file's name.
//!
//! wiremock rather than mockd: mockd is frozen (ADR-0013) and knows no
//! permissions.

use knobas_source::instance::SourceInstance;
use knobas_source::view::FileBytes;
use knobas_source::{AuthMethod, Source, SourceError};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The head of the page Jira served, cut down.
const ACCESS_DENIED: &str = "\n\n<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n\
    <title>Access Denied - knobas testenv</title>\n</head>\n<body></body>\n</html>\n";

const FILE: &str = "/secure/attachment/10000/oncall-dashboard.png";

async fn jira(file: ResponseTemplate) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path(FILE))
        .respond_with(file)
        .mount(&server)
        .await;
    server
}

fn source(base_url: &str) -> Box<dyn Source> {
    let instance = SourceInstance {
        id: "jira".to_owned(),
        kind: "jira".to_owned(),
        display_name: "Jira".to_owned(),
        base_url: base_url.to_owned(),
        auth: Some(AuthMethod::Pat),
        secret: Some("token".to_owned()),
        account: None,
        config: serde_json::json!({}),
    };
    match knobas_source_jira::build(instance) {
        Ok(source) => source,
        Err(error) => panic!("the adapter must build: {error:?}"),
    }
}

/// What a download handed its sink.
#[derive(Default)]
struct Kept {
    name: Option<String>,
    bytes: Vec<u8>,
}

#[async_trait::async_trait]
impl knobas_source::run::DownloadSink for Kept {
    async fn start(&mut self, file_name: &str) -> Result<(), SourceError> {
        self.name = Some(file_name.to_owned());
        Ok(())
    }
    async fn write(&mut self, bytes: &[u8]) -> Result<(), SourceError> {
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
}

fn denied() -> ResponseTemplate {
    // `set_body_raw` and not `set_body_string`, which would answer text/plain.
    ResponseTemplate::new(200)
        .insert_header("X-Seraph-LoginReason", "OK")
        .set_body_raw(ACCESS_DENIED, "text/html;charset=UTF-8")
}

fn is_denied(answer: &Result<impl std::fmt::Debug, SourceError>) -> bool {
    matches!(answer, Err(SourceError::Unauthorized { status: Some(403) }))
}

#[tokio::test]
async fn the_access_denied_page_is_a_403_and_never_the_files_bytes() {
    let jira = jira(denied()).await;
    let url = format!("{}{FILE}", jira.uri());

    let read = source(&jira.uri())
        .attachment_bytes("jira:OPS-77", &url, 5 * 1024 * 1024)
        .await;
    assert!(is_denied(&read), "{read:?}");

    let mut kept = Kept::default();
    let download = source(&jira.uri())
        .download_attachment("jira:OPS-77", &url, "oncall-dashboard.png", &mut kept)
        .await;
    assert!(is_denied(&download), "{download:?}");
    assert!(
        kept.name.is_none() && kept.bytes.is_empty(),
        "nothing was saved"
    );
}

/// An HTML file is still a file: Jira serves it with a disposition, as an
/// attachment, and its bytes come through.
#[tokio::test]
async fn an_html_file_jira_serves_as_an_attachment_reads_through() {
    let jira = jira(
        ResponseTemplate::new(200)
            .insert_header(
                "Content-Disposition",
                "attachment; filename*=UTF-8''oncall-dashboard.png;",
            )
            .set_body_raw("<p>a file</p>", "text/html;charset=UTF-8"),
    )
    .await;
    let url = format!("{}{FILE}", jira.uri());

    let read = source(&jira.uri())
        .attachment_bytes("jira:OPS-77", &url, 1024)
        .await;
    assert_eq!(
        read.ok(),
        Some(Some(FileBytes::Read(b"<p>a file</p>".to_vec())))
    );
    let mut kept = Kept::default();
    source(&jira.uri())
        .download_attachment("jira:OPS-77", &url, "oncall-dashboard.png", &mut kept)
        .await
        .expect("the file downloads");
    assert_eq!(kept.bytes, b"<p>a file</p>");
}
