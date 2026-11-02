//! Which mirrored kinds the launcher answers with, and under which name
//! (#2422, spec #2419).
//!
//! The box lists what a reader looks for, not what they reach from it: a
//! **pipeline** is found, a **run** is reached from its pipeline, and a
//! **commit** from its repository (spec #2419, story 16). A Jira board's or
//! filter's mirror row is left out too: the reader finds it as the `view`
//! its ticket-view row is (`crate::corpus::VIEW`), and both rows would be
//! the same view twice.
//!
//! Which kinds those are is the adapters' to say, through the Builds frame
//! each kind declares (ADR-0022): a kind with frame `pipeline` is answered as
//! the shell's kind `pipeline` whatever the source calls it (TeamCity's
//! `build_config`), and a kind with frame `run` or `saved_view` is left out.
//! `commit` is the code host role's own word, so it is named here.

use knobas_core::payload::Declarations;

/// The kind a hit of any pipeline-framed kind is answered as.
pub const PIPELINE: &str = "pipeline";

/// Kinds left out of every launcher answer whatever their frame.
const ALWAYS_LEFT_OUT: &[&str] = &["commit"];

/// What the declarations say about the launcher's kinds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LauncherFrames {
    /// Every kind declaring frame `pipeline`, sorted and distinct.
    pub pipelines: Vec<String>,
    /// Every kind the launcher leaves out, sorted and distinct.
    pub left_out: Vec<String>,
}

impl LauncherFrames {
    /// Read off every source's declarations.
    #[must_use]
    pub fn of(declarations: &Declarations) -> Self {
        let mut frames = Self {
            pipelines: Vec::new(),
            left_out: ALWAYS_LEFT_OUT.iter().map(|k| (*k).to_owned()).collect(),
        };
        let json = declarations.as_json();
        for kinds in json.as_object().into_iter().flat_map(|s| s.values()) {
            for (kind, paths) in kinds.as_object().into_iter().flatten() {
                match paths.get("frame").and_then(serde_json::Value::as_str) {
                    Some("pipeline") => frames.pipelines.push(kind.clone()),
                    Some("run" | "saved_view") => frames.left_out.push(kind.clone()),
                    _ => {}
                }
            }
        }
        for list in [&mut frames.pipelines, &mut frames.left_out] {
            list.sort();
            list.dedup();
        }
        frames
    }

    /// The kind `kind` is answered as.
    #[must_use]
    pub fn answered_as<'a>(&self, kind: &'a str) -> &'a str {
        if self.pipelines.iter().any(|p| p == kind) {
            PIPELINE
        } else {
            kind
        }
    }

    /// Whether the launcher leaves `kind` out.
    #[must_use]
    pub fn leaves_out(&self, kind: &str) -> bool {
        self.left_out.iter().any(|k| k == kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_core::payload::{Frame, KindPaths};

    fn framed(kind: &str, frame: Option<Frame>) -> KindPaths {
        KindPaths {
            kind: kind.to_owned(),
            frame,
            ..KindPaths::default()
        }
    }

    #[test]
    fn the_frames_name_the_pipelines_and_what_is_left_out() {
        let declarations = Declarations::empty()
            .with(
                "tc",
                vec![
                    framed("build_config", Some(Frame::Pipeline)),
                    framed("build", Some(Frame::Run)),
                ],
            )
            .with(
                "jira",
                vec![
                    framed("ticket", None),
                    framed("jira_view", Some(Frame::SavedView)),
                ],
            );
        let frames = LauncherFrames::of(&declarations);
        assert_eq!(frames.pipelines, ["build_config"]);
        assert_eq!(frames.left_out, ["build", "commit", "jira_view"]);
        assert_eq!(frames.answered_as("build_config"), "pipeline");
        assert_eq!(frames.answered_as("ticket"), "ticket");
        assert!(frames.leaves_out("commit"));
        assert!(!frames.leaves_out("ticket"));
    }
}
