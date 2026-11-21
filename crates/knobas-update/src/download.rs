//! The download (decisions D8).
//!
//! The artifact streams through `HttpClient::send_streaming` into `<dir>/<file>.part`, through the
//! signature check as it lands. At the end the signature is checked against the trusted keys and
//! its comment against the file name; only then is the file renamed to `<file>`. A failure, a
//! stall of [`knobas_http::STALL_TIMEOUT`] or a cancel deletes the `.part` file. No resume.

use std::path::{Path, PathBuf};
use std::sync::PoisonError;
use std::time::{Duration, Instant};

use knobas_http::Method;
use tokio::io::AsyncWriteExt as _;
use tokio_util::sync::CancellationToken;

use crate::keys::Keys;
use crate::manifest::Bound;
use crate::verify::{self, VerifyError};

/// The most bytes an artifact may have.
pub const MAX_BYTES: u64 = 400 * 1024 * 1024;
/// Progress goes out at most this often.
pub const PROGRESS_EVERY: Duration = Duration::from_millis(250);

/// Why a download did not land.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DownloadError {
    #[error("cancelled")]
    Cancelled,
    /// The transfer broke off or never started; `received` bytes had arrived.
    #[error("{why}")]
    Stopped { received: u64, why: String },
    #[error("{0}")]
    Signature(VerifyError),
    #[error("the download is larger than {MAX_BYTES} bytes")]
    TooLarge { received: u64 },
    #[error("{0}")]
    Disk(String),
}

/// Download `bound` into `dir`, verified, and answer the landed file's path. `progress` hears
/// (received, total) at most every [`PROGRESS_EVERY`].
///
/// # Errors
///
/// [`DownloadError`]; the `.part` file is gone in every case.
pub async fn download(
    bound: &Bound,
    dir: &Path,
    keys: &Keys,
    stall: Duration,
    cancel: &CancellationToken,
    progress: &mut (dyn FnMut(u64, Option<u64>) + Send),
) -> Result<PathBuf, DownloadError> {
    let disk = |error: std::io::Error| DownloadError::Disk(error.to_string());
    let signature = verify::decode(&bound.signature).map_err(DownloadError::Signature)?;
    // Before a byte is asked for: a signature by no trusted key, or for another file.
    verify::names(&signature, &bound.file_name).map_err(DownloadError::Signature)?;
    let mut check = keys.stream(&signature).map_err(DownloadError::Signature)?;

    tokio::fs::create_dir_all(dir).await.map_err(disk)?;
    let part = dir.join(format!("{}.part", bound.file_name));
    let done = dir.join(&bound.file_name);

    let stopped = |received: u64, why: String| DownloadError::Stopped { received, why };
    let (client, path, refused) = crate::fetch::client_for(&bound.url, MAX_BYTES)
        .map_err(|error| stopped(0, error.to_string()))?;
    let sent = tokio::select! {
        () = cancel.cancelled() => return Err(DownloadError::Cancelled),
        sent = client.send_streaming(client.request(Method::GET, &path), stall) => sent,
    };
    let mut body = sent.map_err(|error| {
        let hop = refused
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        stopped(0, crate::fetch::failure(&error, hop).to_string())
    })?;
    let total = body
        .headers()
        .get(knobas_http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    if total.is_some_and(|total| total > MAX_BYTES) {
        return Err(DownloadError::TooLarge { received: 0 });
    }

    let mut file = tokio::fs::File::create(&part).await.map_err(disk)?;
    let outcome: Result<(), DownloadError> = async {
        let mut received = 0_u64;
        let mut told = Instant::now();
        progress(0, total);
        loop {
            let chunk = tokio::select! {
                () = cancel.cancelled() => return Err(DownloadError::Cancelled),
                chunk = body.chunk() => chunk,
            };
            let chunk = match chunk {
                Ok(Some(chunk)) => chunk,
                Ok(None) => break,
                Err(error) => return Err(stopped(received, error.to_string())),
            };
            received += chunk.len() as u64;
            if received > MAX_BYTES {
                return Err(DownloadError::TooLarge { received });
            }
            check.update(&chunk);
            file.write_all(&chunk).await.map_err(disk)?;
            if told.elapsed() >= PROGRESS_EVERY {
                told = Instant::now();
                progress(received, total);
            }
        }
        file.flush().await.map_err(disk)?;
        file.sync_all().await.map_err(disk)?;
        progress(received, total);
        Ok(())
    }
    .await;
    drop(file);
    let verified = outcome.and_then(|()| check.finish().map_err(DownloadError::Signature));
    match verified {
        Ok(()) => {
            tokio::fs::rename(&part, &done).await.map_err(disk)?;
            Ok(done)
        }
        Err(error) => {
            let _ = tokio::fs::remove_file(&part).await;
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const FILE: &str = "knobas_3.2.9-ux28_x64-setup.exe";
    const ONE: &str = include_str!("../tests/fixtures/one.pub");
    const TWO: &str = include_str!("../tests/fixtures/two.pub");
    const BODY: &[u8] = include_bytes!("../tests/fixtures/one/knobas_3.2.9-ux28_x64-setup.exe");
    const SIG: &str = include_str!("../tests/fixtures/one/knobas_3.2.9-ux28_x64-setup.exe.sig");
    const SIG_27: &str =
        include_str!("../tests/fixtures/other/knobas_3.2.9-ux27_x64-setup.exe.sig");

    fn keys() -> Keys {
        Keys::from_texts(&[("primary", ONE), ("spare", TWO)]).unwrap()
    }

    fn bound(server: &MockServer, signature: &str) -> Bound {
        Bound {
            url: format!("{}/releases/download/v3.2.9-ux28/{FILE}", server.uri()),
            file_name: FILE.to_owned(),
            signature: signature.to_owned(),
        }
    }

    async fn serve(server: &MockServer, body: Vec<u8>) {
        Mock::given(method("GET"))
            .and(path(format!("/releases/download/v3.2.9-ux28/{FILE}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
            .mount(server)
            .await;
    }

    async fn run(
        bound: &Bound,
        dir: &Path,
        cancel: &CancellationToken,
    ) -> Result<PathBuf, DownloadError> {
        let mut heard = Vec::new();
        let outcome = download(
            bound,
            dir,
            &keys(),
            Duration::from_secs(2),
            cancel,
            &mut |r, t| heard.push((r, t)),
        )
        .await;
        if outcome.is_ok() {
            assert_eq!(
                heard.last(),
                Some(&(BODY.len() as u64, Some(BODY.len() as u64)))
            );
        }
        outcome
    }

    fn files(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    #[tokio::test]
    async fn a_download_lands_whole_and_verified() {
        let server = MockServer::builder().start().await;
        serve(&server, BODY.to_vec()).await;
        let dir = tempfile::tempdir().unwrap();
        let landed = run(&bound(&server, SIG), dir.path(), &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(std::fs::read(&landed).unwrap(), BODY);
        assert_eq!(files(dir.path()), vec![FILE.to_owned()]);
    }

    /// Mutants: skip the final check (the changed byte lands); rename before verifying.
    #[tokio::test]
    async fn a_body_with_one_byte_changed_is_deleted_and_says_signature() {
        let server = MockServer::builder().start().await;
        let mut body = BODY.to_vec();
        body[5] ^= 1;
        serve(&server, body).await;
        let dir = tempfile::tempdir().unwrap();
        let outcome = run(&bound(&server, SIG), dir.path(), &CancellationToken::new()).await;
        assert_eq!(
            outcome,
            Err(DownloadError::Signature(VerifyError::Mismatch))
        );
        assert!(files(dir.path()).is_empty(), "{:?}", files(dir.path()));
    }

    #[tokio::test]
    async fn a_signature_for_another_file_name_is_refused() {
        let server = MockServer::builder().start().await;
        serve(&server, BODY.to_vec()).await;
        let dir = tempfile::tempdir().unwrap();
        let outcome = run(
            &bound(&server, SIG_27),
            dir.path(),
            &CancellationToken::new(),
        )
        .await;
        assert!(
            matches!(
                outcome,
                Err(DownloadError::Signature(VerifyError::OtherFile { .. }))
            ),
            "{outcome:?}"
        );
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "nothing is asked for"
        );
    }

    #[tokio::test]
    async fn a_redirect_to_plain_http_is_never_requested() {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path(format!("/releases/download/v3.2.9-ux28/{FILE}")))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", "http://example.invalid/x.exe"),
            )
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let outcome = run(&bound(&server, SIG), dir.path(), &CancellationToken::new()).await;
        match outcome {
            Err(DownloadError::Stopped { why, .. }) => assert!(why.contains("not https"), "{why}"),
            other => panic!("{other:?}"),
        }
        assert!(files(dir.path()).is_empty());
    }

    #[tokio::test]
    async fn cancel_leaves_no_part_file() {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path(format!("/releases/download/v3.2.9-ux28/{FILE}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(BODY.to_vec())
                    .set_delay(Duration::from_millis(500)),
            )
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let cancel = CancellationToken::new();
        let cancelling = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            cancelling.cancel();
        });
        let outcome = run(&bound(&server, SIG), dir.path(), &cancel).await;
        assert_eq!(outcome, Err(DownloadError::Cancelled));
        assert!(
            files(dir.path())
                .iter()
                .all(|name| !name.ends_with(".part"))
        );
    }

    #[tokio::test]
    async fn a_stalled_body_ends_the_download() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        // Headers, half a body, then nothing.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 4096];
            let _ = socket.read(&mut request).await;
            let head = format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n", BODY.len());
            socket.write_all(head.as_bytes()).await.unwrap();
            socket.write_all(&BODY[..4]).await.unwrap();
            tokio::time::sleep(Duration::from_secs(30)).await;
        });
        let bound = Bound {
            url: format!("http://{address}/releases/download/v3.2.9-ux28/{FILE}"),
            file_name: FILE.to_owned(),
            signature: SIG.to_owned(),
        };
        let dir = tempfile::tempdir().unwrap();
        let outcome = download(
            &bound,
            dir.path(),
            &keys(),
            Duration::from_millis(300),
            &CancellationToken::new(),
            &mut |_, _| {},
        )
        .await;
        match outcome {
            Err(DownloadError::Stopped { received, why }) => {
                assert_eq!(received, 4);
                assert!(why.contains("no bytes"), "{why}");
            }
            other => panic!("{other:?}"),
        }
        assert!(files(dir.path()).is_empty());
    }
}
