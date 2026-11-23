//! `tauri::ipc::Channel<SyncProgress>` as a `knobas_sync::progress::ProgressSink`.
//!
//! The whole reason `knobas-sync` defines a trait instead of taking a Channel:
//! the engine has no business linking Tauri, and a test needs to be able to
//! record progress without a webview.
//!
//! **Sent from the main thread**, as every event is (`crate::events::emit`).
//! With Tauri's `tracing` feature (#747) a channel's send is a script the
//! webview evaluates *synchronously*: the calling thread waits for the main
//! thread. A sync reports from its worker, so a quit, which waits on the main
//! thread for the sync, and the sync, which waited on the main thread for its
//! report, held each other for good (the final suite on Windows, 2026-11-22:
//! knobas never quit). So each send is handed to the main thread, and so is
//! the channel's last drop, which sends the page its `{ end: true }`.

use knobas_sync::progress::{ProgressSink, SyncProgress};
use tauri::ipc::Channel;

/// What runs a task on the main thread: `AppHandle::run_on_main_thread`, or a
/// test's queue.
type OnMain = Box<dyn Fn(Box<dyn FnOnce() + Send>) + Send + Sync>;

pub struct ChannelSink {
    /// `None` only once dropped.
    channel: Option<Channel<SyncProgress>>,
    on_main: OnMain,
}

impl ChannelSink {
    /// `channel`, sent on from the main thread of `app`.
    #[must_use]
    pub fn new<R: tauri::Runtime>(
        app: &tauri::AppHandle<R>,
        channel: Channel<SyncProgress>,
    ) -> ChannelSink {
        let app = app.clone();
        ChannelSink::over(
            Box::new(move |task| {
                if let Err(error) = app.run_on_main_thread(task) {
                    tracing::debug!(%error, "the main thread is gone: progress not sent");
                }
            }),
            channel,
        )
    }

    /// `channel`, with `on_main` running each send and the last drop.
    #[must_use]
    pub fn over(on_main: OnMain, channel: Channel<SyncProgress>) -> ChannelSink {
        ChannelSink {
            channel: Some(channel),
            on_main,
        }
    }
}

impl ProgressSink for ChannelSink {
    fn report(&self, progress: SyncProgress) {
        let Some(channel) = self.channel.clone() else {
            return;
        };
        (self.on_main)(Box::new(move || {
            // Best-effort: a closed channel means the window that asked for
            // progress is gone, which must never fail the sync it is watching.
            if let Err(error) = channel.send(progress) {
                tracing::debug!(%error, "the progress channel is closed");
            }
        }));
    }
}

impl Drop for ChannelSink {
    fn drop(&mut self) {
        if let Some(channel) = self.channel.take() {
            (self.on_main)(Box::new(move || drop(channel)));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::thread::{self, ThreadId};

    use knobas_sync::progress::SyncPhase;

    use super::*;

    /// The main thread's tasks, run by the test when it says.
    type Queue = Arc<Mutex<Vec<Box<dyn FnOnce() + Send>>>>;

    /// Records the thread it is dropped on.
    struct Guard(Arc<Mutex<Option<ThreadId>>>);
    impl Drop for Guard {
        fn drop(&mut self) {
            *self.0.lock().unwrap() = Some(thread::current().id());
        }
    }

    fn progress() -> SyncProgress {
        SyncProgress {
            run_id: 1,
            source_id: "s".into(),
            phase: SyncPhase::Fetching,
            items: 3,
            elapsed_ms: 10,
            message: None,
            walked: None,
            of: None,
            unit: None,
        }
    }

    /// A sink dropped before it reported anything still lets its channel go
    /// on the main thread: no report holds a copy of it (the ux30 end gate's
    /// mutants: the drop handed to nobody passed the test below, whose reports
    /// keep the channel alive until the main thread runs them).
    #[test]
    fn a_sink_that_never_reported_lets_its_channel_go_on_the_main_thread() {
        let dropped: Arc<Mutex<Option<ThreadId>>> = Arc::default();
        let guard = Guard(Arc::clone(&dropped));
        let channel = Channel::<SyncProgress>::new(move |_body| {
            let _ = &guard;
            Ok(())
        });
        let queue: Queue = Arc::default();
        let into = Arc::clone(&queue);
        let sink = ChannelSink::over(
            Box::new(move |task| into.lock().unwrap().push(task)),
            channel,
        );
        thread::spawn(move || drop(sink)).join().unwrap();
        assert!(
            dropped.lock().unwrap().is_none(),
            "the worker let the channel go itself: {:?}",
            dropped.lock().unwrap()
        );
        let tasks = std::mem::take(&mut *queue.lock().unwrap());
        assert_eq!(tasks.len(), 1, "the drop is handed to the main thread");
        for task in tasks {
            task();
        }
        assert_eq!(*dropped.lock().unwrap(), Some(thread::current().id()));
    }

    /// A sync reports from its worker; the channel hears it on the main thread
    /// only, and is let go there too (its last drop is the page's `end`).
    #[test]
    fn a_workers_reports_and_the_channels_end_reach_the_main_thread() {
        let sent: Arc<Mutex<Vec<ThreadId>>> = Arc::default();
        let dropped: Arc<Mutex<Option<ThreadId>>> = Arc::default();
        let guard = Guard(Arc::clone(&dropped));
        let heard = Arc::clone(&sent);
        let channel = Channel::<SyncProgress>::new(move |_body| {
            let _ = &guard;
            heard.lock().unwrap().push(thread::current().id());
            Ok(())
        });
        let queue: Queue = Arc::default();
        let into = Arc::clone(&queue);
        let sink = ChannelSink::over(
            Box::new(move |task| into.lock().unwrap().push(task)),
            channel,
        );
        thread::spawn(move || {
            sink.report(progress());
            sink.report(progress());
            drop(sink);
        })
        .join()
        .unwrap();
        assert!(
            sent.lock().unwrap().is_empty() && dropped.lock().unwrap().is_none(),
            "the worker reached the channel itself: sends {:?}, dropped on {:?}",
            sent.lock().unwrap(),
            dropped.lock().unwrap()
        );
        let tasks = std::mem::take(&mut *queue.lock().unwrap());
        for task in tasks {
            task();
        }
        let main = thread::current().id();
        assert_eq!(*sent.lock().unwrap(), vec![main, main]);
        assert_eq!(*dropped.lock().unwrap(), Some(main));
    }
}
