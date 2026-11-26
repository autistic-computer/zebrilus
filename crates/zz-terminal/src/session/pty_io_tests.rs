#[cfg(unix)]
use super::test_support::*;
use super::*;
#[cfg(unix)]
use crate::DEFAULT_HISTORY_LIMIT;

#[cfg(all(unix, not(target_os = "linux")))]
#[test]
fn wake_pipe_drain_reads_until_empty() {
    let (read, write) = configured_actor_wake_pipe().expect("configured wake pipe");
    for fd in [&read, &write] {
        assert!(
            rustix::io::fcntl_getfd(fd)
                .expect("wake fd flags")
                .contains(rustix::io::FdFlags::CLOEXEC)
        );
        assert!(
            rustix::fs::fcntl_getfl(fd)
                .expect("wake status flags")
                .contains(rustix::fs::OFlags::NONBLOCK)
        );
    }

    for length in [128, 129] {
        let bytes = vec![1_u8; length];
        assert_eq!(
            rustix::io::write(&write, &bytes).expect("fill wake pipe"),
            length
        );
        drain_wake_pipe(&read).expect("drain wake pipe");
        let mut byte = [0_u8; 1];
        assert!(matches!(
            rustix::io::read(&read, &mut byte),
            Err(rustix::io::Errno::AGAIN)
        ));
    }
}

#[cfg(all(unix, not(target_os = "linux")))]
#[test]
fn wake_pipe_write_retries_interrupt_and_accepts_a_full_pipe() {
    let mut attempts = 0;
    write_actor_wake(|| {
        attempts += 1;
        if attempts == 1 {
            Err(rustix::io::Errno::INTR)
        } else {
            Ok(1)
        }
    })
    .expect("interrupted wake retry");
    assert_eq!(attempts, 2);

    write_actor_wake(|| Err(rustix::io::Errno::AGAIN)).expect("full pipe is already readable");
}

#[cfg(unix)]
#[test]
fn saturated_pty_input_does_not_block_resize_or_shutdown() {
    let session = Arc::new(TerminalSession::spawn(
        DEFAULT_HISTORY_LIMIT,
        Arc::new(TerminalAppearance::default()),
        TerminalSpawn {
            command: Some(vec!["sleep 30".to_owned()]),
            ..TerminalSpawn::default()
        },
    ));
    wait_for_test_viewport(&session, |viewport| {
        matches!(viewport.status, SessionStatus::Running)
    });
    let view = TerminalViewId(1);
    attach_streaming(&session, view);
    let deadline = Instant::now() + Duration::from_secs(2);
    while session.latest_viewport_for(view).is_none() {
        assert!(Instant::now() < deadline, "view never attached");
        thread::sleep(Duration::from_millis(10));
    }

    let chunk = Arc::<str>::from("x".repeat(1024 * 1024));
    let mut accepted = 0;
    let mut rejected = 0;
    for _ in 0..(MAX_PENDING_PTY_INPUT_BYTES / chunk.len() + 8) {
        if session.send_command(Command::Text {
            view: Some(view),
            text: Arc::clone(&chunk),
        }) {
            accepted += 1;
        } else {
            rejected += 1;
            break;
        }
    }
    assert!(accepted > 0);
    assert_eq!(rejected, 1);

    let (pending_commands, pending_bytes) = session.commands.pending_input();
    let diagnostics = session.diagnostics();
    assert_eq!(diagnostics.command_queue_len, pending_commands);
    assert_eq!(diagnostics.pending_pty_input_bytes, pending_bytes);
    assert!(diagnostics.pending_pty_input_bytes >= chunk.len());

    session.resize(37, 9, 8, 18);

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let viewport = session.latest_viewport();
        if viewport.columns == 37 && viewport.rows == 9 {
            break;
        }
        assert!(Instant::now() < deadline, "resize stayed behind PTY input");
        thread::sleep(Duration::from_millis(10));
    }

    assert!(session.send_command(Command::ViewAction {
        view,
        action: TerminalViewAction::EnterCopyMode,
    }));
    wait_for_test_viewport(&session, |viewport| {
        matches!(viewport.mode, TerminalMode::Copy { .. })
    });

    let events = session.events();
    drop(session);
    let deadline = Instant::now() + Duration::from_secs(2);
    while !events.receiver.is_closed() {
        assert!(
            Instant::now() < deadline,
            "shutdown stayed behind PTY input"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
#[test]
fn pty_output_drains_while_the_input_writer_is_backpressured() {
    let directory = tempfile::tempdir().expect("PTY duplex test directory");
    let session = TerminalSession::spawn(
        DEFAULT_HISTORY_LIMIT,
        Arc::new(TerminalAppearance::default()),
        TerminalSpawn {
            command: Some(vec![
                "stty raw -echo; printf 'ZZ_DUPLEX_READY\\r\\n'; while [ ! -f release ]; do sleep 0.01; done; dd if=/dev/zero bs=262144 count=1 2>/dev/null; IFS= read -r line; printf '\\r\\nZZ_DUPLEX_DRAIN_OK\\r\\n'; sleep 30"
                    .to_owned(),
            ]),
            working_directory: Some(directory.path().to_owned()),
            ..TerminalSpawn::default()
        },
    );
    let view = TerminalViewId(2);
    attach_streaming(&session, view);
    wait_for_test_viewport(&session, |viewport| {
        let mut contents = String::new();
        for cell in viewport.cells.iter() {
            viewport.push_glyph(*cell, &mut contents);
        }
        contents.contains("ZZ_DUPLEX_READY")
    });

    assert!(session.send_command(Command::Text {
        view: Some(view),
        text: Arc::from(format!("{}\n", "i".repeat(256 * 1024))),
    }));
    let deadline = Instant::now() + Duration::from_secs(15);
    while session.commands.pending_input().0 == 0
        || !session
            .commands
            .queues
            .input
            .as_ref()
            .expect("live terminal input queue")
            .commands
            .is_empty()
    {
        assert!(
            Instant::now() < deadline,
            "PTY input never reached the backpressured writer"
        );
        thread::sleep(Duration::from_millis(10));
    }

    std::fs::write(directory.path().join("release"), []).expect("release PTY output");
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let viewport = session.latest_viewport_for(view);
        let mut contents = String::new();
        if let Some(viewport) = viewport {
            for cell in viewport.cells.iter() {
                viewport.push_glyph(*cell, &mut contents);
            }
        }
        if contents.contains("ZZ_DUPLEX_DRAIN_OK") {
            assert_eq!(session.commands.pending_input(), (0, 0));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "PTY output stopped behind the backpressured writer"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
#[test]
fn pty_writer_retains_every_reported_byte_during_saturation() {
    let (read, write) = rustix::pipe::pipe().expect("PTY writer test pipe");
    rustix::io::ioctl_fionbio(&read, true).expect("nonblocking pipe reader");
    rustix::io::ioctl_fionbio(&write, true).expect("nonblocking pipe writer");
    let filler = [0_u8; 4096];
    loop {
        match rustix::io::write(&write, &filler) {
            Ok(_) | Err(rustix::io::Errno::INTR) => {}
            Err(rustix::io::Errno::AGAIN) => break,
            Err(error) => panic!("fill writer test pipe: {error}"),
        }
    }

    let fd = filedescriptor::FileDescriptor::dup(&write).expect("duplicate pipe writer");
    let mut writer = PtyWriter::new(fd);
    let chunk = vec![1_u8; 32 * 1024];
    for _ in 0..8 {
        assert_eq!(
            writer.write(&chunk).expect("queue saturated input"),
            chunk.len()
        );
    }
    assert_eq!(writer.queued_bytes(), chunk.len() * 8);
}

#[cfg(unix)]
#[test]
fn pty_effects_wait_for_the_existing_writer_backlog() {
    let (read, write) = rustix::pipe::pipe().expect("PTY effects test pipe");
    rustix::io::ioctl_fionbio(&read, true).expect("nonblocking pipe reader");
    rustix::io::ioctl_fionbio(&write, true).expect("nonblocking pipe writer");
    let filler = [0_u8; 4096];
    let mut filler_bytes = 0;
    loop {
        match rustix::io::write(&write, &filler) {
            Ok(written) => filler_bytes += written,
            Err(rustix::io::Errno::INTR) => {}
            Err(rustix::io::Errno::AGAIN) => break,
            Err(error) => panic!("fill PTY effects pipe: {error}"),
        }
    }

    let fd = filedescriptor::FileDescriptor::dup(&write).expect("duplicate pipe writer");
    let mut writer = PtyWriter::new(fd);
    writer.write_all(b"input").expect("queue PTY input");
    assert_eq!(writer.queued_bytes(), 5);

    let effects = RefCell::new(PtyEffects::new());
    effects.borrow_mut().push(b"reply");
    drain_effects_if_writer_ready(&effects, &mut writer).expect("defer PTY effect");
    assert_eq!(effects.borrow().bytes, b"reply");

    let mut scratch = [0_u8; 8192];
    let mut drained_filler = 0;
    while drained_filler < filler_bytes {
        match rustix::io::read(&read, &mut scratch) {
            Ok(read) => drained_filler += read,
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => panic!("drain PTY effects filler: {error}"),
        }
    }
    writer.flush_pending().expect("flush PTY input");
    assert!(!writer.has_pending());
    drain_effects_if_writer_ready(&effects, &mut writer).expect("flush PTY effect");
    assert!(effects.borrow().bytes.is_empty());
}

#[cfg(unix)]
#[test]
fn pty_writer_preserves_order_across_partial_writes() {
    let (read, write) = rustix::pipe::pipe().expect("PTY writer test pipe");
    rustix::io::ioctl_fionbio(&read, true).expect("nonblocking pipe reader");
    rustix::io::ioctl_fionbio(&write, true).expect("nonblocking pipe writer");
    let filler = [0_u8; 4096];
    let mut filler_bytes = 0;
    loop {
        match rustix::io::write(&write, &filler) {
            Ok(written) => filler_bytes += written,
            Err(rustix::io::Errno::INTR) => {}
            Err(rustix::io::Errno::AGAIN) => break,
            Err(error) => panic!("fill writer test pipe: {error}"),
        }
    }

    let fd = filedescriptor::FileDescriptor::dup(&write).expect("duplicate pipe writer");
    let mut writer = PtyWriter::new(fd);
    let first = vec![0x31_u8; 96 * 1024];
    let second = vec![0x32_u8; 96 * 1024];
    assert_eq!(
        writer.write(&first).expect("queue first input"),
        first.len()
    );
    assert_eq!(
        writer.write(&second).expect("queue second input"),
        second.len()
    );

    let mut scratch = [0_u8; 8192];
    let mut drained_filler = 0;
    while drained_filler < filler_bytes {
        match rustix::io::read(&read, &mut scratch) {
            Ok(read) => drained_filler += read,
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => panic!("drain writer test filler: {error}"),
        }
    }

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut actual = Vec::with_capacity(first.len() + second.len());
    while writer.has_pending() {
        writer.flush_pending().expect("flush queued input");
        loop {
            match rustix::io::read(&read, &mut scratch) {
                Ok(read) => actual.extend_from_slice(&scratch[..read]),
                Err(rustix::io::Errno::INTR) => {}
                Err(rustix::io::Errno::AGAIN) => break,
                Err(error) => panic!("read flushed input: {error}"),
            }
        }
        assert!(Instant::now() < deadline, "partial writes never drained");
    }
    loop {
        match rustix::io::read(&read, &mut scratch) {
            Ok(read) => actual.extend_from_slice(&scratch[..read]),
            Err(rustix::io::Errno::INTR) => {}
            Err(rustix::io::Errno::AGAIN) => break,
            Err(error) => panic!("read final flushed input: {error}"),
        }
    }

    let mut expected = first;
    expected.extend_from_slice(&second);
    assert_eq!(actual, expected);
}

#[cfg(unix)]
#[test]
fn pty_writer_releases_large_drained_allocation() {
    let sink = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/null")
        .expect("null sink");
    let fd = filedescriptor::FileDescriptor::dup(&sink).expect("duplicate null sink");
    let mut writer = PtyWriter::new(fd);

    writer
        .write_all(&vec![0_u8; PTY_WRITE_RETAIN_BYTES * 2])
        .expect("write large input");
    writer.flush_pending().expect("drain large input");

    assert!(!writer.has_pending());
    assert_eq!(writer.pending_capacity(), 0);
}

#[test]
fn pty_geometry_uses_total_pixel_dimensions() {
    let geometry = Geometry {
        columns: 120,
        rows: 40,
        cell_width_px: 9,
        cell_height_px: 20,
    };
    assert_eq!(
        geometry.pty_size(),
        PtySize {
            rows: 40,
            cols: 120,
            pixel_width: 1080,
            pixel_height: 800,
        }
    );
}

#[test]
fn pty_reader_recycles_its_bounded_buffer_pool() {
    let (output_tx, output_rx) = crossbeam_channel::bounded(1);
    let (recycle_tx, recycle_rx) = crossbeam_channel::bounded(2);
    let mut pool_allocations = Vec::new();
    for _ in 0..2 {
        let buffer = vec![0_u8; PTY_READ_BUFFER_BYTES];
        pool_allocations.push(buffer.as_ptr() as usize);
        recycle_tx.send(buffer).expect("seed recycle pool");
    }
    let input = vec![b'x'; PTY_READ_BUFFER_BYTES * 3];
    let reader = thread::spawn(move || {
        read_pty(
            Box::new(std::io::Cursor::new(input)),
            Box::new(|| 0),
            output_tx,
            recycle_rx,
            ActorWake::none(),
        );
    });

    let mut observed = Vec::new();
    for _ in 0..3 {
        let ReaderMessage::Data { buffer, length } = output_rx.recv().expect("PTY data") else {
            panic!("PTY reader reached EOF before all input was delivered");
        };
        assert_eq!(length, PTY_READ_BUFFER_BYTES);
        assert!(buffer[..length].iter().all(|byte| *byte == b'x'));
        observed.push(buffer.as_ptr() as usize);
        let _ = recycle_tx.send(buffer);
    }
    assert!(matches!(
        output_rx.recv().expect("PTY EOF"),
        ReaderMessage::Eof
    ));
    reader.join().expect("PTY reader thread");

    assert!(
        observed
            .iter()
            .all(|pointer| pool_allocations.contains(pointer))
    );
    observed.sort_unstable();
    observed.dedup();
    assert!(observed.len() < 3);
}

#[test]
fn pty_output_bursts_leave_ready_data_for_the_next_actor_turn() {
    let (output_tx, output_rx) = crossbeam_channel::unbounded();
    for byte in 1..=PTY_BUFFER_POOL_SIZE {
        output_tx
            .send(ReaderMessage::Data {
                buffer: vec![u8::try_from(byte).expect("test byte")],
                length: 1,
            })
            .expect("queue PTY data");
    }

    let mut consumed = Vec::new();
    let reached_eof = drain_pty_output_burst(
        &output_rx,
        vec![0],
        1,
        false,
        PTY_BUFFER_POOL_SIZE,
        |buffer, length| {
            consumed.extend_from_slice(&buffer[..length]);
        },
    )
    .is_some();

    let burst_limit = u8::try_from(PTY_BUFFER_POOL_SIZE).expect("burst limit fits in a byte");
    assert!(!reached_eof);
    assert_eq!(consumed, (0..burst_limit).collect::<Vec<_>>());
    assert_eq!(output_rx.len(), 1);
    assert!(matches!(
        output_rx.try_recv(),
        Ok(ReaderMessage::Data { buffer, length: 1 })
            if buffer == vec![burst_limit]
    ));
}

#[cfg(unix)]
#[test]
fn a_full_output_sink_keeps_actor_requests_live_and_room_resumes_output() {
    let session = TerminalSession::spawn(
        DEFAULT_HISTORY_LIMIT,
        Arc::new(TerminalAppearance::default()),
        TerminalSpawn {
            command: Some(vec![
                "read _; head -c 1048576 /dev/zero; printf TAP_END; read _".into(),
            ]),
            ..TerminalSpawn::default()
        },
    );
    attach_streaming(&session, TerminalViewId(116));
    wait_for_test_viewport(&session, |viewport| {
        matches!(viewport.status, SessionStatus::Running)
    });
    let (sink, output) = TestOutputSink::install(&session);
    sink.set_room(4);
    assert!(session.send_raw_input(Arc::from(b"go\n".as_slice())));
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut received = Vec::new();
    for _ in 0..4 {
        received.extend_from_slice(&output.recv_deadline(deadline).unwrap());
    }
    assert!(session.settle());
    session.capture(CaptureOptions::default()).unwrap();
    assert!(output.try_recv().is_err());
    sink.set_room(usize::MAX);
    session.output_wake().wake();
    while !received.ends_with(b"TAP_END") {
        received.extend_from_slice(&output.recv_deadline(deadline).unwrap());
    }
    assert!(received.len() >= 1_048_576);
    session.terminate();
}

#[cfg(unix)]
#[test]
fn output_sink_sees_bytes_only_while_it_takes_output() {
    let session = TerminalSession::spawn(
        DEFAULT_HISTORY_LIMIT,
        Arc::new(TerminalAppearance::default()),
        TerminalSpawn {
            command: Some(vec![
                "read _; printf 'BEFORE\\n'; read _; printf 'AFTER\\001\\002\\n'; read _; printf 'LATER\\n'; read _"
                    .to_owned(),
            ]),
            ..TerminalSpawn::default()
        },
    );
    attach_streaming(&session, TerminalViewId(104));
    let (sink, output) = TestOutputSink::install(&session);
    sink.take(false);
    wait_for_test_viewport(&session, |viewport| {
        matches!(viewport.status, SessionStatus::Running)
    });
    session.send_raw_input(Arc::from(b"first\n".as_slice()));
    wait_for_test_viewport(&session, |viewport| {
        let mut contents = String::new();
        for cell in viewport.cells.iter() {
            viewport.push_glyph(*cell, &mut contents);
        }
        contents.contains("BEFORE")
    });

    sink.take(true);
    session.send_raw_input(Arc::from(b"second\n".as_slice()));
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut bytes = Vec::new();
    while !bytes.windows(5).any(|window| window == b"AFTER") {
        let chunk = output
            .recv_deadline(deadline)
            .expect("post-arm terminal output");
        bytes.extend_from_slice(&chunk);
    }
    assert!(!bytes.windows(6).any(|window| window == b"BEFORE"));
    assert!(bytes.windows(7).any(|window| window == b"AFTER\x01\x02"));

    sink.take(false);
    while output.try_recv().is_ok() {}
    session.send_raw_input(Arc::from(b"third\n".as_slice()));
    wait_for_test_viewport(&session, |viewport| {
        let mut contents = String::new();
        for cell in viewport.cells.iter() {
            viewport.push_glyph(*cell, &mut contents);
        }
        contents.contains("LATER")
    });
    assert!(
        output
            .try_iter()
            .all(|chunk| !chunk.windows(5).any(|w| w == b"LATER"))
    );
}
