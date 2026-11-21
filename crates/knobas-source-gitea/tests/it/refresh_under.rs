//! A refresh under a repository (ADR-0047, `Source::refresh_under`): the
//! run's walk of that one repository from where the stored position says it
//! stood, its own part of the position moved as the run would move it, and
//! every other repository left alone. Against the docker-free fake.

use knobas_source::contract::VecSink;
use knobas_source::{Source, SyncItem, UnderAnswer};
use serde_json::{Value, json};

use crate::support::{Fake, State, branch, commit, source};

const REPO: &str = "gitea:tidewater/payout-service";
const MAIN: &str = "gitea:tidewater/payout-service@refs/heads/main";
const FEATURE: &str = "gitea:tidewater/payout-service@refs/heads/feature/PAY-231-sepa-retry";
const COMMIT: &str = "gitea:tidewater/payout-service@1111111111111111111111111111111111111111";
const ELSEWHERE: &str = "elsewhere/unrelated";
const NEW_HEAD: &str = "2222222222222222222222222222222222222222";

async fn full(source: &dyn Source) -> String {
    let mut sink = VecSink(Vec::new());
    source.sync(None, &mut sink).await.expect("full sync")
}

async fn again(source: &dyn Source, cursor: &str) -> (Vec<SyncItem>, String) {
    let mut sink = VecSink(Vec::new());
    let next = source
        .sync(Some(cursor.to_owned()), &mut sink)
        .await
        .expect("incremental sync");
    (sink.0, next)
}

async fn walked(source: &dyn Source, id: &str, cursor: &str) -> (Vec<SyncItem>, Option<String>) {
    match source
        .refresh_under(id, Some(cursor))
        .await
        .expect("the walk succeeds")
    {
        UnderAnswer::Walked { items, cursor } => (items, cursor),
        UnderAnswer::NotOffered => panic!("{id} was not walked"),
    }
}

fn position(cursor: &str) -> Value {
    serde_json::from_str(cursor).expect("a Gitea cursor is JSON")
}

fn ids(items: &[SyncItem]) -> Vec<String> {
    let mut out: Vec<String> = items.iter().map(|item| item.entity.to_string()).collect();
    out.sort();
    out
}

/// `main` of payout-service moved to a new commit, and `elsewhere`'s too.
fn pushed(state: &mut State) {
    let full = "tidewater/payout-service";
    let main = state
        .branches
        .get_mut(full)
        .unwrap()
        .iter_mut()
        .find(|b| b["name"] == "main")
        .unwrap();
    *main = branch(
        "main",
        NEW_HEAD,
        "PAY-240 payout CSV",
        "2026-08-23T09:00:00Z",
    );
    state
        .commits
        .get_mut(&format!("{full}@main"))
        .unwrap()
        .insert(
            0,
            commit(NEW_HEAD, "PAY-240 payout CSV", "2026-08-23T09:00:00Z"),
        );
    state.branches.insert(
        ELSEWHERE.to_owned(),
        vec![branch(
            "main",
            "8888888888888888888888888888888888888888",
            "moved",
            "2026-08-23T09:00:00Z",
        )],
    );
}

/// The case the walk exists for: a push on the repository on screen is in
/// the mirror in one repository's reads, and nothing of another repository
/// is read or written. The next run then finds nothing more of this
/// repository: its position moved with its rows.
#[tokio::test]
async fn a_push_walks_its_repository_alone_and_moves_only_its_position() {
    let mut state = State::tidewater().with_elsewhere();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), json!({}));
    let cursor = full(&*source).await;

    pushed(&mut state);
    fake.remount(&state).await;

    let (items, moved) = walked(&*source, REPO, &cursor).await;
    assert_eq!(
        ids(&items),
        vec![
            format!("gitea:tidewater/payout-service@{NEW_HEAD}"),
            MAIN.to_owned()
        ],
        "the moved branch and its new commit, and nothing else"
    );
    assert!(
        fake.paths()
            .await
            .iter()
            .all(|path| !path.contains(ELSEWHERE)),
        "the walk read another repository"
    );
    let moved = moved.expect("a walk that emitted something moves the position");
    let (before, after) = (position(&cursor), position(&moved));
    assert_eq!(
        after["repos"][ELSEWHERE], before["repos"][ELSEWHERE],
        "another repository's position moved"
    );
    assert_eq!(after["repos_listed_at"], before["repos_listed_at"]);
    assert_eq!(
        after["repos"]["tidewater/payout-service"]["branches"]["main"],
        json!(NEW_HEAD)
    );

    let (next, _) = again(&*source, &moved).await;
    assert!(
        next.iter()
            .all(|item| !item.entity.key.starts_with("tidewater/")),
        "the run after the walk emitted the walked repository again: {:?}",
        ids(&next)
    );
    assert_eq!(
        ids(&next),
        vec!["gitea:elsewhere/unrelated@refs/heads/main".to_owned()],
        "the run still finds what moved elsewhere"
    );
}

/// Fable's ruling 3: the change gate compares the forge with the position,
/// so a branch the walk tombstoned and the forge then restored at the very
/// same head comes back on the next run only because the walk moved the
/// position. With the position left behind, the run would compare the
/// restored branch with a position that still holds it and emit nothing,
/// and the row would stay tombstoned.
#[tokio::test]
async fn a_branch_the_walk_tombstoned_and_the_forge_restored_comes_back_on_the_run() {
    let mut state = State::tidewater();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), json!({}));
    let cursor = full(&*source).await;

    let restored = state.branches["tidewater/payout-service"].clone();
    state
        .branches
        .get_mut("tidewater/payout-service")
        .unwrap()
        .retain(|b| b["name"] != "feature/PAY-231-sepa-retry");
    fake.remount(&state).await;
    let (items, moved) = walked(&*source, REPO, &cursor).await;
    let gone: Vec<&SyncItem> = items.iter().filter(|item| item.deleted).collect();
    assert_eq!(gone.len(), 1, "{:?}", ids(&items));
    assert_eq!(gone[0].entity.to_string(), FEATURE);
    let moved = moved.expect("the tombstone moves the position");

    state
        .branches
        .insert("tidewater/payout-service".to_owned(), restored);
    fake.remount(&state).await;
    let (next, _) = again(&*source, &moved).await;
    let back = next
        .iter()
        .find(|item| item.entity.to_string() == FEATURE)
        .expect("the restored branch comes back on the run");
    assert!(!back.deleted);
}

/// Clause 2's analog, held by the battery too: on a source nothing changed
/// since the run, the walk reads the repository, emits nothing and leaves
/// the position, for the repository, a branch and a commit alike.
#[tokio::test]
async fn a_walk_of_an_unchanged_repository_emits_nothing_and_moves_nothing() {
    let state = State::tidewater();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), json!({}));
    let cursor = full(&*source).await;
    for id in [REPO, MAIN, COMMIT] {
        let (items, moved) = walked(&*source, id, &cursor).await;
        assert!(items.is_empty(), "{id}: {:?}", ids(&items));
        assert_eq!(moved, None, "{id}");
    }
}

/// A branch's and a commit's refresh is their repository's walk.
#[tokio::test]
async fn a_branch_and_a_commit_walk_their_repository() {
    let mut state = State::tidewater();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), json!({}));
    let cursor = full(&*source).await;
    pushed(&mut state);
    fake.remount(&state).await;
    for id in [FEATURE, COMMIT] {
        let (items, moved) = walked(&*source, id, &cursor).await;
        assert!(
            ids(&items).contains(&MAIN.to_owned()),
            "{id}: {:?}",
            ids(&items)
        );
        assert!(moved.is_some(), "{id}");
    }
}

/// No position, no walk: a cursor that does not parse, or one naming no
/// repository, owes a full sync and its sweep, which a position naming one
/// repository would cancel. The app runs the source instead. A pull request
/// and an issue are `refresh`'s, never walked.
#[tokio::test]
async fn without_a_position_or_for_a_pull_request_the_walk_is_not_offered() {
    let state = State::tidewater();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), json!({}));
    let cursor = full(&*source).await;
    let blank = json!({ "v": position(&cursor)["v"], "repos": {} }).to_string();
    for (id, cursor) in [
        (REPO, None),
        (REPO, Some("not a cursor")),
        (REPO, Some(blank.as_str())),
        ("gitea:tidewater/payout-service#142", Some(cursor.as_str())),
        (
            "gitea:tidewater/payout-service/issues/7",
            Some(cursor.as_str()),
        ),
    ] {
        assert!(
            matches!(
                source.refresh_under(id, cursor).await,
                Ok(UnderAnswer::NotOffered)
            ),
            "{id} with {cursor:?}"
        );
    }
}

/// A repository whose own read answers 404 is its tombstone, and its entry
/// leaves the position, so one that comes back is walked from blank.
#[tokio::test]
async fn a_repository_gitea_answers_404_is_its_tombstone_and_leaves_the_position() {
    let mut state = State::tidewater().with_elsewhere();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), json!({}));
    let cursor = full(&*source).await;

    state.missing.insert("tidewater/payout-service".to_owned());
    fake.remount(&state).await;
    let (items, moved) = walked(&*source, REPO, &cursor).await;
    assert_eq!(ids(&items), vec![REPO.to_owned()]);
    assert!(items[0].deleted);
    let after = position(&moved.expect("the entry leaves the position"));
    assert!(after["repos"].get("tidewater/payout-service").is_none());
    assert_eq!(
        after["repos"][ELSEWHERE],
        position(&cursor)["repos"][ELSEWHERE]
    );
}
