//! D9's ranking table (the everything-search decisions, 2026-11-20): where a
//! repository its files found stands in its kind, over a mirror of
//! repositories and a files index written for it. Each test is a row or two
//! of the table; `SearchFilters::sources` keeps each to its own source in the
//! shared database.

use std::collections::HashMap;

use chrono::{DateTime, Duration, TimeZone, Utc};
use knobas_files::{FileIndex, Repo};
use knobas_search::picker::Picker;
use knobas_search::{EverythingMatches, SearchFilters, SearchQuery, SearchResponse, Searcher};

use super::item_search::source;
use super::one_letter::pool;

fn at(minutes: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 1, 12, 0, 0).unwrap() + Duration::minutes(minutes)
}

const SERVER: &str = "package com.tidewater.settlebridge;\n\
                      \n\
                      import com.tidewater.settlebridge.config.SettleBridgeConfig;\n\
                      \n\
                      // Forwards settlement batches to the ledger.\n\
                      public final class SettleBridgeServer {\n\
                      \x20   private final SettleBridgeConfig config;\n\
                      }\n";

/// A mirror of repositories, and the files index of their files.
struct World {
    pool: sqlx::PgPool,
    source: String,
    picker: Picker,
    files: FileIndex,
    _dir: tempfile::TempDir,
    /// The repository a short name makes, by that name.
    ids: HashMap<&'static str, String>,
}

/// One repository: its short name, its full name, its description, its
/// files, and how many minutes after the hour it changed.
type Repository = (
    &'static str,
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
    i64,
);

async fn world(repositories: &[Repository], favourites: &[&str]) -> World {
    let pool = pool().await;
    let source = source("files");
    let dir = tempfile::tempdir().unwrap();
    let files = FileIndex::open(&dir.path().join("files-1")).unwrap();
    let mut writer = files.writer().unwrap();
    let mut repos = HashMap::new();
    let mut ids = HashMap::new();
    for (no, (short, name, description, contents, minutes)) in repositories.iter().enumerate() {
        let id = format!("{source}:{name}");
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'repo', $2)")
            .bind(&id)
            .bind(name)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
             values ($1, $2, 'repo', $3, $4, '{}'::jsonb, $5)",
        )
        .bind(&id)
        .bind(&source)
        .bind(name)
        .bind(format!("{name}\n{description}"))
        .bind(at(*minutes))
        .execute(&pool)
        .await
        .unwrap();
        let no = u64::try_from(no).unwrap() + 1;
        for (path, text) in *contents {
            writer.put(no, path, Some(text)).unwrap();
        }
        repos.insert(
            no,
            Repo {
                entity_id: id.clone(),
                source_id: source.clone(),
                favourite: favourites.contains(short),
                updated: at(*minutes).timestamp(),
            },
        );
        ids.insert(*short, id);
    }
    writer.commit().unwrap();
    drop(writer);
    files.set_repos(repos);
    let picker = Picker::default();
    picker.reload(&pool).await.unwrap();
    World {
        pool,
        source,
        picker,
        files,
        _dir: dir,
        ids,
    }
}

impl World {
    async fn ask_with(
        &self,
        raw: &str,
        titles: bool,
        files: Option<FileIndex>,
        matches: EverythingMatches,
    ) -> SearchResponse {
        let mut searcher = Searcher::new(self.pool.clone())
            .with_picker(self.picker.clone())
            .with_everything(matches);
        if let Some(files) = files {
            searcher = searcher.with_files(files);
        }
        searcher
            .search(SearchQuery {
                titles,
                raw: raw.to_owned(),
                limit: 40,
                filters: SearchFilters {
                    sources: vec![self.source.clone()],
                    ..SearchFilters::default()
                },
            })
            .await
            .unwrap()
    }

    async fn ask(&self, raw: &str) -> SearchResponse {
        self.ask_with(
            raw,
            false,
            Some(self.files.clone()),
            EverythingMatches::default(),
        )
        .await
    }

    /// The repositories' kind of `response`, by short name.
    fn repos(&self, response: &SearchResponse) -> Vec<&'static str> {
        let by_id: HashMap<&String, &'static str> =
            self.ids.iter().map(|(short, id)| (id, *short)).collect();
        response
            .groups
            .iter()
            .filter(|group| group.kind == "repo")
            .flat_map(|group| &group.hits)
            .filter_map(|hit| by_id.get(&hit.row.entity_id).copied())
            .collect()
    }

    fn hit<'a>(&self, response: &'a SearchResponse, short: &str) -> &'a knobas_search::SearchHit {
        response
            .groups
            .iter()
            .flat_map(|group| &group.hits)
            .find(|hit| hit.row.entity_id == self.ids[short])
            .unwrap_or_else(|| panic!("no {short} in {response:#?}"))
    }
}

fn flat(segments: &[knobas_search::Segment]) -> String {
    segments
        .iter()
        .map(|s| {
            if s.hit {
                format!("[{}]", s.text)
            } else {
                s.text.clone()
            }
        })
        .collect()
}

const GATEWAY: Repository = (
    "gateway",
    "tidewater/ledger-gateway",
    "Settles batches against the ledger.",
    &[
        (
            "src/main/java/com/tidewater/settlebridge/SettleBridgeServer.java",
            SERVER,
        ),
        (
            "src/main/java/com/tidewater/settlebridge/config/SettleBridgeConfig.java",
            "public record SettleBridgeConfig() {}",
        ),
        (
            "src/test/java/com/tidewater/settlebridge/SettleBridgeServerTest.java",
            "class SettleBridgeServerTest {}",
        ),
        ("deploy/k8s/deployment.yaml", "image: settle-bridge:4.2.0"),
        ("README.md", "The ledger gateway."),
        ("pom.xml", "<artifactId>ledger-gateway</artifactId>"),
    ],
    10,
);
const API: Repository = (
    "api",
    "tidewater/ledger-api",
    "The ledger's public face.",
    &[
        (
            "src/Settle.java",
            "import com.tidewater.settlebridge.client.Client;",
        ),
        (
            "src/Batch.java",
            "import com.tidewater.settlebridge.client.Batch;",
        ),
        ("src/LedgerClient.java", "public class LedgerClient {}"),
    ],
    20,
);

/// Rows 1, 3 and 4: the renamed repository, found by no name and no
/// description, first by its files; its clients by an import line after it.
#[tokio::test]
async fn the_renamed_repository_is_found_by_its_files_alone_and_shown_by_its_class_line() {
    let w = world(&[GATEWAY, API], &[]).await;
    for typed in ["settlebridge", "SettleBridge"] {
        let answer = w.ask(typed).await;
        assert_eq!(w.repos(&answer), ["gateway", "api"], "{typed}");
    }
    // Row 3: two terms, both in `SettleBridge`'s parts; the client's
    // lower-case `settlebridge` holds no word that `bridge` begins.
    assert_eq!(w.repos(&w.ask("settle bridge").await), ["gateway"]);
    let answer = w.ask("settlebridge").await;
    let gateway = w.hit(&answer, "gateway");
    let files = gateway.files.expect("its files found it");
    // Three classes and the image `settle-bridge` by text, the three classes by name.
    assert_eq!((files.by_text, files.by_name), (4, 3));
    let first = &gateway.excerpts[0];
    assert_eq!(
        first.place.as_deref(),
        Some("src/main/java/com/tidewater/settlebridge/SettleBridgeServer.java:6")
    );
    assert_eq!(
        flat(&first.segments),
        "public final class [SettleBridge]Server {"
    );
    let api = w.hit(&answer, "api");
    assert_eq!(api.files.map(|f| (f.by_text, f.by_name)), Some((2, 0)));
    assert_eq!(
        flat(&api.excerpts[0].segments),
        "import com.tidewater.[settlebridge].client.Batch;"
    );
    // Row 4: the definition, not the import.
    let answer = w.ask("SettleBridgeServer").await;
    assert_eq!(
        flat(&w.hit(&answer, "gateway").excerpts[0].segments),
        "public final class [SettleBridgeServer] {"
    );
}

/// Rows 14 to 17: Titles, another kind's letter, and the boxes.
#[tokio::test]
async fn titles_another_kind_and_cleared_boxes_say_what_the_files_answer() {
    let w = world(&[GATEWAY, API], &[]).await;
    let titled = w
        .ask_with(
            "settlebridge",
            true,
            Some(w.files.clone()),
            EverythingMatches::default(),
        )
        .await;
    assert!(w.repos(&titled).is_empty(), "{titled:#?}");
    assert!(w.repos(&w.ask("#t settlebridge").await).is_empty());
    assert_eq!(w.repos(&w.ask("#r settlebridge").await), ["gateway", "api"]);
    let no_names = EverythingMatches {
        file_names: false,
        ..EverythingMatches::default()
    };
    let by_text = w
        .ask_with("settlebridge", false, Some(w.files.clone()), no_names)
        .await;
    assert_eq!(w.repos(&by_text), ["gateway", "api"]);
    assert_eq!(w.hit(&by_text, "gateway").files.map(|f| f.by_name), Some(0));
    let none = EverythingMatches {
        file_names: false,
        file_text: false,
        ..EverythingMatches::default()
    };
    assert!(
        w.repos(
            &w.ask_with("settlebridge", false, Some(w.files.clone()), none)
                .await
        )
        .is_empty()
    );
}

/// Row 5: the repository named by the word, then one with a file named by
/// it, then one whose files only say it.
#[tokio::test]
async fn a_name_then_a_files_name_then_a_files_text() {
    let w = world(
        &[
            (
                "says",
                "tidewater/billing",
                "",
                &[("src/Invoice.java", "// a payout follows")],
                30,
            ),
            (
                "named",
                "tidewater/payout-service",
                "",
                &[("src/Main.java", "x")],
                10,
            ),
            (
                "filed",
                "tidewater/scheduler",
                "",
                &[(
                    "src/payout/PayoutScheduler.java",
                    "class PayoutScheduler {}",
                )],
                20,
            ),
        ],
        &[],
    )
    .await;
    assert_eq!(w.repos(&w.ask("payout").await), ["named", "filed", "says"]);
}

/// Row 6: names the word holds come before any file row.
#[tokio::test]
async fn solid_names_rank_over_a_files_name() {
    let w = world(
        &[
            (
                "files",
                "tidewater/edge",
                "",
                &[("proxy/Proxy.java", "class Proxy {}")],
                50,
            ),
            ("auth", "tidewater/auth-proxy", "", &[], 10),
            ("adproxy", "tidewater/ADProxy", "", &[], 20),
        ],
        &[],
    )
    .await;
    let order = w.repos(&w.ask("proxy").await);
    assert_eq!(order.last(), Some(&"files"), "{order:?}");
    assert_eq!(order.len(), 3);
}

/// Rows 7 and 8: a file named by the words before a description that says
/// them once, and a description that says them four times before a file.
#[tokio::test]
async fn a_files_name_stands_between_a_strong_text_row_and_a_single_mention() {
    let w = world(
        &[
            (
                "docs",
                "tidewater/docs-site",
                "A client for the ledger.",
                &[],
                30,
            ),
            (
                "client",
                "tidewater/api",
                "",
                &[("src/LedgerClient.java", "public class LedgerClient {}")],
                10,
            ),
        ],
        &[],
    )
    .await;
    assert_eq!(w.repos(&w.ask("ledger client").await), ["client", "docs"]);
    let w = world(
        &[
            (
                "filed",
                "tidewater/consensus",
                "",
                &[("quorum/Quorum.java", "class Quorum {}")],
                30,
            ),
            (
                "said",
                "tidewater/raft",
                "quorum quorum quorum quorum: every write waits for a quorum.",
                &[],
                10,
            ),
        ],
        &[],
    )
    .await;
    assert_eq!(w.repos(&w.ask("quorum").await), ["said", "filed"]);
}

/// Rows 9 and 10: more files first; among equals a favourite, then the newer.
#[tokio::test]
async fn more_files_first_then_a_favourite_then_the_newer() {
    let five: &[(&str, &str)] = &[
        ("a", "RetryQueue"),
        ("b", "RetryQueue"),
        ("c", "RetryQueue"),
        ("d", "RetryQueue"),
        ("e", "RetryQueue"),
    ];
    let two: &[(&str, &str)] = &[("a", "RetryQueue"), ("b", "RetryQueue")];
    let w = world(
        &[
            ("two", "tidewater/billing", "", two, 30),
            ("five", "tidewater/payout-service", "", five, 10),
            ("newer", "tidewater/ledger", "", two, 40),
            ("favourite", "tidewater/refunds", "", two, 5),
        ],
        &["favourite"],
    )
    .await;
    assert_eq!(
        w.repos(&w.ask("retryqueue").await),
        ["five", "favourite", "newer", "two"]
    );
}

/// Row 12: a quoted phrase only where a line holds it side by side.
#[tokio::test]
async fn a_phrase_finds_only_a_line_that_holds_it() {
    let w = world(
        &[
            (
                "side",
                "tidewater/a",
                "",
                &[("log.txt", "the connection refused the call")],
                10,
            ),
            (
                "apart",
                "tidewater/b",
                "",
                &[("log.txt", "a connection was refused")],
                20,
            ),
        ],
        &[],
    )
    .await;
    assert_eq!(w.repos(&w.ask("\"connection refused\"").await), ["side"]);
    assert_eq!(
        w.repos(&w.ask("connection refused").await),
        ["apart", "side"]
    );
}

/// Row 13: two letters ask no file.
#[tokio::test]
async fn two_letters_find_no_file() {
    let w = world(
        &[(
            "co",
            "tidewater/x",
            "",
            &[("co/Co.java", "class Co {}")],
            10,
        )],
        &[],
    )
    .await;
    let answer = w.ask("co").await;
    assert!(
        answer
            .groups
            .iter()
            .flat_map(|g| &g.hits)
            .all(|hit| hit.files.is_none()),
        "{answer:#?}"
    );
}

/// Row 20's rule, and the way back: a searcher with no files index answers
/// as one with an index that holds nothing, and as one with none at all.
#[tokio::test]
async fn no_index_and_an_empty_one_answer_as_before() {
    let w = world(&[GATEWAY, API], &[]).await;
    let dir = tempfile::tempdir().unwrap();
    let empty = FileIndex::open(&dir.path().join("files-1")).unwrap();
    for typed in ["ledger", "payout", "settlebridge", "gate"] {
        let groups = |r: SearchResponse| serde_json::to_value(r.groups).unwrap();
        let without = groups(
            w.ask_with(typed, false, None, EverythingMatches::default())
                .await,
        );
        let none = groups(
            w.ask_with(
                typed,
                false,
                Some(FileIndex::none()),
                EverythingMatches::default(),
            )
            .await,
        );
        let held_nothing = groups(
            w.ask_with(
                typed,
                false,
                Some(empty.clone()),
                EverythingMatches::default(),
            )
            .await,
        );
        assert_eq!(without, none, "{typed}");
        assert_eq!(without, held_nothing, "{typed}");
    }
}

/// D9 point 6: a repository the statements found by its name and the files
/// found too keeps its place and is shown by its file.
#[tokio::test]
async fn a_repository_its_name_found_keeps_its_place_and_gains_its_file() {
    let w = world(
        &[
            (
                "named",
                "tidewater/settlebridge-tools",
                "",
                &[("Tool.java", "class SettleBridgeTool {}")],
                10,
            ),
            GATEWAY,
        ],
        &[],
    )
    .await;
    let answer = w.ask("settlebridge").await;
    assert_eq!(w.repos(&answer), ["named", "gateway"]);
    let named = w.hit(&answer, "named");
    assert!(named.files.is_some());
    assert_eq!(
        flat(&named.excerpts[0].segments),
        "class [SettleBridge]Tool {}"
    );
}

/// Row 11: the picker's loose name before a repository whose files only say
/// the word.
#[tokio::test]
async fn a_loose_name_comes_before_a_files_text() {
    let w = world(
        &[
            (
                "said",
                "tidewater/dashboards",
                "",
                &[("deploy/values.yaml", "stack: obsstack")],
                50,
            ),
            ("loose", "scale-platform/observability-stack", "", &[], 10),
        ],
        &[],
    )
    .await;
    assert_eq!(w.repos(&w.ask("obsstack").await), ["loose", "said"]);
}
