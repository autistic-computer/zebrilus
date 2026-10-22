use calloop::{
    EventLoop, PostAction,
    channel::{self, Sender},
    timer::TimeoutAction,
};
use zz_gpui::util::ResultExt;

use std::{mem::MaybeUninit, thread, time::Duration};

use zz_gpui::{
    PlatformDispatcher, Priority, PriorityQueueReceiver, PriorityQueueSender, RunnableVariant,
    profiler,
};

struct TimerAfter {
    duration: Duration,
    runnable: RunnableVariant,
}

pub(crate) struct LinuxDispatcher {
    main_sender: PriorityQueueCalloopSender<RunnableVariant>,
    timer_sender: Sender<TimerAfter>,
    background_sender: PriorityQueueSender<RunnableVariant>,
    _background_threads: Vec<thread::JoinHandle<()>>,
    main_thread_id: thread::ThreadId,
}

const MIN_THREADS: usize = 2;

impl LinuxDispatcher {
    pub fn new(main_sender: PriorityQueueCalloopSender<RunnableVariant>) -> Self {
        let (background_sender, background_receiver) = PriorityQueueReceiver::new();
        let thread_count =
            std::thread::available_parallelism().map_or(MIN_THREADS, |i| i.get().max(MIN_THREADS));

        let mut background_threads = (0..thread_count)
            .map(|i| {
                let receiver: PriorityQueueReceiver<RunnableVariant> = background_receiver.clone();
                std::thread::Builder::new()
                    .name(format!("Worker-{i}"))
                    .spawn(move || {
                        for runnable in receiver.iter() {
                            let location = runnable.metadata().location;
                            let spawned = runnable.metadata().spawned;
                            profiler::update_running_task(spawned, location);
                            runnable.run();
                            profiler::save_task_timing();
                        }
                    })
                    .unwrap()
            })
            .collect::<Vec<_>>();

        let (timer_sender, timer_channel) = calloop::channel::channel::<TimerAfter>();
        let timer_thread = std::thread::Builder::new()
            .name("Timer".to_owned())
            .spawn(move || {
                let mut event_loop: EventLoop<()> =
                    EventLoop::try_new().expect("Failed to initialize timer loop!");

                let handle = event_loop.handle();
                let timer_handle = event_loop.handle();
                let signal = event_loop.get_signal();
                handle
                    .insert_source(timer_channel, move |e, _, _| {
                        // The dispatcher owning the sender is gone; timers already
                        // scheduled would run tasks nothing can observe.
                        if let channel::Event::Closed = e {
                            signal.stop();
                        }
                        if let channel::Event::Msg(timer) = e {
                            let mut runnable = Some(timer.runnable);
                            timer_handle
                                .insert_source(
                                    calloop::timer::Timer::from_duration(timer.duration),
                                    move |_, _, _| {
                                        if let Some(runnable) = runnable.take() {
                                            let location = runnable.metadata().location;
                                            let spawned = runnable.metadata().spawned;
                                            profiler::update_running_task(spawned, location);
                                            runnable.run();
                                            profiler::save_task_timing();
                                        }
                                        TimeoutAction::Drop
                                    },
                                )
                                .expect("Failed to start timer");
                        }
                    })
                    .expect("Failed to start timer thread");

                event_loop.run(None, &mut (), |_| {}).log_err();
            })
            .unwrap();

        background_threads.push(timer_thread);

        Self {
            main_sender,
            timer_sender,
            background_sender,
            _background_threads: background_threads,
            main_thread_id: thread::current().id(),
        }
    }
}

impl PlatformDispatcher for LinuxDispatcher {
    fn is_main_thread(&self) -> bool {
        thread::current().id() == self.main_thread_id
    }

    fn dispatch(&self, runnable: RunnableVariant, priority: Priority) {
        self.background_sender
            .send(priority, runnable)
            .unwrap_or_else(|_| panic!("blocking sender returned without value"));
    }

    fn dispatch_on_main_thread(&self, runnable: RunnableVariant, priority: Priority) {
        self.main_sender
            .send(priority, runnable)
            .unwrap_or_else(|runnable| {
                // NOTE: Runnable may wrap a Future that is !Send.
                //
                // This is usually safe because we only poll it on the main thread.
                // However if the send fails, we know that:
                // 1. main_receiver has been dropped (which implies the app is shutting down)
                // 2. we are on a background thread.
                // It is not safe to drop something !Send on the wrong thread, and
                // the app will exit soon anyway, so we must forget the runnable.
                std::mem::forget(runnable);
            });
    }

    fn dispatch_after(&self, duration: Duration, runnable: RunnableVariant) {
        if let Err(err) = self.timer_sender.send(TimerAfter { duration, runnable }) {
            // The timer thread has shut down. Dropping a scheduled runnable cancels its task
            // and makes the next poll of any awaiter panic. Leaking leaves the task pending,
            // which is acceptable during shutdown.
            std::mem::forget(err);
        }
    }

    fn spawn_realtime(&self, f: Box<dyn FnOnce() + Send>) {
        std::thread::spawn(move || {
            // SAFETY: always safe to call
            let thread_id = unsafe { libc::pthread_self() };

            let policy = libc::SCHED_FIFO;
            let sched_priority = 65;

            // SAFETY: all sched_param members are valid when initialized to zero.
            let mut sched_param =
                unsafe { MaybeUninit::<libc::sched_param>::zeroed().assume_init() };
            sched_param.sched_priority = sched_priority;
            // SAFETY: sched_param is a valid initialized structure
            let result = unsafe { libc::pthread_setschedparam(thread_id, policy, &sched_param) };
            if result != 0 {
                log::warn!("failed to set realtime thread priority");
            }

            f();
        });
    }
}

pub struct PriorityQueueCalloopSender<T> {
    sender: PriorityQueueSender<T>,
    ping: calloop::ping::Ping,
}

impl<T> PriorityQueueCalloopSender<T> {
    fn new(tx: PriorityQueueSender<T>, ping: calloop::ping::Ping) -> Self {
        Self { sender: tx, ping }
    }

    fn send(&self, priority: Priority, item: T) -> Result<(), zz_gpui::queue::SendError<T>> {
        let res = self.sender.send(priority, item);
        if res.is_ok() {
            self.ping.ping();
        }
        res
    }
}

impl<T> Drop for PriorityQueueCalloopSender<T> {
    fn drop(&mut self) {
        self.ping.ping();
    }
}

pub struct PriorityQueueCalloopReceiver<T> {
    receiver: PriorityQueueReceiver<T>,
    source: calloop::ping::PingSource,
    ping: calloop::ping::Ping,
}

impl<T> PriorityQueueCalloopReceiver<T> {
    pub fn new() -> (PriorityQueueCalloopSender<T>, Self) {
        let (ping, source) = calloop::ping::make_ping().expect("Failed to create a Ping.");

        let (tx, rx) = PriorityQueueReceiver::new();

        (
            PriorityQueueCalloopSender::new(tx, ping.clone()),
            Self {
                receiver: rx,
                source,
                ping,
            },
        )
    }
}

use calloop::channel::Event;

#[derive(Debug)]
pub struct ChannelError(calloop::ping::PingError);

impl std::fmt::Display for ChannelError {
    #[cfg_attr(feature = "nightly_coverage", coverage(off))]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl std::error::Error for ChannelError {
    #[cfg_attr(feature = "nightly_coverage", coverage(off))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl<T> calloop::EventSource for PriorityQueueCalloopReceiver<T> {
    type Event = Event<T>;
    type Metadata = ();
    type Ret = ();
    type Error = ChannelError;

    fn process_events<F>(
        &mut self,
        readiness: calloop::Readiness,
        token: calloop::Token,
        mut callback: F,
    ) -> Result<calloop::PostAction, Self::Error>
    where
        F: FnMut(Self::Event, &mut Self::Metadata) -> Self::Ret,
    {
        let mut clear_readiness = false;
        let mut disconnected = false;

        let action = self
            .source
            .process_events(readiness, token, |(), &mut ()| {
                let mut is_empty = true;

                let receiver = self.receiver.clone();
                for runnable in receiver.try_iter() {
                    match runnable {
                        Ok(r) => {
                            callback(Event::Msg(r), &mut ());
                            is_empty = false;
                        }
                        Err(_) => {
                            disconnected = true;
                        }
                    }
                }

                if disconnected {
                    callback(Event::Closed, &mut ());
                }

                if is_empty {
                    clear_readiness = true;
                }
            })
            .map_err(ChannelError)?;

        if disconnected {
            Ok(PostAction::Remove)
        } else if clear_readiness {
            Ok(action)
        } else {
            // Re-notify the ping source so we can try again.
            self.ping.ping();
            Ok(PostAction::Continue)
        }
    }

    fn register(
        &mut self,
        poll: &mut calloop::Poll,
        token_factory: &mut calloop::TokenFactory,
    ) -> calloop::Result<()> {
        self.source.register(poll, token_factory)
    }

    fn reregister(
        &mut self,
        poll: &mut calloop::Poll,
        token_factory: &mut calloop::TokenFactory,
    ) -> calloop::Result<()> {
        self.source.reregister(poll, token_factory)
    }

    fn unregister(&mut self, poll: &mut calloop::Poll) -> calloop::Result<()> {
        self.source.unregister(poll)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::BTreeMap,
        sync::Arc,
        time::{Duration, Instant},
    };
    use zz_gpui::{BackgroundExecutor, ForegroundExecutor};

    type Switches = BTreeMap<String, (u64, u64)>;

    fn context_switches() -> Switches {
        let main_tid = unsafe { libc::gettid() };
        let mut by_name = Switches::new();
        for task in std::fs::read_dir("/proc/self/task").unwrap().flatten() {
            let path = task.path();
            let Ok(status) = std::fs::read_to_string(path.join("status")) else {
                continue;
            };
            let tid: i32 = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.parse().ok())
                .unwrap_or_default();
            let name = if tid == main_tid {
                "main".to_owned()
            } else {
                let comm = std::fs::read_to_string(path.join("comm")).unwrap_or_default();
                comm.trim()
                    .trim_end_matches(|c: char| c.is_ascii_digit())
                    .trim_end_matches('-')
                    .to_owned()
            };
            let field = |key: &str| -> u64 {
                status
                    .lines()
                    .find_map(|line| line.strip_prefix(key))
                    .and_then(|value| value.trim().parse().ok())
                    .unwrap_or_default()
            };
            let entry = by_name.entry(name).or_default();
            entry.0 += field("voluntary_ctxt_switches:");
            entry.1 += field("nonvoluntary_ctxt_switches:");
        }
        by_name
    }

    fn process_cpu() -> Duration {
        let mut usage = MaybeUninit::<libc::rusage>::zeroed();
        unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
        let usage = unsafe { usage.assume_init() };
        let micros = |time: libc::timeval| {
            Duration::from_secs(time.tv_sec as u64) + Duration::from_micros(time.tv_usec as u64)
        };
        micros(usage.ru_utime) + micros(usage.ru_stime)
    }

    fn report(label: &str, ops: u32, wall: Duration, cpu: Duration, before: &Switches) {
        let after = context_switches();
        let mut total = 0;
        let mut rows = Vec::new();
        for (name, (voluntary, involuntary)) in &after {
            let (voluntary0, involuntary0) = before.get(name).copied().unwrap_or_default();
            let delta = voluntary - voluntary0 + involuntary - involuntary0;
            if delta > 0 {
                total += delta;
                rows.push(format!("{name}={:.2}", delta as f64 / f64::from(ops)));
            }
        }
        println!(
            "{label}: ops={ops} wall_ns_per_op={:.0} cpu_ns_per_op={:.0} switches_per_op={:.2} [{}]",
            wall.as_nanos() as f64 / f64::from(ops),
            cpu.as_nanos() as f64 / f64::from(ops),
            total as f64 / f64::from(ops),
            rows.join(" ")
        );
    }

    fn main_loop() -> (EventLoop<'static, ()>, Arc<LinuxDispatcher>) {
        let event_loop = EventLoop::<()>::try_new().unwrap();
        let handle = event_loop.handle();
        let (sender, receiver) = PriorityQueueCalloopReceiver::new();
        let dispatcher = Arc::new(LinuxDispatcher::new(sender));
        handle
            .insert_source(receiver, {
                let handle = handle.clone();
                move |event, _, _: &mut ()| {
                    if let Event::Msg(runnable) = event {
                        handle.insert_idle(|_| {
                            runnable.run();
                        });
                    }
                }
            })
            .unwrap();
        (event_loop, dispatcher)
    }

    fn run_bench(
        label: &str,
        ops: u32,
        body: impl AsyncFnOnce(ForegroundExecutor, BackgroundExecutor) + 'static,
    ) {
        let (mut event_loop, dispatcher) = main_loop();
        let foreground = ForegroundExecutor::new(dispatcher.clone());
        let background = BackgroundExecutor::new(dispatcher.clone());
        let signal = event_loop.get_signal();
        thread::sleep(Duration::from_millis(50));
        let before = context_switches();
        let cpu = process_cpu();
        let started = Instant::now();
        foreground
            .spawn({
                let foreground = foreground.clone();
                async move {
                    body(foreground, background).await;
                    signal.stop();
                }
            })
            .detach();
        event_loop.run(None, &mut (), |_| {}).unwrap();
        report(label, ops, started.elapsed(), process_cpu() - cpu, &before);
        drop(dispatcher);
    }

    #[test]
    #[ignore = "benchmark: cargo test -p zz-gpui-platform --all-features --lib bench_dispatch -- --ignored --nocapture --test-threads=1"]
    fn bench_dispatch_timer_ticks() {
        const TICKS: u32 = 200;
        run_bench(
            "main_thread_timer_tick",
            TICKS,
            async move |_, background: BackgroundExecutor| {
                for _ in 0..TICKS {
                    background.timer(Duration::from_millis(2)).await;
                }
            },
        );
    }

    #[test]
    #[ignore = "benchmark: cargo test -p zz-gpui-platform --all-features --lib bench_dispatch -- --ignored --nocapture --test-threads=1"]
    fn bench_dispatch_runnable_round_trips() {
        const OPS: u32 = 20_000;
        run_bench(
            "main_thread_spawn_await",
            OPS,
            async move |foreground: ForegroundExecutor, _| {
                for _ in 0..OPS {
                    foreground.spawn(async {}).await;
                }
            },
        );
        run_bench(
            "background_round_trip",
            OPS,
            async move |_, background: BackgroundExecutor| {
                for _ in 0..OPS {
                    background.spawn(async {}).await;
                }
            },
        );
    }

    #[test]
    fn dispatcher_threads_exit_when_owner_drops() {
        let (sender, _receiver) = PriorityQueueCalloopReceiver::new();
        let mut dispatcher = LinuxDispatcher::new(sender);
        let threads = std::mem::take(&mut dispatcher._background_threads);
        drop(dispatcher);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while threads.iter().any(|thread| !thread.is_finished())
            && std::time::Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(threads.iter().all(thread::JoinHandle::is_finished));
        for thread in threads {
            thread
                .join()
                .expect("dispatcher thread should exit cleanly");
        }
    }

    #[test]
    fn calloop_works() {
        let mut event_loop = calloop::EventLoop::try_new().unwrap();
        let handle = event_loop.handle();

        let (tx, rx) = PriorityQueueCalloopReceiver::new();

        struct Data {
            got_msg: bool,
            got_closed: bool,
        }

        let mut data = Data {
            got_msg: false,
            got_closed: false,
        };

        let _channel_token = handle
            .insert_source(rx, move |evt, &mut (), data: &mut Data| match evt {
                Event::Msg(()) => {
                    data.got_msg = true;
                }

                Event::Closed => {
                    data.got_closed = true;
                }
            })
            .unwrap();

        // nothing is sent, nothing is received
        event_loop
            .dispatch(Some(::std::time::Duration::ZERO), &mut data)
            .unwrap();

        assert!(!data.got_msg);
        assert!(!data.got_closed);
        // a message is send

        tx.send(Priority::Medium, ()).unwrap();
        event_loop
            .dispatch(Some(::std::time::Duration::ZERO), &mut data)
            .unwrap();

        assert!(data.got_msg);
        assert!(!data.got_closed);

        // the sender is dropped
        drop(tx);
        event_loop
            .dispatch(Some(::std::time::Duration::ZERO), &mut data)
            .unwrap();

        assert!(data.got_msg);
        assert!(data.got_closed);
    }
}
