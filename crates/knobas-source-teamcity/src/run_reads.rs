//! A build's live reads for the run page (ADR-0022; #854, #855, #856): its
//! whole log, its tests and one test's history, its artifact folders, a
//! text artifact's head, and the downloads. All read-only, none stored.
//!
//! Endpoints (TeamCity REST, plus the two download URLs its web UI links):
//!
//! * `GET /downloadBuildLog.html?buildId=N` -- the whole log, streamed
//!   ([`crate::log::LogReader`]).
//! * `GET /app/rest/testOccurrences?locator=build:(id:N),count:C` -- the tests;
//!   `locator=test:(id:T),buildType:(id:B),count:10` -- one test's history.
//! * `GET /app/rest/builds/id:N/artifacts/children/<path>` -- one folder.
//! * `GET /app/rest/builds/id:N/artifacts/files/<path>` -- one file's bytes.
//! * `GET /app/rest/builds/id:N/artifacts/archived/<path>` -- a folder, zipped.
//! * `GET /downloadArtifacts.html?buildId=N` -- every artifact, zipped.

use knobas_source::SourceError;
use knobas_source::run::{
    ArtifactEntry, ArtifactPreview, DownloadSink, DownloadTarget, PREVIEW_BYTES_MAX, RunLog,
    RunTest, RunTests, TEST_HISTORY_RUNS, TESTS_MAX, TestOutcome, TestStatus, is_artifact_path,
    module_of,
};

use crate::client::HttpRest;
use crate::ids::path_escaped;
use crate::log::LogReader;
use crate::rest::{ARTIFACT_FIELDS, Files};

/// What a run's tests are asked for (#855).
pub(crate) const RUN_TEST_FIELDS: &str =
    "nextHref,testOccurrence(name,status,ignored,muted,duration,details,newFailure,test(id))";

/// What a test's history is asked for (#855).
pub(crate) const TEST_HISTORY_FIELDS: &str =
    "testOccurrence(status,ignored,build(id,number,branchName))";

/// What a download's file name is read from: the configuration and number.
const NAME_FIELDS: &str = "number,buildType(id)";

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Occurrences {
    #[serde(default)]
    test_occurrence: Vec<Occurrence>,
    next_href: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Occurrence {
    name: Option<String>,
    status: Option<String>,
    #[serde(default)]
    ignored: bool,
    #[serde(default)]
    muted: bool,
    duration: Option<u64>,
    details: Option<String>,
    #[serde(default)]
    new_failure: bool,
    test: Option<TestRef>,
    build: Option<BuildRef>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct TestRef {
    id: Option<serde_json::Value>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildRef {
    id: Option<i64>,
    number: Option<String>,
    branch_name: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Named {
    number: Option<String>,
    build_type: Option<NamedType>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct NamedType {
    id: Option<String>,
}

fn status_of(o: &Occurrence) -> TestStatus {
    if o.ignored {
        TestStatus::Ignored
    } else if o.status.as_deref() == Some("FAILURE") {
        TestStatus::Failed
    } else {
        TestStatus::Passed
    }
}

/// A test's id as the history locator takes it: TeamCity issues numbers, and
/// nothing else may reach the locator.
fn test_id(value: &serde_json::Value) -> Option<String> {
    let id = match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => return None,
    };
    is_test_id(&id).then_some(id)
}

fn is_test_id(id: &str) -> bool {
    let digits = id.strip_prefix('-').unwrap_or(id);
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

fn run_test(o: Occurrence) -> RunTest {
    let status = status_of(&o);
    let name = o.name.unwrap_or_default();
    RunTest {
        id: o.test.and_then(|t| t.id).as_ref().and_then(test_id),
        module: module_of(&name),
        name,
        status,
        muted: o.muted,
        new_failure: o.new_failure,
        duration_ms: o.duration,
        details: o.details.filter(|d| !d.trim().is_empty()),
    }
}

/// Failed first (a muted failure after the others), then the rest, each in
/// the source's order.
pub(crate) fn ordered(mut tests: Vec<RunTest>) -> Vec<RunTest> {
    tests.sort_by_key(|t| match (t.status, t.muted) {
        (TestStatus::Failed, false) => 0,
        (TestStatus::Failed, true) => 1,
        _ => 2,
    });
    tests
}

/// The artifact path's segments, each percent-encoded, joined by `/`.
fn escaped_path(path: &str) -> String {
    path.split('/')
        .map(path_escaped)
        .collect::<Vec<_>>()
        .join("/")
}

fn check_path(path: &str) -> Result<(), SourceError> {
    if is_artifact_path(path) {
        Ok(())
    } else {
        Err(SourceError::protocol(format!(
            "teamcity: {path:?} is not a path inside a build's artifacts"
        )))
    }
}

/// The last segment of a path: the file name it suggests.
fn last_segment(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

impl HttpRest {
    /// A GET streamed chunk by chunk into `each`; `each` answers `false` to
    /// stop reading. The body has no total deadline, only a stall timeout: a
    /// whole log can take longer than a request may (#1012).
    async fn stream(
        &self,
        path: &str,
        query: &[(&str, String)],
        mut each: impl FnMut(&[u8]) -> bool + Send,
    ) -> Result<(), SourceError> {
        let request = self
            .client
            .request(knobas_http::Method::GET, path)
            .query(query);
        let mut response = self
            .client
            .send_streaming(request, knobas_http::STALL_TIMEOUT)
            .await?;
        while let Some(chunk) = response.chunk().await.map_err(|error| {
            SourceError::Unreachable(format!("teamcity: {path} broke off: {error}"))
        })? {
            if !each(&chunk) {
                break;
            }
        }
        Ok(())
    }

    /// A range of build `id`'s whole log, and its steps (#854).
    pub(crate) async fn run_log(
        &self,
        id: i64,
        from: Option<u64>,
        count: u32,
    ) -> Result<RunLog, SourceError> {
        let mut reader = LogReader::new(from, count);
        self.stream(
            "downloadBuildLog.html",
            &[("buildId", id.to_string())],
            |bytes| {
                reader.feed(bytes);
                true
            },
        )
        .await?;
        Ok(reader.finish())
    }

    async fn occurrences(&self, locator: &str, fields: &str) -> Result<Occurrences, SourceError> {
        self.client
            .get_json(
                "app/rest/testOccurrences",
                &[("locator", locator), ("fields", fields)],
            )
            .await
    }

    /// Build `id`'s tests, failed first, at most [`TESTS_MAX`] (#855). Past
    /// the cap the failed ones are read on their own, so none is left out.
    pub(crate) async fn run_tests(&self, id: i64) -> Result<RunTests, SourceError> {
        let all = self
            .occurrences(
                &format!("build:(id:{id}),count:{}", TESTS_MAX + 1),
                RUN_TEST_FIELDS,
            )
            .await?;
        let mut truncated =
            all.next_href.is_some() || all.test_occurrence.len() > TESTS_MAX as usize;
        let mut tests: Vec<RunTest> = all.test_occurrence.into_iter().map(run_test).collect();
        if truncated {
            let failed = self
                .occurrences(
                    &format!("build:(id:{id}),status:FAILURE,count:{TESTS_MAX}"),
                    RUN_TEST_FIELDS,
                )
                .await?;
            let mut failed: Vec<RunTest> =
                failed.test_occurrence.into_iter().map(run_test).collect();
            tests.retain(|t| t.status != TestStatus::Failed);
            failed.append(&mut tests);
            tests = failed;
        }
        let mut tests = ordered(tests);
        if tests.len() > TESTS_MAX as usize {
            tests.truncate(TESTS_MAX as usize);
            truncated = true;
        }
        Ok(RunTests { tests, truncated })
    }

    /// How test `test` came out in the last [`TEST_HISTORY_RUNS`] builds of
    /// build `id`'s configuration, newest first (#855).
    pub(crate) async fn test_history(
        &self,
        source_id: &str,
        id: i64,
        test: &str,
    ) -> Result<Vec<TestOutcome>, SourceError> {
        if !is_test_id(test) {
            return Err(SourceError::protocol(format!(
                "teamcity: {test:?} is not a test id TeamCity could have issued"
            )));
        }
        let (config, _) = self.config_and_number(id).await?;
        let config = crate::ids::config_id(&format!("{source_id}:build:{id}"), &config)?;
        let answer = self
            .occurrences(
                &format!("test:(id:{test}),buildType:(id:{config}),count:{TEST_HISTORY_RUNS}"),
                TEST_HISTORY_FIELDS,
            )
            .await?;
        let mut outcomes: Vec<(i64, TestOutcome)> = answer
            .test_occurrence
            .into_iter()
            .filter_map(|o| {
                let status = status_of(&o);
                let build = o.build?;
                let build_id = build.id?;
                Some((
                    build_id,
                    TestOutcome {
                        run: format!("{source_id}:build:{build_id}"),
                        number: build.number,
                        branch: build.branch_name,
                        status,
                    },
                ))
            })
            .collect();
        outcomes.sort_by_key(|(build_id, _)| std::cmp::Reverse(*build_id));
        Ok(outcomes.into_iter().map(|(_, o)| o).collect())
    }

    /// One folder of build `id`'s artifacts, `""` the root (#856).
    pub(crate) async fn run_artifacts(
        &self,
        id: i64,
        path: &str,
    ) -> Result<Vec<ArtifactEntry>, SourceError> {
        check_path(path)?;
        let files: Files = if path.is_empty() {
            self.artifacts(id).await?
        } else {
            self.client
                .get_json(
                    &format!(
                        "app/rest/builds/id:{id}/artifacts/children/{}",
                        escaped_path(path)
                    ),
                    &[("fields", ARTIFACT_FIELDS)],
                )
                .await?
        };
        Ok(files
            .file
            .into_iter()
            .map(|f| {
                let folder = f.children.is_some();
                ArtifactEntry {
                    path: if path.is_empty() {
                        f.name.clone()
                    } else {
                        format!("{path}/{}", f.name)
                    },
                    size: if folder { None } else { f.size },
                    children: f
                        .children
                        .and_then(|c| c.count)
                        .map(|n| u32::try_from(n).unwrap_or(u32::MAX)),
                    folder,
                    name: f.name,
                }
            })
            .collect())
    }

    /// The first [`PREVIEW_BYTES_MAX`] bytes of one artifact file (#856); a
    /// file with a NUL byte in them is not text.
    pub(crate) async fn artifact_preview(
        &self,
        id: i64,
        path: &str,
    ) -> Result<ArtifactPreview, SourceError> {
        check_path(path)?;
        if path.is_empty() {
            return Err(SourceError::protocol("teamcity: a preview names a file"));
        }
        let mut kept: Vec<u8> = Vec::new();
        self.stream(
            &format!(
                "app/rest/builds/id:{id}/artifacts/files/{}",
                escaped_path(path)
            ),
            &[],
            |bytes| {
                kept.extend_from_slice(bytes);
                kept.len() <= PREVIEW_BYTES_MAX
            },
        )
        .await?;
        Ok(preview_of(&kept))
    }

    /// Stream `target` of build `id` into `sink` (D8, #856).
    pub(crate) async fn download(
        &self,
        id: i64,
        target: &DownloadTarget,
        sink: &mut dyn DownloadSink,
    ) -> Result<(), SourceError> {
        let (path, query, name) = match target {
            DownloadTarget::Log => {
                let (config, number) = self.config_and_number(id).await?;
                (
                    "downloadBuildLog.html".to_owned(),
                    vec![("buildId", id.to_string())],
                    format!("{config}-{number}.log"),
                )
            }
            DownloadTarget::All => {
                let (config, number) = self.config_and_number(id).await?;
                (
                    "downloadArtifacts.html".to_owned(),
                    vec![("buildId", id.to_string())],
                    format!("{config}-{number}-artifacts.zip"),
                )
            }
            DownloadTarget::File { path } | DownloadTarget::Folder { path } => {
                check_path(path)?;
                if path.is_empty() {
                    return Err(SourceError::protocol(
                        "teamcity: a file or folder download names its path",
                    ));
                }
                let folder = matches!(target, DownloadTarget::Folder { .. });
                let (kind, name) = if folder {
                    ("archived", format!("{}.zip", last_segment(path)))
                } else {
                    ("files", last_segment(path).to_owned())
                };
                (
                    format!(
                        "app/rest/builds/id:{id}/artifacts/{kind}/{}",
                        escaped_path(path)
                    ),
                    vec![],
                    name,
                )
            }
        };
        // The sink's writes are async and `stream`'s callback is not, so the
        // bytes are read here chunk by chunk rather than through it. Streamed
        // like `stream`: a 1 GB zip may take as long as it takes (#1012).
        let request = self
            .client
            .request(knobas_http::Method::GET, &path)
            .query(&query);
        let mut response = self
            .client
            .send_streaming(request, knobas_http::STALL_TIMEOUT)
            .await?;
        sink.start(&name).await?;
        while let Some(chunk) = response.chunk().await.map_err(|error| {
            SourceError::Unreachable(format!(
                "teamcity: the download of {name} broke off: {error}"
            ))
        })? {
            sink.write(&chunk).await?;
        }
        Ok(())
    }

    async fn config_and_number(&self, id: i64) -> Result<(String, String), SourceError> {
        let named: Named = self
            .client
            .get_json(
                &format!("app/rest/builds/id:{id}"),
                &[("fields", NAME_FIELDS)],
            )
            .await?;
        let config = named
            .build_type
            .and_then(|t| t.id)
            .unwrap_or_else(|| "build".to_owned());
        let number = named.number.unwrap_or_else(|| id.to_string());
        Ok((config, number))
    }
}

/// What a preview of `bytes` is: text unless it holds a NUL, cut at
/// [`PREVIEW_BYTES_MAX`].
pub(crate) fn preview_of(bytes: &[u8]) -> ArtifactPreview {
    let truncated = bytes.len() > PREVIEW_BYTES_MAX;
    let head = &bytes[..bytes.len().min(PREVIEW_BYTES_MAX)];
    if head.contains(&0) {
        return ArtifactPreview {
            text: None,
            truncated,
        };
    }
    let text = match std::str::from_utf8(head) {
        Ok(text) => text.to_owned(),
        // Cut inside a character: the valid prefix, never a replacement mark.
        Err(error) if truncated && error.error_len().is_none() => {
            String::from_utf8_lossy(&head[..error.valid_up_to()]).into_owned()
        }
        Err(_) => String::from_utf8_lossy(head).into_owned(),
    };
    ArtifactPreview {
        text: Some(text),
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test(name: &str, status: TestStatus, muted: bool) -> RunTest {
        RunTest {
            id: None,
            name: name.into(),
            module: None,
            status,
            muted,
            new_failure: false,
            duration_ms: None,
            details: None,
        }
    }

    #[test]
    fn failed_tests_come_first_and_a_muted_failure_after_them() {
        let names: Vec<String> = ordered(vec![
            test("a", TestStatus::Passed, false),
            test("b", TestStatus::Failed, true),
            test("c", TestStatus::Ignored, false),
            test("d", TestStatus::Failed, false),
            test("e", TestStatus::Passed, false),
        ])
        .into_iter()
        .map(|t| t.name)
        .collect();
        assert_eq!(names, ["d", "b", "a", "c", "e"]);
    }

    #[test]
    fn only_a_number_is_a_test_id() {
        assert_eq!(test_id(&serde_json::json!(-4200)).as_deref(), Some("-4200"));
        assert_eq!(test_id(&serde_json::json!("917")).as_deref(), Some("917"));
        assert_eq!(test_id(&serde_json::json!("1),build:(id:2")), None);
        assert_eq!(test_id(&serde_json::json!(null)), None);
    }

    #[test]
    fn a_preview_is_text_unless_it_holds_a_nul_and_says_when_it_was_cut() {
        assert_eq!(preview_of(b"a\nb").text.as_deref(), Some("a\nb"));
        assert_eq!(preview_of(b"PK\x03\x04\0\0").text, None);
        // One byte, then two-byte characters: the cut falls inside one.
        let long = format!("a{}", "é".repeat(PREVIEW_BYTES_MAX)).into_bytes();
        let cut = preview_of(&long);
        assert!(cut.truncated);
        let text = cut.text.unwrap();
        assert!(!text.contains('\u{fffd}'), "no half a character at the cut");
        assert_eq!(text.len(), PREVIEW_BYTES_MAX - 1);
    }

    #[test]
    fn an_artifact_path_is_escaped_segment_by_segment() {
        assert_eq!(
            escaped_path("test report/a+b.xml"),
            "test%20report/a%2Bb.xml"
        );
        assert_eq!(last_segment("logs/integration.log"), "integration.log");
        assert!(check_path("../etc").is_err());
    }

    #[test]
    fn the_selectors_have_no_whitespace_and_balance() {
        for f in [RUN_TEST_FIELDS, TEST_HISTORY_FIELDS, NAME_FIELDS] {
            assert!(!f.contains(' '), "{f:?}");
            assert_eq!(f.matches('(').count(), f.matches(')').count(), "{f:?}");
        }
    }
}
