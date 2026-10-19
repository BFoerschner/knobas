//! Every call into an external adapter has a deadline and a memory cap
//! (ADR-0028, issue #1204).
//!
//! **The deadline** is wall-clock, for the whole call: instantiating the
//! component, its own work, and every host call it waits on (a request, the
//! sink). The engine runs with epoch interruption, and a ticker thread moves
//! the epoch every [`TICK`]; at each tick running WASM yields to the
//! executor, so a component that spins forever is still a future that can
//! be dropped. [`bounded`] drops it at the deadline: the store goes with it,
//! and nothing the component did outlives the call.
//!
//! **The memory cap** bounds the linear memories and tables of one call's
//! store, all of them together. A growth past it traps the call at once, naming the cap,
//! rather than answering `-1` for the component to turn into an abort with
//! no reason in it. It is also what bounds an outgoing request body, which
//! the host buffers ([`crate::net`]).
//!
//! Both default to [`DEFAULT_DEADLINE`] and [`DEFAULT_MEMORY_MIB`]. A
//! manifest may ask for more, up to [`MAX_DEADLINE`] and [`MAX_MEMORY_MIB`];
//! install refuses one that asks above either ([`Limits::from_manifest`]).

use std::future::Future;
use std::sync::OnceLock;
use std::time::Duration;

use knobas_source::SourceError;
use wasmtime::{Config, Engine, ResourceLimiter};

/// What a call gets when the manifest asks for nothing: ten minutes. A
/// built-in's slowest call is a first sync of a large instance, page after
/// page, each request held to `knobas-http`'s 45 s budget; ten minutes is
/// room for that, and a hang still ends well inside an hour.
pub const DEFAULT_DEADLINE: Duration = Duration::from_secs(600);

/// The most a manifest may ask for: an hour.
pub const MAX_DEADLINE: Duration = Duration::from_secs(3_600);

/// What a call's store may hold when the manifest asks for nothing.
pub const DEFAULT_MEMORY_MIB: u32 = 256;

/// The most a manifest may ask for.
pub const MAX_MEMORY_MIB: u32 = 1_024;

/// How often the epoch moves, which is how often running WASM yields.
pub const TICK: Duration = Duration::from_millis(50);

const MIB: u64 = 1024 * 1024;

/// One adapter's limits, applied to every call into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The wall-clock time one call may take.
    pub deadline: Duration,
    /// What one call's store may hold in its linear memories, in MiB.
    pub memory_mib: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            deadline: DEFAULT_DEADLINE,
            memory_mib: DEFAULT_MEMORY_MIB,
        }
    }
}

impl Limits {
    /// The limits a manifest asks for, each absent one at its default.
    ///
    /// # Errors
    ///
    /// The reason, when either is zero or above the host's maximum.
    pub fn from_manifest(asked: &crate::manifest::Limits) -> Result<Self, String> {
        let mut limits = Self::default();
        if let Some(secs) = asked.deadline_secs {
            let max = MAX_DEADLINE.as_secs();
            if secs == 0 || u64::from(secs) > max {
                return Err(format!(
                    "limits.deadline_secs {secs} is outside what this knobas allows (1 to {max})"
                ));
            }
            limits.deadline = Duration::from_secs(secs.into());
        }
        if let Some(mib) = asked.memory_mib {
            if mib == 0 || mib > MAX_MEMORY_MIB {
                return Err(format!(
                    "limits.memory_mib {mib} is outside what this knobas allows (1 to \
                     {MAX_MEMORY_MIB})"
                ));
            }
            limits.memory_mib = mib;
        }
        Ok(limits)
    }

    /// The memory cap in bytes.
    #[must_use]
    pub fn memory_bytes(self) -> u64 {
        u64::from(self.memory_mib) * MIB
    }
}

/// Which limit a call passed. Its text is what the source's health line
/// reads after "was stopped at".
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Exceeded {
    #[error("its deadline of {} s", .0.as_secs())]
    Deadline(Duration),
    #[error("its memory cap of {0} MiB")]
    Memory(u32),
}

impl Exceeded {
    /// The call's failure: the run ends failed with this (ADR-0005).
    pub fn into_error(self, kind: &str) -> SourceError {
        SourceError::protocol(format!(
            "the external adapter {kind:?} was stopped at {self}"
        ))
    }
}

/// The store's limiter: the memory cap over all of its linear memories and
/// tables.
pub struct MemoryCap {
    mib: u32,
    cap: usize,
    /// Bytes the store's memories hold now, summed.
    memories: usize,
    /// Bytes the store's tables hold now, summed, at [`TABLE_ELEMENT_BYTES`]
    /// an element: a table is host memory too, and a growth the cap did not
    /// see could ask the host for more than it has.
    tables: usize,
}

/// What one table element is counted as: the widest a wasmtime table
/// element is, so the count is never under what the host holds.
const TABLE_ELEMENT_BYTES: usize = 16;

impl MemoryCap {
    /// The cap `limits` set, over a store that holds nothing yet.
    #[must_use]
    pub fn new(limits: Limits) -> Self {
        Self {
            mib: limits.memory_mib,
            cap: usize::try_from(limits.memory_bytes()).unwrap_or(usize::MAX),
            memories: 0,
            tables: 0,
        }
    }
}

impl ResourceLimiter for MemoryCap {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        let memories = (self.memories - current.min(self.memories)).saturating_add(desired);
        if memories.saturating_add(self.tables) > self.cap {
            return Err(Exceeded::Memory(self.mib).into());
        }
        self.memories = memories;
        Ok(true)
    }

    fn table_growing(
        &mut self,
        current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        let current = current.saturating_mul(TABLE_ELEMENT_BYTES);
        let desired = desired.saturating_mul(TABLE_ELEMENT_BYTES);
        let tables = (self.tables - current.min(self.tables)).saturating_add(desired);
        if tables.saturating_add(self.memories) > self.cap {
            return Err(Exceeded::Memory(self.mib).into());
        }
        self.tables = tables;
        Ok(true)
    }
}

/// The process's one engine, with epoch interruption, and the ticker that
/// moves its epoch. Every adapter compiles and runs on it.
pub(crate) fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let mut config = Config::new();
        config.epoch_interruption(true);
        // wasmtime's macOS mach-port trap handler aborts the process when its
        // `mach_msg` returns anything but success (#1257). Signal handlers, as
        // on Linux, have no such thread to die.
        config.macos_use_mach_ports(false);
        let engine = Engine::new(&config).expect("the host's engine configuration is valid");
        let ticking = engine.clone();
        std::thread::Builder::new()
            .name("knobas-wasm-epoch".into())
            .spawn(move || {
                loop {
                    std::thread::sleep(TICK);
                    ticking.increment_epoch();
                }
            })
            .expect("the epoch ticker starts");
        engine
    })
}

/// `call` into `adapter`, stopped at its deadline.
pub(crate) async fn bounded<T>(
    adapter: &crate::WasmAdapter,
    call: impl Future<Output = Result<T, SourceError>>,
) -> Result<T, SourceError> {
    let deadline = adapter.limits().deadline;
    match tokio::time::timeout(deadline, call).await {
        Ok(answer) => answer,
        Err(_) => Err(Exceeded::Deadline(deadline).into_error(adapter.kind())),
    }
}

/// The limit a trap names, if it was one.
pub(crate) fn exceeded(error: &wasmtime::Error) -> Option<Exceeded> {
    error.downcast_ref::<Exceeded>().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cap_of(mib: u32) -> MemoryCap {
        MemoryCap::new(Limits {
            memory_mib: mib,
            ..Limits::default()
        })
    }

    /// A table is host memory too: a growth past the cap traps naming it,
    /// rather than asking the host for whatever the component wants.
    #[test]
    fn a_table_growth_past_the_cap_is_refused_naming_it() {
        let mut cap = cap_of(1);
        assert!(cap.table_growing(0, 1_000, None).unwrap());
        let error = cap.table_growing(1_000, usize::MAX / 2, None).unwrap_err();
        assert_eq!(exceeded(&error), Some(Exceeded::Memory(1)));
    }

    /// Memories and tables share the one cap.
    #[test]
    fn memories_and_tables_count_against_one_cap() {
        let mut cap = cap_of(1);
        assert!(cap.memory_growing(0, 768 * 1024, None).unwrap());
        let error = cap.table_growing(0, 32 * 1024, None).unwrap_err();
        assert_eq!(exceeded(&error), Some(Exceeded::Memory(1)));
        assert!(cap.table_growing(0, 1_024, None).unwrap());
        let error = cap
            .memory_growing(768 * 1024, 1024 * 1024, None)
            .unwrap_err();
        assert_eq!(exceeded(&error), Some(Exceeded::Memory(1)));
    }
}
