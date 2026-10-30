//! User command renderers (#2262, spec #2234 step 5, ADR-0033) at the IPC
//! seam: saved to a real `knobas.setting`, then run by id the way
//! `run_renderer` runs them, the real programs on this machine.

use std::time::{Duration, Instant};

use knobas_app::renderers::{
    self, CommandOutput, RenderContent, RenderRun, RendererInput, UserRendererDraft,
};
use sqlx::PgPool;

async fn scratch(name: &str) -> PgPool {
    knobas_db::test_util::scratch_database(name)
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database")
}

fn draft(name: &str, command: &str, output: CommandOutput) -> UserRendererDraft {
    UserRendererDraft {
        id: None,
        name: name.to_owned(),
        command: command.to_owned(),
        patterns: vec!["*.puml".to_owned()],
        fences: vec!["PlantUML".to_owned()],
        input: RendererInput::Text,
        output,
        timeout_secs: None,
    }
}

fn text(content: &str) -> RenderContent {
    RenderContent::Text {
        text: content.to_owned(),
    }
}

/// Saved renderers come back trimmed, with ids from their names, fence
/// languages lowercase, and the default timeout; a second read is the same.
#[tokio::test]
async fn saved_renderers_read_back_with_ids_and_lowercase_fences() {
    let pool = scratch("renderer_commands_round_trip").await;
    assert!(renderers::list(&pool).await.unwrap().is_empty());

    let saved = renderers::save(
        &pool,
        &[
            draft("  PlantUML ", "plantuml -tsvg -pipe", CommandOutput::Svg),
            draft("PlantUML", "cat", CommandOutput::Html),
        ],
    )
    .await
    .unwrap();

    assert_eq!(saved[0].id, "plantuml");
    assert_eq!(saved[0].name, "PlantUML");
    assert_eq!(saved[0].fences, ["plantuml"]);
    assert_eq!(saved[0].timeout_secs, renderers::DEFAULT_TIMEOUT_SECS);
    assert_eq!(saved[1].id, "plantuml-2", "two renderers never share an id");
    assert_eq!(renderers::list(&pool).await.unwrap(), saved);
}

/// A command with an unclosed quote, or none at all, is refused by name.
#[tokio::test]
async fn a_command_that_cannot_run_is_refused_on_save() {
    let pool = scratch("renderer_commands_refused").await;
    let error = renderers::save(&pool, &[draft("Bad", "cat 'x", CommandOutput::Html)])
        .await
        .unwrap_err();
    assert!(error.message.contains("Bad"), "{}", error.message);
    let error = renderers::save(&pool, &[draft("Empty", "   ", CommandOutput::Html)])
        .await
        .unwrap_err();
    assert!(error.message.contains("Empty"), "{}", error.message);
}

/// `cat` with HTML output hands back exactly what went in on stdin, the
/// `<script>` too: sanitizing is the frame's job, not the command's.
#[tokio::test]
async fn cat_hands_back_its_stdin_as_html() {
    let pool = scratch("renderer_commands_cat").await;
    let saved = renderers::save(&pool, &[draft("Cat", "cat", CommandOutput::Html)])
        .await
        .unwrap();
    let html = "<p>hello</p><script>parent.document.title = 'owned'</script>";

    let run = renderers::run(&pool, &saved[0].id, text(html))
        .await
        .unwrap();

    assert_eq!(
        run,
        RenderRun::Drawn {
            output: CommandOutput::Html,
            body: html.to_owned()
        }
    );
}

/// The command is an argument vector with no shell: `;`, `|`, `$HOME` and a
/// backtick reach `printf` as literal text. Through `sh -c` the `;` would
/// end the command and `$HOME` would be the home directory.
#[tokio::test]
async fn the_command_is_never_passed_through_a_shell() {
    let pool = scratch("renderer_commands_no_shell").await;
    let saved = renderers::save(
        &pool,
        &[draft(
            "Meta",
            "printf %s a;echo|b $HOME `id`",
            CommandOutput::Html,
        )],
    )
    .await
    .unwrap();

    let run = renderers::run(&pool, &saved[0].id, text("")).await.unwrap();

    assert_eq!(
        run,
        RenderRun::Drawn {
            output: CommandOutput::Html,
            body: "a;echo|b$HOME`id`".to_owned()
        }
    );
}

/// A command slower than its timeout is stopped at the timeout and says so.
#[tokio::test]
async fn a_renderer_past_its_timeout_is_stopped_and_says_so() {
    let pool = scratch("renderer_commands_timeout").await;
    let mut slow = draft("Slow", "sleep 8", CommandOutput::Svg);
    slow.timeout_secs = Some(1);
    let saved = renderers::save(&pool, &[slow]).await.unwrap();

    let started = Instant::now();
    let run = renderers::run(&pool, &saved[0].id, text("x"))
        .await
        .unwrap();

    assert!(
        matches!(run, RenderRun::TimedOut { seconds: 1, .. }),
        "{run:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "{:?}",
        started.elapsed()
    );
}

/// A command that exits non-zero answers its status and its stderr.
#[tokio::test]
async fn a_failing_renderer_answers_its_stderr() {
    let pool = scratch("renderer_commands_stderr").await;
    let saved = renderers::save(
        &pool,
        &[draft(
            "Broken",
            "sh -c \"echo 'Syntax error on line 3' >&2; exit 3\"",
            CommandOutput::Svg,
        )],
    )
    .await
    .unwrap();

    let run = renderers::run(&pool, &saved[0].id, text("@startuml"))
        .await
        .unwrap();

    assert_eq!(
        run,
        RenderRun::Failed {
            status: Some(3),
            stderr: "Syntax error on line 3\n".to_owned()
        }
    );
}

/// A program that is not there is a run that did not start, not an error.
#[tokio::test]
async fn a_missing_program_did_not_start() {
    let pool = scratch("renderer_commands_missing").await;
    let saved = renderers::save(
        &pool,
        &[draft(
            "Gone",
            "knobas-no-such-renderer-2262",
            CommandOutput::Svg,
        )],
    )
    .await
    .unwrap();

    let run = renderers::run(&pool, &saved[0].id, text("x"))
        .await
        .unwrap();

    assert!(matches!(run, RenderRun::NotStarted { .. }), "{run:?}");
}

/// A bytes renderer gets the bytes on stdin; an image renderer's output comes
/// back as a data URL of its type.
#[tokio::test]
async fn bytes_in_and_an_image_out() {
    let pool = scratch("renderer_commands_bytes").await;
    let mut image = draft("Png", "cat", CommandOutput::Image);
    image.input = RendererInput::Bytes;
    let saved = renderers::save(&pool, &[image]).await.unwrap();
    // A PNG signature and four more bytes, base64.
    let png = "iVBORw0KGgoBAgME";

    let run = renderers::run(
        &pool,
        &saved[0].id,
        RenderContent::Bytes {
            base64: png.to_owned(),
        },
    )
    .await
    .unwrap();

    assert_eq!(
        run,
        RenderRun::Drawn {
            output: CommandOutput::Image,
            body: format!("data:image/png;base64,{png}")
        }
    );
}

/// An id that is not saved is refused by name.
#[tokio::test]
async fn an_unknown_renderer_is_refused() {
    let pool = scratch("renderer_commands_unknown").await;
    let error = renderers::run(&pool, "nope", text("x")).await.unwrap_err();
    assert!(error.message.contains("nope"), "{}", error.message);
}
