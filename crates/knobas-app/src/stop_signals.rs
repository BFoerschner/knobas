//! A stop signal is a quit (the ux30 end gate, 2026-11-23).
//!
//! The exit handler (`on_run_event` in `lib.rs`) stops the embedded
//! PostgreSQL on Tauri's two exit events, and a signal raises neither. So a
//! `SIGTERM` from a logout on Linux, a session manager, `systemctl --user
//! stop`, a package manager replacing the app or a plain `kill`, a `SIGHUP`
//! from a closed terminal and a `SIGINT` from Ctrl-C each ended the process
//! and left its postmaster running: the port held and the data directory in
//! use until the next start adopted the server (`knobas_db::EmbeddedDb::start`).
//!
//! [`listen`] turns the first of them into the quit the app already knows.
//! The app's own caller asks the event loop to exit (`AppHandle::exit`), which
//! raises `ExitRequested` and `Exit` and so runs the whole exit handler, the
//! database last; the windowless backend runs the same handler directly. A
//! second signal while that quit runs ends the process at once, so a quit that
//! hangs can still be ended the usual way; the server's supervisor
//! (`knobas_db::supervisor`) then stops it.
//!
//! A signal is not the only end that runs no quit: on Linux the display's end
//! (a logout) ends the process from inside GTK, before or during a quit. The
//! supervisor covers that too.
//!
//! Unix only. Windows sends a GUI process no signal: a log-off or shutdown
//! reaches it as window messages, and a `taskkill /F` cannot be caught.

/// The signals [`listen`] answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopSignal {
    /// `SIGTERM`: a logout, a service manager, a package manager, `kill`.
    Term,
    /// `SIGINT`: Ctrl-C in the terminal that started knobas.
    Int,
    /// `SIGHUP`: that terminal closed.
    Hup,
}

impl StopSignal {
    /// The signal's number, for the exit status a second one ends with
    /// (128 plus it, as a shell reports a death by signal).
    #[must_use]
    pub fn number(self) -> i32 {
        match self {
            Self::Hup => 1,
            Self::Int => 2,
            Self::Term => 15,
        }
    }

    /// Its name, for the log.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Term => "SIGTERM",
            Self::Int => "SIGINT",
            Self::Hup => "SIGHUP",
        }
    }
}

/// Answer the first `SIGTERM`, `SIGINT` or `SIGHUP` with `quit`, on Tauri's
/// async runtime; a later one ends the process at once.
///
/// The handlers are in place when this returns, so a signal sent after it is
/// never the default one that leaves the server behind.
///
/// # Errors
///
/// When the runtime cannot register a handler. Nothing is listening then, and
/// a signal ends the process as before.
#[cfg(unix)]
pub fn listen(quit: impl FnOnce(StopSignal) + Send + 'static) -> std::io::Result<()> {
    use tokio::signal::unix::{SignalKind, signal};

    let runtime = tauri::async_runtime::handle();
    // Registered here, in the runtime's context, not inside the task: a
    // signal between this return and the task's first poll is then queued
    // for the task rather than delivered to the default handler.
    let _context = runtime.inner().enter();
    let mut term = signal(SignalKind::terminate())?;
    let mut int = signal(SignalKind::interrupt())?;
    let mut hup = signal(SignalKind::hangup())?;
    runtime.spawn(async move {
        let mut quit = Some(quit);
        loop {
            let signal = tokio::select! {
                Some(()) = term.recv() => StopSignal::Term,
                Some(()) = int.recv() => StopSignal::Int,
                Some(()) = hup.recv() => StopSignal::Hup,
                else => return,
            };
            if let Some(quit) = quit.take() {
                tracing::info!(
                    signal = signal.name(),
                    "a stop signal: quitting as the app's quit does"
                );
                // Off the runtime's worker: the windowless backend's quit
                // blocks on this same runtime.
                std::thread::spawn(move || quit(signal));
            } else {
                tracing::warn!(
                    signal = signal.name(),
                    "a second stop signal during the quit: ending now"
                );
                std::process::exit(128 + signal.number());
            }
        }
    });
    Ok(())
}

/// Nothing to listen for: see the module's last paragraph.
///
/// # Errors
///
/// Never.
#[cfg(not(unix))]
pub fn listen(_quit: impl FnOnce(StopSignal) + Send + 'static) -> std::io::Result<()> {
    Ok(())
}
