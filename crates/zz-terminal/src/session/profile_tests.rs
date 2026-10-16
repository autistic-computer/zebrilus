use super::*;

fn cadence() -> &'static str {
    static VALUE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    VALUE.get_or_init(|| std::env::var("ZZ_PROFILE_CADENCE").unwrap_or_default())
}

pub(super) fn publish_interval(interval: Duration) -> Duration {
    if interval == CONTENT_PUBLISH_STALENESS && cadence() == "10ms" {
        Duration::from_millis(10)
    } else {
        interval
    }
}

pub(super) fn publish_from_start() -> bool {
    cadence() == "start"
}

fn cpu_time_ns() -> u128 {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
    u128::try_from(time.tv_sec).expect("CPU seconds") * 1_000_000_000
        + u128::try_from(time.tv_nsec).expect("CPU nanoseconds")
}

fn sequence(viewport: &TerminalViewport) -> Option<u32> {
    viewport.cells.iter().take(6).try_fold(0, |value, cell| {
        Some(value * 10 + char::from_u32(cell.glyph())?.to_digit(10)?)
    })
}

fn percentile(values: &mut [u128], percent: usize) -> u128 {
    values.sort_unstable();
    values
        .get(values.len().saturating_sub(1) * percent / 100)
        .copied()
        .unwrap_or(0)
}

#[test]
#[ignore = "run each ZZ_PROFILE_CADENCE in a separate optimized process without other workloads"]
fn profile_terminal_publication_cadence() {
    let display_hz: u32 = std::env::var("ZZ_PROFILE_DISPLAY_HZ")
        .unwrap_or_else(|_| "60".to_owned())
        .parse()
        .expect("display Hz");
    assert!(matches!(display_hz, 60 | 120));
    let script = tempfile::NamedTempFile::new().expect("producer file");
    std::fs::write(
        script.path(),
        "import os,time\ntime.sleep(0.5)\nstart=time.monotonic()\nfor frame in range(720):\n payload=(f'{frame:06d}'+chr(65+frame%26)*113+'\\r\\n'+(chr(65+frame%26)*119+'\\r\\n')*38+chr(65+frame%26)*119).encode()\n os.write(1,b'\\x1b[H'+payload)\n time.sleep(max(0,start+(frame+1)/120-time.monotonic()))\ntime.sleep(0.2)\n",
    )
    .expect("producer source");
    let session = TerminalSession::spawn(
        2000,
        Arc::new(TerminalAppearance::default()),
        TerminalSpawn {
            shell: Some("/bin/sh".to_owned()),
            command: Some(vec![format!("exec python3 '{}'", script.path().display())]),
            initial_size: Some(TerminalSize::cells(120, 40)),
            non_login_shell: true,
            ..TerminalSpawn::default()
        },
    );
    let view = TerminalViewId(451);
    session.attach_view(view);
    session.set_view_area(view, Some((120, 40)));
    session.set_view_stream(view, ViewStream::Foreground);
    assert!(session.settle());
    let events = session.events();
    let before = cpu_time_ns();
    let started = Instant::now();
    let display_period = Duration::from_secs_f64(1.0 / f64::from(display_hz));
    let mut display_due = started;
    let mut publication_count = 0;
    let mut publication_times = Vec::new();
    let mut display_times = Vec::new();
    let mut display_sequences = Vec::new();
    let mut last_publication = None;
    let mut last_display = None;
    loop {
        let now = Instant::now();
        assert!(now.duration_since(started) < Duration::from_secs(12));
        while let Ok(event) = events.try_recv() {
            if matches!(event, TerminalEvent::ViewportReady { .. }) {
                publication_count += 1;
                if let Some(frame) = session.latest_viewport_for(view)
                    && let Some(seq) = sequence(&frame)
                    && last_publication != Some(seq)
                {
                    last_publication = Some(seq);
                    publication_times.push(now.duration_since(started).as_nanos());
                }
            }
        }
        if now >= display_due {
            while display_due <= now {
                display_due += display_period;
            }
            if let Some(frame) = session.latest_viewport_for(view)
                && let Some(seq) = sequence(&frame)
                && last_display != Some(seq)
            {
                last_display = Some(seq);
                display_times.push(now.duration_since(started).as_nanos());
                display_sequences.push(seq);
            }
        }
        if session.completion().is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    let elapsed = started.elapsed();
    let cpu = cpu_time_ns() - before;
    let mut publication_intervals = publication_times
        .windows(2)
        .map(|w| w[1] - w[0])
        .collect::<Vec<_>>();
    let mut display_intervals = display_times
        .windows(2)
        .map(|w| w[1] - w[0])
        .collect::<Vec<_>>();
    let skipped = display_sequences
        .windows(2)
        .map(|w| w[1].saturating_sub(w[0] + 1))
        .sum::<u32>();
    assert_eq!(last_display, Some(719));
    assert_eq!(session.completion().expect("completion").code, 0);
    println!(
        "{{\"cadence\":\"{}\",\"display_hz\":{display_hz},\"producer_hz\":120,\"producer_frames\":720,\"elapsed_ns\":{},\"cpu_ns\":{cpu},\"publications\":{publication_count},\"unique_publications\":{},\"displayed_updates\":{},\"skipped_sequences\":{skipped},\"publication_p50_ns\":{},\"publication_p95_ns\":{},\"display_p50_ns\":{},\"display_p95_ns\":{}}}",
        cadence(),
        elapsed.as_nanos(),
        publication_times.len(),
        display_times.len(),
        percentile(&mut publication_intervals, 50),
        percentile(&mut publication_intervals, 95),
        percentile(&mut display_intervals, 50),
        percentile(&mut display_intervals, 95),
    );
}
