//! Whether anything in a checkout changed since its working tree was last
//! read: the stamp [`super::working_tree::read_known`] and
//! [`super::uncommitted::read_known`] keep beside each answer, so a checkout
//! nothing touched answers what git said last time without running it again
//! (the scale pass on PR #2479, 2026-11-10).
//!
//! The stamp itself is [`knobas_git::stamp`] since the git-starts pass
//! (2026-11-16): it was Unix's alone, so on Windows every view of a
//! repository started `git status`. There it reads Windows's own file
//! metadata, and it sits in the crate with no native dependency so that half
//! is compiled and tested on Windows in CI.

pub use knobas_git::stamp::{of, tracked_paths};
