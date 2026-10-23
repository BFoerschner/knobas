//! Entity and room reads, and the two link writes -- stream D
//! (interfaces §2.5).
//!
//! Its `State<'_, Lifecycle>` is not an accident and is not stream D being
//! tidy: carry-over §10.6(a). `AppState` exists only once PostgreSQL is up,
//! and a `#[tauri::command]` resolves every argument *before* its body runs,
//! so a command declaring `State<'_, AppState>` is rejected by Tauri itself
//! during bring-up with the bare string `"state not managed"` -- no code for
//! the frontend to branch on. `Lifecycle` is managed at build time and is
//! always there; `lifecycle.pool()?` is the single place `not_ready` comes
//! from.
//!
//! ## Where the reads live
//!
//! The entity list read and the Builds frame's mirror reads are in the core
//! crate (`knobas_core::entity_list`, `knobas_core::builds`, #1317 and
//! #1318), beside the mini board read, with their statements and types; the
//! commands here are their shims.
//!
//! ## Layout
//!
//! One submodule per section of what was one file (#1316, contract §10.8),
//! each named for what it holds. This root re-exports every command and type
//! at `commands::entity::*`, the path each had before the split, so the
//! handler list, the mirror and every test address them unchanged. The
//! types the two core modules hold are addressed there, not here. A helper
//! one section shares with another is `pub(super)`, and each submodule takes
//! its siblings and this root's imports through `use super::*`.

use chrono::{DateTime, Utc};
use knobas_core::activity::ActivityRow;
use knobas_core::entity::EntityRef;
use knobas_core::link::{LinkEntry, LinkRow, Origin};
use knobas_core::suggest::{self, SuggestionEntry};
use knobas_sync::scheduler::AdapterRegistry;
use sqlx::{PgPool, Row};
use tauri::State;
use uuid::Uuid;

use crate::{IpcError, Lifecycle};

mod board;
mod builds;
mod capture;
mod checkouts;
mod contexts;
mod detail;
mod docs;
mod inbox;
mod link_sets;
mod links;
mod list;
mod lookups;
mod projects;
mod standup;
mod start_work;
mod suggestions;
mod viewed;

pub use board::*;
pub use builds::*;
pub use capture::*;
pub use checkouts::*;
pub use contexts::*;
pub use detail::*;
pub use docs::*;
pub use inbox::*;
pub use link_sets::*;
pub use links::*;
pub use list::*;
pub use lookups::*;
pub use projects::*;
pub use standup::*;
pub use start_work::*;
pub use suggestions::*;
pub use viewed::*;

#[cfg(test)]
mod tests {
    const MIRROR: &str = include_str!("../../../../../app/src/lib/ipc/entity.ts");

    /// Every command this module registers is invoked by that name from the
    /// mirror, and every command the mirror invokes is registered.
    ///
    /// A `#[tauri::command]` is addressed by a *string*, so a rename on one
    /// side is not a compile error anywhere -- it is a button that rejects with
    /// Tauri's own "command not found" the first time somebody presses it. The
    /// list is read off `lib.rs`'s handler barrel rather than written out here,
    /// so a command registered and never mirrored fails this too.
    #[test]
    fn the_mirror_invokes_the_commands_by_their_registered_names() {
        let barrel = include_str!("../../lib.rs");
        let registered: Vec<&str> = barrel
            .lines()
            .filter_map(|line| line.trim().strip_prefix("commands::entity::"))
            .map(|line| line.trim_end_matches(','))
            .collect();
        assert!(
            registered.len() >= 9,
            "only {} entity commands found in the handler barrel -- the parse \
             is wrong, not the barrel",
            registered.len()
        );
        for command in &registered {
            assert!(
                MIRROR.contains(&format!("\"{command}\"")),
                "{command} is registered but never invoked from entity.ts"
            );
        }
        for invoked in MIRROR.split("invoke<").skip(1) {
            let name = invoked
                .split_once('"')
                .and_then(|(_, rest)| rest.split_once('"'))
                .map(|(name, _)| name)
                .expect("every invoke names a command");
            assert!(
                registered.contains(&name),
                "entity.ts invokes {name}, which is not in the handler barrel"
            );
        }
    }

    /// `entity.ts` reads `list_entities` in reads of the ceiling the backend
    /// clamps to (#1643): a ceiling raised or lowered on one side only would
    /// have the views ask for rows that never come, or stop paging early.
    #[test]
    fn the_mirror_pages_by_the_backends_row_ceiling() {
        let declared = format!(
            "export const ROW_CEILING = {};",
            crate::commands::ROW_CEILING
        );
        assert!(
            MIRROR.contains(&declared),
            "entity.ts does not declare `{declared}`"
        );
    }

    /// `mini_board`'s saved-view argument (#1245) is the one `entity.ts` sends.
    /// Tauri reads a command's arguments by name, camel-cased, so a rename on
    /// one side alone is no compile error: the board would quietly answer the
    /// room instead of the view. Read off both sources, the way
    /// [`the_mirror_invokes_the_commands_by_their_registered_names`] reads the
    /// barrel.
    #[test]
    fn mini_board_takes_the_saved_view_the_mirror_sends() {
        let own = include_str!("board.rs");
        let signature = own
            .split_once(concat!("pub async fn ", "mini_board(")) // split, or it finds this line
            .and_then(|(_, rest)| rest.split_once(") ->"))
            .map(|(arguments, _)| arguments)
            .expect("mini_board's signature");
        assert!(
            signature.contains("saved_view: Option<String>,"),
            "mini_board takes no `saved_view`: {signature}"
        );
        let sent = MIRROR
            .split_once("invoke<MiniBoard>(\"mini_board\", {")
            .and_then(|(_, rest)| rest.split_once("});"))
            .map(|(arguments, _)| arguments)
            .expect("entity.ts invokes mini_board");
        assert!(
            sent.contains("savedView: "),
            "entity.ts sends mini_board no `savedView`: {sent}"
        );
    }

    /// `mini_board` takes a role and no source list (#1299), and `entity.ts`
    /// sends exactly that: a `sources` left on the mirror would be dropped by
    /// Tauri unread, and the board would quietly answer every source. Read
    /// off both sources, as the saved-view test above reads them.
    #[test]
    fn mini_board_takes_the_role_the_mirror_sends() {
        let own = include_str!("board.rs");
        let signature = own
            .split_once(concat!("pub async fn ", "mini_board(")) // split, or it finds this line
            .and_then(|(_, rest)| rest.split_once(") ->"))
            .map(|(arguments, _)| arguments)
            .expect("mini_board's signature");
        assert!(
            signature.contains("role: Option<String>,"),
            "mini_board takes no `role`: {signature}"
        );
        assert!(
            !signature.contains("sources"),
            "mini_board still takes `sources`: {signature}"
        );
        let sent = MIRROR
            .split_once("invoke<MiniBoard>(\"mini_board\", {")
            .and_then(|(_, rest)| rest.split_once("});"))
            .map(|(arguments, _)| arguments)
            .expect("entity.ts invokes mini_board");
        assert!(
            sent.contains("role: "),
            "entity.ts sends mini_board no `role`: {sent}"
        );
        assert!(
            !sent.contains("sources"),
            "entity.ts still sends mini_board `sources`: {sent}"
        );
    }
}
