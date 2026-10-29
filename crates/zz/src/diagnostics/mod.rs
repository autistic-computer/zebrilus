//! Process logging, resource sampling, and application diagnostics.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use ala::{App, Entity, KeyBinding};
use zz_protocol::{ClientMessageKind, CommandInvocation};
use zz_ui::ROOT_KEY_CONTEXT;

use crate::{browser::controller::BrowserController, mux::client::MuxClient};

pub(crate) use zz_cli::diagnostics::{
    VERBOSE_LOG, application_args, capture_stall_sample, cef_log_file, elapsed_us, elapsed_us_from,
    enabled, platform_log_dir, timer,
};

const APP_STATE_SAMPLE_INTERVAL: Duration = Duration::from_secs(5);

pub fn start_app_state_sampler(
    controller: Entity<BrowserController>,
    mux: Entity<MuxClient>,
    cx: &mut App,
) {
    if !enabled() {
        return;
    }
    log_app_state(&controller, &mux, cx, "startup");
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor()
                .timer(APP_STATE_SAMPLE_INTERVAL)
                .await;
            cx.update(|cx| log_app_state(&controller, &mux, cx, "periodic"));
        }
    })
    .detach();
}

ala::actions!(zz, [DebugMark]);

#[cfg(target_os = "macos")]
const DEBUG_MARK_KEYSTROKE: &str = "cmd-shift-m";
#[cfg(not(target_os = "macos"))]
const DEBUG_MARK_KEYSTROKE: &str = "ctrl-shift-m";

static DEBUG_MARK_SEQ: AtomicU64 = AtomicU64::new(0);

/// Binds a key that stamps a marker plus a state snapshot into the log,
/// forwards the marker to the daemon, and shows a toast.
pub fn init_debug_mark(
    controller: Entity<BrowserController>,
    mux: Entity<MuxClient>,
    cx: &mut App,
) {
    cx.bind_keys([KeyBinding::new(
        DEBUG_MARK_KEYSTROKE,
        DebugMark,
        Some(ROOT_KEY_CONTEXT),
    )]);
    cx.on_action(move |_: &DebugMark, cx| {
        let seq = DEBUG_MARK_SEQ.fetch_add(1, Ordering::Relaxed) + 1;
        log::info!(target: "zz::diagnostics::marker", "user_marker seq={seq}");
        log_app_state(&controller, &mux, cx, "user_marker");
        mux.read(cx).execute(CommandInvocation::new(
            "debug-marker",
            [format!("seq={seq}")],
        ));
        mux.update(cx, |_, cx| {
            MuxClient::emit_notification(ClientMessageKind::Info, format!("log marker #{seq}"), cx);
        });
        if let Err(error) = thread::Builder::new()
            .name("zz-stall-sample".to_owned())
            .spawn(|| capture_stall_sample("user_marker"))
        {
            log::warn!(
                target: "zz::diagnostics::stall",
                "stall_sample_failed reason=user_marker error={error}"
            );
        }
        log::logger().flush();
    });
}

const STALL_THRESHOLD_US: u64 = 500_000;
const STALL_HEARTBEAT_INTERVAL: Duration = Duration::from_millis(100);

/// Logs main-thread freezes and recoveries from a watchdog thread.
pub fn start_main_thread_watchdog(cx: &mut App) {
    let epoch = Instant::now();
    let stall_age: Box<dyn Fn() -> u64 + Send> = if let Some(busy) = cx.main_thread_busy() {
        Box::new(move || {
            u64::try_from(busy.busy_for().min(epoch.elapsed()).as_micros()).unwrap_or(u64::MAX)
        })
    } else {
        let heartbeat = Arc::new(AtomicU64::new(0));
        let beat = Arc::clone(&heartbeat);
        cx.spawn(async move |cx| {
            loop {
                cx.background_executor()
                    .timer(STALL_HEARTBEAT_INTERVAL)
                    .await;
                beat.store(elapsed_us_from(epoch), Ordering::Relaxed);
            }
        })
        .detach();
        Box::new(move || elapsed_us_from(epoch).saturating_sub(heartbeat.load(Ordering::Relaxed)))
    };
    if let Err(error) = thread::Builder::new()
        .name("zz-stall-watchdog".to_owned())
        .spawn(move || watch_main_thread(&*stall_age, epoch))
    {
        log::error!(
            target: "zz::diagnostics::stall",
            "could not start stall watchdog: {error}"
        );
    }
}

fn watch_main_thread(stall_age: &dyn Fn() -> u64, epoch: Instant) {
    let mut stalled_since = None;
    let mut check_interval = watchdog_check_interval(0);
    loop {
        thread::sleep(check_interval);
        let age = stall_age();
        let beat = elapsed_us_from(epoch).saturating_sub(age);
        check_interval = watchdog_check_interval(age);
        if age > STALL_THRESHOLD_US {
            if stalled_since.is_none() {
                stalled_since = Some(beat);
                log::warn!(
                    target: "zz::diagnostics::stall",
                    "main_thread_stall stalled_us={age}"
                );
                log::logger().flush();
                capture_stall_sample("watchdog");
            }
        } else if let Some(since) = stalled_since.take() {
            log::warn!(
                target: "zz::diagnostics::stall",
                "main_thread_stall_recovered duration_us={}",
                beat.saturating_sub(since)
            );
        }
    }
}

fn watchdog_check_interval(age_us: u64) -> Duration {
    if age_us > STALL_THRESHOLD_US {
        STALL_HEARTBEAT_INTERVAL
    } else {
        Duration::from_micros(STALL_THRESHOLD_US - age_us + 1)
    }
}

fn log_app_state(
    controller: &Entity<BrowserController>,
    mux: &Entity<MuxClient>,
    cx: &App,
    reason: &str,
) {
    log::info!(
        target: "zz::diagnostics::app_state",
        "snapshot reason={reason} windows={}",
        cx.windows().len()
    );
    mux.read(cx).log_diagnostic_snapshot(reason);
    controller.read(cx).log_diagnostic_snapshot(reason);
}

pub(crate) fn open_logs(cx: &App) {
    let path = VERBOSE_LOG
        .get()
        .and_then(Clone::clone)
        .unwrap_or_else(|| platform_log_dir().join("zz.app.log"));
    if path.exists() {
        cx.reveal_path(&path);
    } else if let Some(directory) = path.parent()
        && let Ok(url) = url::Url::from_directory_path(directory)
    {
        cx.open_url(url.as_str());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watchdog_deadlines_preserve_stall_detection_and_recovery() {
        for start in (100_000_u64..2_000_000).step_by(10_000) {
            let end = start + 900_000;
            let last_beat = start / 100_000 * 100_000;
            let mut now = 0;
            let mut detected = None;
            let mut recovered = None;
            while now < end + 200_000 {
                let beat = if (start..end).contains(&now) {
                    last_beat
                } else {
                    now / 100_000 * 100_000
                };
                let age = now - beat;
                if age > STALL_THRESHOLD_US {
                    detected.get_or_insert(now);
                } else if detected.is_some() {
                    recovered.get_or_insert(now);
                }
                now += u64::try_from(watchdog_check_interval(age).as_micros()).unwrap();
            }
            let detected = detected.expect("a stranded foreground heartbeat must be detected");
            assert!(detected > last_beat + STALL_THRESHOLD_US);
            assert!(detected <= last_beat + STALL_THRESHOLD_US + 100_000);
            let recovered = recovered.expect("foreground progress must clear the stall");
            assert!((end..=end + 100_000).contains(&recovered));
        }
    }

    #[test]
    fn busy_marker_deadlines_detect_and_clear_stalls() {
        for start in (100_000_u64..2_000_000).step_by(10_000) {
            let end = start + 900_000;
            let mut now = 0;
            let mut detected = None;
            let mut recovered = None;
            while now < end + 200_000 {
                let age = if (start..end).contains(&now) {
                    now - start
                } else {
                    0
                };
                if age > STALL_THRESHOLD_US {
                    detected.get_or_insert(now);
                } else if detected.is_some() {
                    recovered.get_or_insert(now);
                }
                now += u64::try_from(watchdog_check_interval(age).as_micros()).unwrap();
            }
            let detected = detected.expect("a busy main loop must be detected");
            assert!(detected > start + STALL_THRESHOLD_US);
            assert!(detected <= start + STALL_THRESHOLD_US + 100_000);
            let recovered = recovered.expect("an idle main loop must clear the stall");
            assert!((end..=end + 100_000).contains(&recovered));
        }
    }

    #[test]
    fn idle_busy_marker_checks_at_most_twice_a_second() {
        let mut now = 0;
        let mut checks = 0;
        while now < 10_000_000 {
            now += u64::try_from(watchdog_check_interval(0).as_micros()).unwrap();
            checks += 1;
        }
        assert!(checks <= 20);
    }

    #[test]
    fn healthy_watchdog_checks_only_at_expiring_heartbeat_deadlines() {
        let mut now = 0;
        let mut checks = 0;
        while now < 10_000_000 {
            let beat = now / 100_000 * 100_000;
            let age = now - beat;
            assert!(age <= STALL_THRESHOLD_US);
            now += u64::try_from(watchdog_check_interval(age).as_micros()).unwrap();
            checks += 1;
        }
        assert!(checks <= 25);
    }
}
