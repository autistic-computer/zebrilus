use super::*;

#[cfg(target_os = "linux")]
fn gather_fixture(
    gather: &PtyGather,
    pool: usize,
) -> (
    std::os::fd::OwnedFd,
    Receiver<ReaderMessage>,
    BufferReturn,
    Vec<usize>,
) {
    let (write_fd, pane, output_rx, buffers, allocations) = parked_fixture(pool);
    assert!(
        gather.lend(pane).is_ok(),
        "register the fixture with the gather"
    );
    (write_fd, output_rx, buffers, allocations)
}

#[cfg(target_os = "linux")]
fn parked_fixture(
    pool: usize,
) -> (
    std::os::fd::OwnedFd,
    GatherPane,
    Receiver<ReaderMessage>,
    BufferReturn,
    Vec<usize>,
) {
    let (read_fd, write_fd) = rustix::pipe::pipe().expect("PTY fixture pipe");
    rustix::io::ioctl_fionbio(&read_fd, true).expect("nonblocking fixture reader");
    let (output_tx, output_rx) = crossbeam_channel::bounded(pool + 1);
    let (recycle_tx, recycle_rx) = crossbeam_channel::bounded(pool);
    let mut allocations = Vec::new();
    for _ in 0..pool {
        let buffer = vec![0_u8; PTY_READ_BUFFER_BYTES];
        allocations.push(buffer.as_ptr() as usize);
        recycle_tx.send(buffer).expect("seed gather pool");
    }
    let fd = filedescriptor::FileDescriptor::dup(&read_fd).expect("fixture reader");
    let (pane, lease) = GatherPane::open(fd, output_tx, recycle_rx, ActorWake::none());
    (
        write_fd,
        pane,
        output_rx,
        BufferReturn {
            buffers: recycle_tx,
            gather: lease,
        },
        allocations,
    )
}

#[cfg(target_os = "linux")]
fn write_fixture(write_fd: &std::os::fd::OwnedFd, bytes: &[u8]) {
    let mut offset = 0;
    while offset < bytes.len() {
        match rustix::io::write(write_fd, &bytes[offset..]) {
            Ok(sent) => offset += sent,
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => panic!("fixture write failed: {error}"),
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
fn linux_pty_gather_sends_a_quiet_pane_home_after_its_data() {
    let gather = PtyGather::start("zz-pty-home-t".to_owned()).expect("gather thread");
    let (write_fd, output_rx, buffers, _) = gather_fixture(&gather, PTY_BUFFER_POOL_SIZE);
    let first = lend_pattern(0, 3, PTY_READ_BUFFER_BYTES + 100);
    let writer = {
        let first = first.clone();
        let write_fd = write_fd.try_clone().expect("fixture writer");
        thread::spawn(move || write_fixture(&write_fd, &first))
    };
    let mut observed = Vec::new();
    let pane = loop {
        match output_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("gather output")
        {
            ReaderMessage::Data { buffer, length } => {
                observed.extend_from_slice(&buffer[..length]);
                buffers.give(buffer);
            }
            ReaderMessage::Home => break sent_home(&buffers),
            ReaderMessage::Eof => panic!("the fixture closed"),
        }
    };
    writer.join().expect("fixture writer");
    assert_eq!(observed, first);

    write_fixture(&write_fd, b"after");
    assert!(output_rx.recv_timeout(PTY_GATHER_LEND_IDLE * 2).is_err());
    let mut read = [0_u8; 16];
    assert_eq!(rustix::io::read(&pane.source.fd, &mut read), Ok(5));
    assert_eq!(&read[..5], b"after");

    assert!(gather.lend(pane).is_ok());
    write_fixture(&write_fd, b"again");
    let (buffer, length) = gathered(&gather, &output_rx, &buffers).expect("lent again");
    assert_eq!(&buffer[..length], b"again");
}

#[cfg(target_os = "linux")]
#[test]
fn linux_direct_reader_lends_its_pane_after_a_burst_without_a_pause() {
    let gather = PtyGather::start("zz-pty-direct-t".to_owned()).expect("gather thread");
    let (write_fd, pane, output_rx, buffers, _) = parked_fixture(PTY_BUFFER_POOL_SIZE);
    let (mut reader, mut buffer) = DirectReader::new(Some(gather.clone()), pane);
    assert_eq!(buffer.len(), PTY_READ_BUFFER_BYTES);
    let half = PTY_READ_BUFFER_BYTES / 2;
    assert!(!reader.lend_if_busy(half, &mut buffer, &buffers));
    reader.last -= PTY_GATHER_LEND_BUSY;
    assert!(!reader.lend_if_busy(half, &mut buffer, &buffers));
    assert!(reader.fd().is_some());
    reader.last = Instant::now() + Duration::from_mins(1);
    assert!(reader.lend_if_busy(half, &mut buffer, &buffers));
    assert!(reader.fd().is_none());
    assert!(buffer.is_empty());

    write_fixture(&write_fd, b"lent");
    let (lent, length) = gathered(&gather, &output_rx, &buffers).expect("gathered after the lend");
    assert_eq!(&lent[..length], b"lent");
}

#[cfg(target_os = "linux")]
fn gathered(
    gather: &PtyGather,
    output: &Receiver<ReaderMessage>,
    buffers: &BufferReturn,
) -> Option<(Vec<u8>, usize)> {
    loop {
        match output
            .recv_timeout(Duration::from_secs(2))
            .expect("gather output")
        {
            ReaderMessage::Data { buffer, length } => return Some((buffer, length)),
            ReaderMessage::Home => assert!(gather.lend(sent_home(buffers)).is_ok()),
            ReaderMessage::Eof => return None,
        }
    }
}

#[cfg(target_os = "linux")]
fn sent_home(buffers: &BufferReturn) -> GatherPane {
    buffers.gather.take_home().expect("the pane waits at home")
}

#[cfg(target_os = "linux")]
#[test]
fn linux_pty_gather_preserves_order_through_bounded_buffers() {
    let gather = PtyGather::start("zz-pty-gather".to_owned()).expect("gather thread");
    let (write_fd, output_rx, buffers, allocations) = gather_fixture(&gather, PTY_BUFFER_POOL_SIZE);
    let expected = (0..PTY_READ_BUFFER_BYTES * 2 + 137)
        .map(|index| u8::try_from(index % 251).expect("bounded byte"))
        .collect::<Vec<_>>();
    let writer_expected = expected.clone();
    let writer = thread::spawn(move || {
        let mut offset = 0;
        while offset < writer_expected.len() {
            match rustix::io::write(&write_fd, &writer_expected[offset..]) {
                Ok(written) => offset += written,
                Err(rustix::io::Errno::INTR) => {}
                Err(error) => panic!("fixture write failed: {error}"),
            }
        }
    });

    let mut observed = Vec::with_capacity(expected.len());
    let mut observed_allocations = Vec::new();
    while let Some((buffer, length)) = gathered(&gather, &output_rx, &buffers) {
        observed.extend_from_slice(&buffer[..length]);
        observed_allocations.push(buffer.as_ptr() as usize);
        buffers.give(buffer);
    }
    writer.join().expect("fixture writer");

    assert_eq!(observed, expected);
    assert!(
        observed_allocations
            .iter()
            .all(|pointer| allocations.contains(pointer))
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_pty_gather_delivers_an_interactive_partial_batch() {
    let gather = PtyGather::start("zz-pty-gather".to_owned()).expect("gather thread");
    let (write_fd, output_rx, buffers, _) = gather_fixture(&gather, PTY_BUFFER_POOL_SIZE);

    assert_eq!(
        rustix::io::write(&write_fd, b"prompt").expect("interactive fixture write"),
        6
    );
    let (buffer, length) =
        gathered(&gather, &output_rx, &buffers).expect("interactive gather output");
    assert_eq!(&buffer[..length], b"prompt");
    buffers.give(buffer);

    drop(write_fd);
    assert!(gathered(&gather, &output_rx, &buffers).is_none());
}

#[cfg(target_os = "linux")]
#[test]
fn linux_pty_gather_parks_a_starved_pane_without_blocking_its_neighbour() {
    let gather = PtyGather::start("zz-pty-gather".to_owned()).expect("gather thread");
    let (busy_fd, busy_output, busy_buffers, _) = gather_fixture(&gather, 1);
    let (quiet_fd, quiet_output, quiet_buffers, _) = gather_fixture(&gather, 1);

    rustix::io::write(&busy_fd, b"first").expect("busy fixture write");
    let (held, length) = gathered(&gather, &busy_output, &busy_buffers).expect("first busy batch");
    assert_eq!(&held[..length], b"first");
    rustix::io::write(&busy_fd, b"second").expect("busy fixture write");
    rustix::io::write(&quiet_fd, b"quiet").expect("quiet fixture write");
    let (quiet, length) = gathered(&gather, &quiet_output, &quiet_buffers).expect("quiet batch");
    assert_eq!(&quiet[..length], b"quiet");
    assert!(busy_output.is_empty());

    busy_buffers.give(held);
    let (resumed, length) =
        gathered(&gather, &busy_output, &busy_buffers).expect("resumed busy batch");
    assert_eq!(&resumed[..length], b"second");
}

#[cfg(target_os = "linux")]
#[test]
fn linux_pty_gather_delivers_a_partial_batch_beside_a_busy_neighbour() {
    let gather = PtyGather::start("zz-pty-gather".to_owned()).expect("gather thread");
    let (busy_fd, busy_output, busy_buffers, _) = gather_fixture(&gather, PTY_BUFFER_POOL_SIZE);
    let (quiet_fd, quiet_output, quiet_buffers, _) = gather_fixture(&gather, PTY_BUFFER_POOL_SIZE);
    let writer = thread::spawn(move || {
        let chunk = vec![b'b'; PTY_READ_BUFFER_BYTES];
        loop {
            match rustix::io::write(&busy_fd, &chunk) {
                Ok(_) | Err(rustix::io::Errno::INTR) => {}
                Err(rustix::io::Errno::PIPE) => return,
                Err(error) => panic!("busy fixture write failed: {error}"),
            }
        }
    });
    let stop = Arc::new(AtomicBool::new(false));
    let consumer_stop = Arc::clone(&stop);
    let (streaming_tx, streaming_rx) = crossbeam_channel::bounded(1);
    let consumer = thread::spawn(move || {
        while !consumer_stop.load(Ordering::Relaxed) {
            if let Ok(ReaderMessage::Data { buffer, .. }) =
                busy_output.recv_timeout(Duration::from_millis(10))
            {
                let _ = streaming_tx.try_send(());
                busy_buffers.give(buffer);
            }
        }
    });
    streaming_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("the busy pane streams");

    let batch = vec![b'q'; PTY_BRIDGE_THRESHOLD_BYTES * 2];
    assert_eq!(
        rustix::io::write(&quiet_fd, &batch).expect("quiet fixture write"),
        batch.len()
    );
    let (quiet, length) = gathered(&gather, &quiet_output, &quiet_buffers).expect("quiet batch");
    assert_eq!(&quiet[..length], &batch[..]);
    assert!(!writer.is_finished());
    stop.store(true, Ordering::Relaxed);
    consumer.join().expect("busy consumer");
    writer.join().expect("busy writer");
}

#[cfg(target_os = "linux")]
fn gather_threads(name: &str) -> HashSet<u32> {
    std::fs::read_dir("/proc/self/task").map_or_else(
        |_| HashSet::new(),
        |tasks| {
            tasks
                .filter_map(Result::ok)
                .filter(|task| {
                    std::fs::read_to_string(task.path().join("comm"))
                        .is_ok_and(|comm| comm.trim_end() == name)
                })
                .filter_map(|task| task.file_name().to_str()?.parse().ok())
                .collect()
        },
    )
}

#[cfg(target_os = "linux")]
fn lend_pattern(offset: usize, seed: usize, length: usize) -> Vec<u8> {
    (offset..offset + length)
        .map(|index| u8::try_from((index + seed) % 251).expect("bounded byte"))
        .collect()
}

#[cfg(target_os = "linux")]
#[test]
fn linux_pty_gather_lends_a_second_busy_pane_and_takes_it_back() {
    let name = "zz-pty-lend-t";
    let gather = PtyGather::start(name.to_owned()).expect("gather thread");
    let stop_writers = Arc::new(AtomicBool::new(false));
    let stop_readers = Arc::new(AtomicBool::new(false));
    let mut writers = Vec::new();
    let mut readers = Vec::new();
    for seed in [0, 97] {
        let (write_fd, output_rx, buffers, _) = gather_fixture(&gather, PTY_BUFFER_POOL_SIZE);
        let stop = Arc::clone(&stop_writers);
        writers.push(thread::spawn(move || {
            let mut written = 0;
            while !stop.load(Ordering::Relaxed) {
                let chunk = lend_pattern(written, seed, PTY_READ_BUFFER_BYTES);
                let mut offset = 0;
                while offset < chunk.len() {
                    match rustix::io::write(&write_fd, &chunk[offset..]) {
                        Ok(sent) => offset += sent,
                        Err(rustix::io::Errno::INTR) => {}
                        Err(error) => panic!("busy fixture write failed: {error}"),
                    }
                }
                written += chunk.len();
            }
            (write_fd, written)
        }));
        let stop = Arc::clone(&stop_readers);
        let home = gather.clone();
        readers.push(thread::spawn(move || {
            let mut observed = 0;
            let mut check = |message: ReaderMessage| match message {
                ReaderMessage::Data { buffer, length } => {
                    assert_eq!(buffer[..length], lend_pattern(observed, seed, length)[..]);
                    observed += length;
                    buffers.give(buffer);
                }
                ReaderMessage::Home => {
                    assert!(home.lend(sent_home(&buffers)).is_ok());
                }
                ReaderMessage::Eof => panic!("busy fixture closed"),
            };
            while !stop.load(Ordering::Relaxed) {
                if let Ok(message) = output_rx.recv_timeout(Duration::from_millis(10)) {
                    check(message);
                }
            }
            while let Ok(message) = output_rx.try_recv() {
                check(message);
            }
            (observed, output_rx, buffers)
        }));
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    while gather_threads(name).len() < 2 {
        assert!(Instant::now() < deadline, "no busy pane was lent");
        thread::sleep(Duration::from_millis(1));
    }
    stop_writers.store(true, Ordering::Relaxed);
    let writers = writers
        .into_iter()
        .map(|writer| writer.join().expect("busy writer"))
        .collect::<Vec<_>>();
    let deadline = Instant::now() + Duration::from_secs(5);
    while gather_threads(name).len() > 1 {
        assert!(Instant::now() < deadline, "the lent pane never came back");
        thread::sleep(Duration::from_millis(5));
    }
    stop_readers.store(true, Ordering::Relaxed);
    let readers = readers
        .into_iter()
        .map(|reader| reader.join().expect("busy reader"))
        .collect::<Vec<_>>();

    for (((write_fd, written), (observed, output_rx, buffers)), seed) in
        writers.into_iter().zip(readers).zip([0, 97])
    {
        assert_eq!(observed, written);
        let batch = lend_pattern(written, seed, PTY_BRIDGE_THRESHOLD_BYTES / 2);
        assert_eq!(
            rustix::io::write(&write_fd, &batch).expect("returned pane write"),
            batch.len()
        );
        let (buffer, length) =
            gathered(&gather, &output_rx, &buffers).expect("returned pane batch");
        assert_eq!(&buffer[..length], &batch[..]);
        buffers.give(buffer);
    }
}

#[cfg(target_os = "linux")]
fn lent_out(gather: &PtyGather, buffers: &BufferReturn) -> bool {
    let owner = buffers.gather.slot.owner.lock();
    matches!(
        (&owner.pipe, &gather.panes.wake.pipe),
        (Some(owner), Some(home)) if !Arc::ptr_eq(owner, home)
    )
}

#[cfg(target_os = "linux")]
#[test]
fn linux_pty_gather_lends_a_bursting_pane_to_the_same_thread_each_time() {
    let name = "zz-pty-reuse-t";
    let gather = PtyGather::start(name.to_owned()).expect("gather thread");
    let (flood_fd, flood_output, flood_buffers, _) = gather_fixture(&gather, PTY_BUFFER_POOL_SIZE);
    let (burst_fd, burst_output, burst_buffers, _) = gather_fixture(&gather, PTY_BUFFER_POOL_SIZE);
    let flood = thread::spawn(move || {
        let chunk = vec![b'f'; PTY_READ_BUFFER_BYTES];
        loop {
            match rustix::io::write(&flood_fd, &chunk) {
                Ok(_) | Err(rustix::io::Errno::INTR) => {}
                Err(rustix::io::Errno::PIPE) => return,
                Err(error) => panic!("flood fixture write failed: {error}"),
            }
        }
    });
    let stop = Arc::new(AtomicBool::new(false));
    let consumer_stop = Arc::clone(&stop);
    let consumer = thread::spawn(move || {
        while !consumer_stop.load(Ordering::Relaxed) {
            if let Ok(ReaderMessage::Data { buffer, .. }) =
                flood_output.recv_timeout(Duration::from_millis(10))
            {
                flood_buffers.give(buffer);
            }
        }
    });

    let mut threads = HashSet::new();
    let mut lends = 0;
    let mut offset = 0;
    let deadline = Instant::now() + Duration::from_secs(20);
    while lends < 3 {
        assert!(Instant::now() < deadline, "the bursting pane was not lent");
        let burst = lend_pattern(offset, 7, PTY_READ_BUFFER_BYTES + 100);
        let writer = {
            let burst = burst.clone();
            let burst_fd = burst_fd.try_clone().expect("burst writer");
            thread::spawn(move || write_fixture(&burst_fd, &burst))
        };
        let mut observed = 0;
        let mut lent = false;
        while observed < burst.len() {
            let (buffer, length) =
                gathered(&gather, &burst_output, &burst_buffers).expect("burst batch");
            assert_eq!(&buffer[..length], &burst[observed..observed + length]);
            observed += length;
            burst_buffers.give(buffer);
            lent |= lent_out(&gather, &burst_buffers);
            threads.extend(gather_threads(name));
        }
        writer.join().expect("burst writer");
        offset += burst.len();
        let back = Instant::now() + Duration::from_secs(2);
        while lent_out(&gather, &burst_buffers) {
            assert!(Instant::now() < back, "the bursting pane never came back");
            threads.extend(gather_threads(name));
            thread::sleep(Duration::from_millis(5));
        }
        lends += usize::from(lent);
    }
    stop.store(true, Ordering::Relaxed);
    consumer.join().expect("flood consumer");
    flood.join().expect("flood writer");
    assert_eq!(threads.len(), 2, "lent threads {threads:?}");
}

#[cfg(target_os = "linux")]
#[test]
fn linux_pty_gather_stops_after_its_panes_and_last_handle_are_gone() {
    let gather = PtyGather::start("zz-pty-gather".to_owned()).expect("gather thread");
    let (write_fd, _output_rx, buffers, _) = gather_fixture(&gather, 1);
    drop(buffers);
    let deadline = Instant::now() + Duration::from_secs(2);
    while rustix::io::write(&write_fd, b"x") != Err(rustix::io::Errno::PIPE) {
        assert!(
            Instant::now() < deadline,
            "gather kept the released fd open"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let home = Arc::clone(&gather.thread);
    drop(gather);
    let deadline = Instant::now() + Duration::from_secs(2);
    while !home.is_finished() {
        assert!(
            Instant::now() < deadline,
            "the gather thread outlived its last handle"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(target_os = "linux")]
#[test]
fn linux_pty_gather_closes_the_fd_of_a_released_pane() {
    let gather = PtyGather::start("zz-pty-gather".to_owned()).expect("gather thread");
    let (write_fd, output_rx, buffers, _) = gather_fixture(&gather, 1);
    drop(gather);
    drop(buffers);
    let deadline = Instant::now() + Duration::from_secs(2);
    while rustix::io::write(&write_fd, b"x") != Err(rustix::io::Errno::PIPE) {
        assert!(
            Instant::now() < deadline,
            "gather kept the released fd open"
        );
        thread::sleep(Duration::from_millis(5));
    }
    loop {
        match output_rx.recv_timeout(Duration::from_secs(2)) {
            Ok(_) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                panic!("gather kept the released pane")
            }
        }
    }
}
