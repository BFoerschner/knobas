//! The schedule's core on a hand-moved clock with scripted sides: when it
//! reads, what stops a side, what it tells, and one schedule per scope.

use super::*;

/// A clock moved by hand.
struct HandClock(watch::Sender<Duration>);

impl HandClock {
    fn new() -> Arc<Self> {
        Arc::new(Self(watch::channel(Duration::ZERO).0))
    }
    fn set(&self, at: Duration) {
        self.0.send_replace(at);
    }
}

impl Clock for HandClock {
    fn now(&self) -> Duration {
        *self.0.borrow()
    }
    fn at(&self, at: Duration) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let mut now = self.0.subscribe();
        Box::pin(async move {
            let _ = now.wait_for(|now| *now >= at).await;
        })
    }
}

type Seen = Arc<Mutex<Vec<(SideKind, Duration)>>>;

/// A side that answers its script, one answer per read (then `Quiet`), and
/// notes when it was read.
struct Scripted {
    kind: SideKind,
    script: VecDeque<Look>,
    clock: Arc<HandClock>,
    seen: Seen,
}

#[async_trait]
impl Side for Scripted {
    fn kind(&self) -> SideKind {
        self.kind
    }
    async fn look(&mut self) -> Look {
        self.seen
            .lock()
            .unwrap()
            .push((self.kind, self.clock.now()));
        self.script.pop_front().unwrap_or(Look::Quiet)
    }
}

fn side(kind: SideKind, script: &[Look], clock: &Arc<HandClock>, seen: &Seen) -> Box<dyn Side> {
    Box::new(Scripted {
        kind,
        script: script.iter().copied().collect(),
        clock: Arc::clone(clock),
        seen: Arc::clone(seen),
    })
}

type Told = Arc<Mutex<Vec<Vec<SideKind>>>>;

struct Running {
    clock: Arc<HandClock>,
    seen: Seen,
    told: Told,
    stop: watch::Sender<bool>,
    closed: Arc<std::sync::atomic::AtomicBool>,
    task: tokio::task::JoinHandle<bool>,
}

fn start(sides: impl FnOnce(&Arc<HandClock>, &Seen) -> Vec<Box<dyn Side>>) -> Running {
    let clock = HandClock::new();
    let seen = Seen::default();
    let told = Told::default();
    let (stop, stopped) = watch::channel(false);
    let closed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let sides = sides(&clock, &seen);
    let (c, t, q) = (Arc::clone(&clock), Arc::clone(&told), Arc::clone(&closed));
    let task = tokio::spawn(async move {
        let closed = move || q.load(Ordering::SeqCst);
        let tell = move |left: &[SideKind]| t.lock().unwrap().push(left.to_vec());
        run(c.as_ref(), Duration::ZERO, sides, stopped, &closed, &tell).await
    });
    Running {
        clock,
        seen,
        told,
        stop,
        closed,
        task,
    }
}

/// Let the schedule's task run what the clock let through.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(60)).await;
}

const S: fn(u64) -> Duration = Duration::from_secs;

/// Move the clock to each of `at`, settling after each.
async fn move_to(clock: &HandClock, at: &[u64]) {
    for &secs in at {
        clock.set(S(secs));
        settle().await;
    }
}

#[tokio::test]
async fn a_quiet_side_is_read_at_5_15_30_60_and_120_seconds_and_then_the_schedule_ends() {
    let running = start(|clock, seen| vec![side(SideKind::Code, &[], clock, seen)]);
    move_to(&running.clock, &[4]).await;
    assert!(
        running.seen.lock().unwrap().is_empty(),
        "nothing before 5 s"
    );
    move_to(&running.clock, &[5, 14, 15, 29, 30, 59, 60, 119]).await;
    assert!(!running.task.is_finished(), "still looking before 120 s");
    move_to(&running.clock, &[120]).await;
    let seen: Vec<Duration> = running
        .seen
        .lock()
        .unwrap()
        .iter()
        .map(|(_, at)| *at)
        .collect();
    assert_eq!(seen, [S(5), S(15), S(30), S(60), S(120)]);
    assert!(
        running.task.await.expect("the schedule ends"),
        "it ended by itself"
    );
    assert_eq!(
        *running.told.lock().unwrap(),
        [Vec::<SideKind>::new()],
        "its end is told once"
    );
}

#[tokio::test]
async fn a_side_that_moved_stops_and_the_other_goes_on_to_the_end() {
    let running = start(|clock, seen| {
        vec![
            side(SideKind::Code, &[Look::Quiet, Look::Moved], clock, seen),
            side(SideKind::Builds, &[], clock, seen),
        ]
    });
    move_to(&running.clock, &[5, 15]).await;
    assert_eq!(
        *running.told.lock().unwrap(),
        [vec![SideKind::Builds]],
        "the code side stopped at 15 s"
    );
    move_to(&running.clock, &[30, 60, 120]).await;
    let code = running
        .seen
        .lock()
        .unwrap()
        .iter()
        .filter(|(kind, _)| *kind == SideKind::Code)
        .count();
    let builds: Vec<Duration> = running
        .seen
        .lock()
        .unwrap()
        .iter()
        .filter(|(kind, _)| *kind == SideKind::Builds)
        .map(|(_, at)| *at)
        .collect();
    assert_eq!(code, 2, "the code side is read no more once it moved");
    assert_eq!(builds, [S(5), S(15), S(30), S(60), S(120)]);
    assert!(running.task.await.unwrap());
    assert_eq!(
        *running.told.lock().unwrap(),
        [vec![SideKind::Builds], vec![]]
    );
}

#[tokio::test]
async fn once_every_side_moved_or_is_over_the_schedule_ends_early() {
    let running = start(|clock, seen| {
        vec![
            side(SideKind::Code, &[Look::Over], clock, seen),
            side(SideKind::Builds, &[Look::Quiet, Look::Moved], clock, seen),
        ]
    });
    move_to(&running.clock, &[5]).await;
    assert_eq!(
        *running.told.lock().unwrap(),
        [vec![SideKind::Builds]],
        "a side that cannot be read stops"
    );
    move_to(&running.clock, &[15]).await;
    assert!(running.task.await.unwrap(), "ended at 15 s, not 120 s");
    assert_eq!(running.seen.lock().unwrap().len(), 3);
    assert_eq!(
        *running.told.lock().unwrap(),
        [vec![SideKind::Builds], vec![]]
    );
}

#[tokio::test]
async fn a_stopped_schedule_reads_and_tells_nothing_more() {
    let running = start(|clock, seen| vec![side(SideKind::Code, &[], clock, seen)]);
    move_to(&running.clock, &[5]).await;
    running.stop.send_replace(true);
    move_to(&running.clock, &[15, 30, 60, 120]).await;
    assert!(!running.task.await.unwrap(), "stopped, not ended");
    assert_eq!(running.seen.lock().unwrap().len(), 1);
    assert!(running.told.lock().unwrap().is_empty());
}

#[tokio::test]
async fn the_quit_ends_a_schedule_that_waits() {
    let running = start(|clock, seen| vec![side(SideKind::Code, &[], clock, seen)]);
    running.closed.store(true, Ordering::SeqCst);
    // The wait looks up every `NAP` without the clock moving.
    tokio::time::sleep(NAP * 2).await;
    assert!(running.task.is_finished(), "the quit ends the wait");
    assert!(!running.task.await.unwrap());
    assert!(running.seen.lock().unwrap().is_empty());
}

#[test]
fn a_new_trigger_on_a_scope_restarts_its_schedule_without_telling_it_ended() {
    let lookouts = Lookouts::default();
    let (first, first_stop, told) =
        lookouts.start("forge:tide/pay", &[SideKind::Code, SideKind::Builds]);
    assert_eq!(
        told,
        [looking_of(
            "forge:tide/pay",
            &[SideKind::Code, SideKind::Builds]
        )]
    );
    let (second, _, told) = lookouts.start("forge:tide/pay", &[SideKind::Code]);
    assert_eq!(
        told,
        [looking_of("forge:tide/pay", &[SideKind::Code])],
        "only the new start is told"
    );
    assert!(*first_stop.borrow(), "the first schedule is stopped");
    assert_eq!(lookouts.looking().len(), 1, "one schedule per scope");
    assert_eq!(
        lookouts.left("forge:tide/pay", first, &[]),
        None,
        "the stopped one tells nothing"
    );
    assert_eq!(lookouts.looking().len(), 1, "and does not end the new one");
    assert!(lookouts.left("forge:tide/pay", second, &[]).is_some());
    assert!(lookouts.looking().is_empty(), "the new one ends");
}

#[test]
fn past_eight_scopes_the_oldest_is_stopped_and_told_ended() {
    let lookouts = Lookouts::default();
    let stops: Vec<_> = (0..AT_ONCE)
        .map(|n| lookouts.start(&format!("forge:r{n}"), &[SideKind::Code]).1)
        .collect();
    let (_, _, told) = lookouts.start("forge:newest", &[SideKind::Builds]);
    assert_eq!(
        told,
        [
            looking_of("forge:r0", &[]),
            looking_of("forge:newest", &[SideKind::Builds])
        ]
    );
    assert!(*stops[0].borrow(), "the oldest is stopped");
    assert!(stops[1..].iter().all(|stop| !*stop.borrow()));
    assert_eq!(lookouts.looking().len(), AT_ONCE);
}
