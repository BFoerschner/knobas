//! A platform and a shell that record (decisions §24), behind `test-util`.
//! Every test of the controller, the commands and the quit path runs on
//! them, on Linux too.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::host::{Hooks, HostError, Rect, SiteHost, SiteView, ViewEvent, ViewSpec};
use crate::keys::{Claim, Press};
use crate::scripts::Dirty;
use crate::sites::{Shell, SiteEvent};

/// How a fake view answers the unsaved-changes probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeAnswer {
    Clean,
    Changed,
    /// Text that is neither `true` nor `false`.
    Odd,
    /// Never: the caller's timeout decides.
    Never,
}

/// One fake view's record.
#[derive(Default)]
pub struct FakeViewState {
    pub calls: Vec<String>,
    pub visible: bool,
    pub bounds: Rect,
    pub probe: Option<ProbeAnswer>,
    pub probes: usize,
    pub hooks: Option<Hooks>,
    /// Probes never answered, held so the caller's channel stays open.
    pub unanswered: Vec<Box<dyn FnOnce(Dirty) + Send>>,
}

/// A view that records every call.
pub struct FakeView {
    state: Rc<RefCell<FakeViewState>>,
}

impl SiteView for FakeView {
    fn load(&self, url: &str) {
        self.state.borrow_mut().calls.push(format!("load {url}"));
    }
    fn set_bounds(&self, bounds: Rect) {
        let mut state = self.state.borrow_mut();
        state.bounds = bounds;
    }
    fn set_visible(&self, visible: bool) {
        let mut state = self.state.borrow_mut();
        if state.visible != visible {
            state.calls.push(if visible {
                "show".into()
            } else {
                "hide".into()
            });
        }
        state.visible = visible;
    }
    fn focus(&self) {
        self.state.borrow_mut().calls.push("focus".into());
    }
    fn back(&self) {
        self.state.borrow_mut().calls.push("back".into());
    }
    fn forward(&self) {
        self.state.borrow_mut().calls.push("forward".into());
    }
    fn reload(&self) {
        self.state.borrow_mut().calls.push("reload".into());
    }
    fn stop(&self) {
        self.state.borrow_mut().calls.push("stop".into());
    }
    fn zoom(&self, level: f64) {
        self.state.borrow_mut().calls.push(format!("zoom {level}"));
    }
    fn probe(&self, answer: Box<dyn FnOnce(Dirty) + Send>) {
        let reply = {
            let mut state = self.state.borrow_mut();
            state.probes += 1;
            state.probe.unwrap_or(ProbeAnswer::Clean)
        };
        match reply {
            ProbeAnswer::Clean => answer(Dirty::from_answer("false")),
            ProbeAnswer::Changed => answer(Dirty::from_answer("true")),
            ProbeAnswer::Odd => answer(Dirty::from_answer("\"true\"")),
            ProbeAnswer::Never => self.state.borrow_mut().unanswered.push(answer),
        }
    }
    fn sleep(&self, asleep: bool) {
        let mut state = self.state.borrow_mut();
        if asleep {
            state.calls.push("sleep".into());
        }
    }
    fn poll(&self) -> Option<ViewEvent> {
        None
    }
}

impl Drop for FakeView {
    fn drop(&mut self) {
        self.state.borrow_mut().calls.push("dropped".into());
    }
}

/// A platform whose views record, kept in a map by source the tests read.
#[derive(Clone, Default)]
pub struct FakeHost {
    pub views: Rc<RefCell<HashMap<String, Rc<RefCell<FakeViewState>>>>>,
    pub calls: Rc<RefCell<Vec<String>>>,
    /// Answers each new view's probe with this.
    pub probe: Rc<RefCell<HashMap<String, ProbeAnswer>>>,
    /// `make` fails with this.
    pub fail: Rc<RefCell<Option<String>>>,
    /// `forget` answers this.
    pub forget_holds: Rc<RefCell<bool>>,
}

impl FakeHost {
    /// The record of `source`'s view.
    ///
    /// # Panics
    ///
    /// When no view was made for `source`.
    #[must_use]
    pub fn view(&self, source: &str) -> Rc<RefCell<FakeViewState>> {
        self.views
            .borrow()
            .get(source)
            .cloned()
            .expect("a view was made")
    }

    /// Set how `source`'s view answers the probe.
    pub fn answer(&self, source: &str, answer: ProbeAnswer) {
        self.probe.borrow_mut().insert(source.to_owned(), answer);
        if let Some(view) = self.views.borrow().get(source) {
            view.borrow_mut().probe = Some(answer);
        }
    }

    fn hooks(&self, source: &str) -> Hooks {
        self.view(source).borrow().hooks.clone().expect("hooks")
    }

    /// The view navigates to `url`: whether the rule let it.
    #[must_use]
    pub fn navigate(&self, source: &str, url: &str) -> bool {
        let hooks = self.hooks(source);
        (hooks.navigate)(url)
    }

    /// The view reports `event`.
    pub fn report(&self, source: &str, event: ViewEvent) {
        let hooks = self.hooks(source);
        (hooks.event)(event);
    }

    /// A key goes down in the view.
    #[must_use]
    pub fn key(&self, source: &str, press: &Press) -> Claim {
        let hooks = self.hooks(source);
        (hooks.key)(press)
    }

    /// The view starts a download.
    #[must_use]
    pub fn download(&self, source: &str, suggested: &str) -> Option<(std::path::PathBuf, u64)> {
        let hooks = self.hooks(source);
        (hooks.download)("https://x.example/f", suggested)
    }
}

impl SiteHost for FakeHost {
    fn make(&self, spec: &ViewSpec, hooks: Hooks) -> Result<Box<dyn SiteView>, HostError> {
        if let Some(error) = self.fail.borrow().clone() {
            return Err(error);
        }
        self.calls
            .borrow_mut()
            .push(format!("make {} {}", spec.source, spec.url));
        let state = Rc::new(RefCell::new(FakeViewState {
            probe: self.probe.borrow().get(&spec.source).copied(),
            hooks: Some(hooks),
            bounds: spec.bounds,
            ..FakeViewState::default()
        }));
        self.views
            .borrow_mut()
            .insert(spec.source.clone(), Rc::clone(&state));
        Ok(Box::new(FakeView { state }))
    }

    fn focus_shell(&self) {
        self.calls.borrow_mut().push("focus shell".into());
    }

    fn forget(&self, spec: &ViewSpec) -> bool {
        self.calls
            .borrow_mut()
            .push(format!("forget {}", spec.source));
        !*self.forget_holds.borrow()
    }
}

type Task = Box<dyn FnOnce() + Send>;

/// A shell on one thread: `on_main` runs at once, `later` queues until
/// [`FakeShell::drain`], `emit` records.
#[derive(Default)]
pub struct FakeShell {
    queue: Mutex<VecDeque<Task>>,
    pub events: Mutex<Vec<SiteEvent>>,
    pub quits: AtomicU64,
    pub landed: Mutex<Vec<String>>,
    clock: AtomicU64,
    pub size: Mutex<(f64, f64)>,
}

impl FakeShell {
    #[must_use]
    pub fn new() -> Self {
        Self {
            size: Mutex::new((1440.0, 868.0)),
            ..Self::default()
        }
    }

    /// Run what `later` queued, until nothing is left.
    pub fn drain(&self) {
        loop {
            let task = self.queue.lock().unwrap().pop_front();
            match task {
                Some(task) => task(),
                None => break,
            }
        }
    }

    /// Set the clock.
    pub fn set_now(&self, ms: u64) {
        self.clock.store(ms, Ordering::SeqCst);
    }

    /// The events sent so far, taken.
    pub fn take_events(&self) -> Vec<SiteEvent> {
        std::mem::take(&mut *self.events.lock().unwrap())
    }
}

impl Shell for FakeShell {
    fn on_main(&self, f: Task) {
        f();
    }
    fn later(&self, f: Task) {
        self.queue.lock().unwrap().push_back(f);
    }
    fn emit(&self, event: SiteEvent) {
        self.events.lock().unwrap().push(event);
    }
    #[allow(clippy::cast_precision_loss)]
    fn now(&self) -> f64 {
        self.clock.load(Ordering::SeqCst) as f64
    }
    fn window_size(&self) -> (f64, f64) {
        *self.size.lock().unwrap()
    }
    fn quit(&self) {
        self.quits.fetch_add(1, Ordering::SeqCst);
    }
    fn landed(&self, source: &str) {
        self.landed.lock().unwrap().push(source.to_owned());
    }
}
