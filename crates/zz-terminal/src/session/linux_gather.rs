use super::*;

#[cfg(target_os = "linux")]
impl LinuxChildWatch {
    pub(super) fn reap(&self) -> bool {
        if self.reaped.get() {
            return true;
        }
        let Some(status) = reap_child(self.pid, false) else {
            return false;
        };
        self.reaped.set(true);
        let _ = self.exit.try_send(status);
        self.wake.notify();
        true
    }
}

#[cfg(target_os = "linux")]
impl Drop for LinuxChildWatch {
    fn drop(&mut self) {
        if self.reap() {
            return;
        }
        let pid = self.pid;
        if let Err(error) = thread::Builder::new()
            .name("zz-child-reap".into())
            .spawn(move || {
                let _ = reap_child(pid, true);
            })
        {
            log::warn!(
                "could not wait for pane child {}: {error}",
                pid.as_raw_nonzero()
            );
        }
    }
}

#[cfg(target_os = "linux")]
pub(super) fn watch_child_linux(
    process_id: Option<u32>,
    exit: Sender<std::io::Result<ExitStatus>>,
    wake: ActorWake,
) -> Result<Option<LinuxChildWatch>, WorkerError> {
    let pid = child_pid(process_id)?;
    if let Ok(pidfd) = rustix::process::pidfd_open(pid, rustix::process::PidfdFlags::empty()) {
        return Ok(Some(LinuxChildWatch {
            pid,
            pidfd,
            exit,
            wake,
            reaped: Cell::new(false),
        }));
    }
    thread::Builder::new()
        .name("zz-child-wait".into())
        .spawn(move || {
            let status = reap_child(pid, true).unwrap_or_else(|| {
                Err(std::io::Error::other("the pane child was reaped elsewhere"))
            });
            let _ = exit.try_send(status);
            wake.notify();
        })
        .map_err(WorkerError::Io)?;
    Ok(None)
}

#[cfg(target_os = "linux")]
pub(super) fn pty_gather_enabled() -> bool {
    std::env::var_os("ZZ_PTY_GATHER").is_none_or(|value| value != "0")
}

#[cfg(target_os = "linux")]
impl GatherSender {
    fn new(wake: ActorWake) -> (Self, Receiver<GatherPane>) {
        let (panes, receiver) = crossbeam_channel::unbounded();
        (
            Self {
                panes: Some(panes),
                wake,
            },
            receiver,
        )
    }

    fn send(&self, pane: GatherPane, back: &ActorWake) -> Result<(), GatherPane> {
        pane.source.slot.move_to(&self.wake);
        let sent = match &self.panes {
            Some(panes) => panes
                .send(pane)
                .map_err(crossbeam_channel::SendError::into_inner),
            None => Err(pane),
        };
        match &sent {
            Ok(()) => self.wake.notify(),
            Err(pane) => pane.source.slot.move_to(back),
        }
        sent
    }

    fn close(&mut self) {
        if self.panes.take().is_some() {
            self.wake.notify();
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for GatherSender {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(target_os = "linux")]
impl GatherSlot {
    fn notify(&self) {
        self.owner.lock().notify();
    }

    fn move_to(&self, owner: &ActorWake) {
        *self.owner.lock() = owner.clone();
    }
}

#[cfg(target_os = "linux")]
struct GatherWake {
    rx: std::os::fd::OwnedFd,
    pending: Arc<AtomicBool>,
    wake: ActorWake,
}

#[cfg(target_os = "linux")]
impl GatherWake {
    fn new() -> Result<Self, WorkerError> {
        let (rx, pipe) =
            configured_actor_wake_pipe().map_err(|error| WorkerError::Io(error.into()))?;
        let pending = Arc::new(AtomicBool::new(false));
        let wake = ActorWake {
            ready: None,
            pipe: Some(Arc::new(pipe)),
            pending: Some(Arc::clone(&pending)),
        };
        Ok(Self { rx, pending, wake })
    }
}

#[cfg(target_os = "linux")]
impl PtyGather {
    pub(super) fn start(name: String) -> Result<Self, WorkerError> {
        let wake = GatherWake::new()?;
        let (panes, incoming) = GatherSender::new(wake.wake.clone());
        let thread = thread::Builder::new()
            .name(name.clone())
            .spawn(move || Gather::new(name, wake, incoming, GatherRole::Home).run())
            .map_err(WorkerError::Io)?;
        Ok(Self {
            panes: Arc::new(panes),
            thread: Arc::new(thread),
        })
    }

    pub(super) fn stopped(&self) -> bool {
        self.thread.is_finished()
    }

    pub(super) fn lend(&self, mut pane: GatherPane) -> Result<(), GatherPane> {
        pane.filled = Some(Instant::now());
        pane.sent = pane.filled;
        let back = pane.source.wake.clone();
        self.panes.send(pane, &back)
    }
}

#[cfg(target_os = "linux")]
impl DirectReader {
    pub(super) fn new(gather: Option<PtyGather>, pane: GatherPane) -> (Self, Vec<u8>) {
        let mut reader = Self {
            gather,
            pane: None,
            last: Instant::now(),
            bytes: 0,
        };
        let buffer = reader.park(pane);
        (reader, buffer)
    }

    pub(super) fn fd(&self) -> Option<&filedescriptor::FileDescriptor> {
        self.pane.as_ref().map(|pane| &pane.source.fd)
    }

    pub(super) fn park(&mut self, mut pane: GatherPane) -> Vec<u8> {
        let buffer = pane
            .buffer
            .take()
            .or_else(|| pane.source.recycled.try_recv().ok())
            .unwrap_or_else(|| vec![0_u8; PTY_READ_BUFFER_BYTES]);
        self.pane = Some(pane);
        self.bytes = 0;
        buffer
    }

    pub(super) fn lend_if_busy(
        &mut self,
        read: usize,
        buffer: &mut Vec<u8>,
        buffers: &BufferReturn,
    ) -> bool {
        if self.gather.is_none() {
            return false;
        }
        let now = Instant::now();
        if now.saturating_duration_since(self.last) >= PTY_GATHER_LEND_BUSY {
            self.bytes = 0;
        }
        self.last = now;
        self.bytes += read;
        if self.bytes < PTY_READ_BUFFER_BYTES {
            return false;
        }
        let (Some(gather), Some(pane)) = (&self.gather, self.pane.take()) else {
            return false;
        };
        buffers.give(std::mem::take(buffer));
        let Err(pane) = gather.lend(pane) else {
            return true;
        };
        *buffer = self.park(pane);
        false
    }
}

#[cfg(target_os = "linux")]
impl GatherLease {
    pub(super) fn take_home(&self) -> Option<GatherPane> {
        self.slot.home.lock().take()
    }

    pub(super) fn returned(&self) {
        std::sync::atomic::fence(Ordering::SeqCst);
        if self.slot.starved.load(Ordering::Relaxed)
            && self.slot.starved.swap(false, Ordering::AcqRel)
        {
            self.slot.notify();
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for GatherLease {
    fn drop(&mut self) {
        let parked = {
            let mut home = self.slot.home.lock();
            self.slot.closed.store(true, Ordering::Release);
            home.take()
        };
        drop(parked);
        self.slot.notify();
    }
}

#[cfg(target_os = "linux")]
impl GatherPane {
    const fn new(source: GatherSource) -> Self {
        Self {
            source,
            buffer: None,
            length: 0,
            spins: 0,
            active: false,
            waiting: false,
            done: false,
            filled: None,
            sent: None,
        }
    }

    pub(super) fn open(
        fd: filedescriptor::FileDescriptor,
        output: Sender<ReaderMessage>,
        recycled: Receiver<Vec<u8>>,
        wake: ActorWake,
    ) -> (Self, GatherLease) {
        let slot = Arc::new(GatherSlot {
            starved: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            owner: Mutex::new(wake.clone()),
            home: Mutex::new(None),
        });
        (
            Self::new(GatherSource {
                fd,
                output,
                recycled,
                wake,
                slot: Arc::clone(&slot),
            }),
            GatherLease { slot },
        )
    }

    fn go_home(mut self) -> Result<(), Self> {
        self.send();
        if self.done || self.source.output.is_full() {
            return Err(self);
        }
        self.active = false;
        self.waiting = false;
        self.spins = 0;
        let slot = Arc::clone(&self.source.slot);
        let output = self.source.output.clone();
        let wake = self.source.wake.clone();
        {
            let mut home = slot.home.lock();
            if slot.closed.load(Ordering::Acquire) {
                return Err(self);
            }
            slot.move_to(&wake);
            *home = Some(self);
        }
        if output.try_send(ReaderMessage::Home).is_ok() {
            wake.notify();
        }
        Ok(())
    }

    fn has_buffer(&mut self) -> bool {
        if self.buffer.is_none() {
            self.buffer = self.source.recycled.try_recv().ok().or_else(|| {
                self.source.slot.starved.store(true, Ordering::Relaxed);
                std::sync::atomic::fence(Ordering::SeqCst);
                let buffer = self.source.recycled.try_recv().ok();
                if buffer.is_some() {
                    self.source.slot.starved.store(false, Ordering::Relaxed);
                }
                buffer
            });
        }
        self.buffer.is_some()
    }

    fn step(&mut self) -> bool {
        let Some(buffer) = self.buffer.as_mut() else {
            return false;
        };
        debug_assert_eq!(buffer.len(), PTY_READ_BUFFER_BYTES);
        match rustix::io::read(&self.source.fd, &mut buffer[self.length..]) {
            Ok(0) | Err(rustix::io::Errno::IO) => {}
            Ok(read) => {
                self.length += read;
                self.spins = 0;
                self.waiting = false;
                if self.length < buffer.len() {
                    return true;
                }
                self.filled = Some(Instant::now());
                self.send();
                return !self.done && self.has_buffer();
            }
            Err(rustix::io::Errno::INTR) => return true,
            Err(rustix::io::Errno::AGAIN) => {
                self.waiting = self.length >= PTY_BRIDGE_THRESHOLD_BYTES;
                if !self.waiting {
                    self.send();
                }
                return false;
            }
            Err(error) => log::debug!("Linux PTY gather stopped: {error}"),
        }
        self.send();
        if !self.done {
            let _ = self.source.output.try_send(ReaderMessage::Eof);
            self.source.wake.notify();
        }
        self.done = true;
        false
    }

    fn missed(&mut self) {
        if !self.waiting {
            return;
        }
        self.spins += 1;
        if self.spins > PTY_GATHER_BRIDGE_SPIN_MAX {
            self.waiting = false;
            self.spins = 0;
            self.send();
        }
    }

    fn send(&mut self) {
        if self.length == 0 {
            return;
        }
        let length = std::mem::take(&mut self.length);
        let Some(buffer) = self.buffer.take() else {
            return;
        };
        if self
            .source
            .output
            .try_send(ReaderMessage::Data { buffer, length })
            .is_ok()
        {
            self.sent = Some(Instant::now());
            self.source.wake.notify();
        } else {
            self.done = true;
        }
    }

    fn quiet_for(&self, now: Instant) -> Duration {
        self.filled.map_or(Duration::MAX, |filled| {
            now.saturating_duration_since(filled)
        })
    }

    fn released(&self) -> bool {
        self.done || self.source.slot.closed.load(Ordering::Acquire)
    }
}

#[cfg(target_os = "linux")]
impl Drop for GatherPane {
    fn drop(&mut self) {
        if !self.released() {
            let _ = self.source.output.try_send(ReaderMessage::Eof);
            self.source.wake.notify();
        }
    }
}

#[cfg(target_os = "linux")]
fn poll_gather(
    wake_rx: &std::os::fd::OwnedFd,
    panes: &mut [GatherPane],
    timeout: Option<&rustix::event::Timespec>,
) -> rustix::io::Result<(bool, SmallVec<[usize; 8]>)> {
    use rustix::event::{PollFd, PollFlags};

    let polled = panes
        .iter_mut()
        .enumerate()
        .filter(|(_, pane)| !pane.done && !pane.active)
        .filter_map(|(index, pane)| pane.has_buffer().then_some(index))
        .collect::<SmallVec<[usize; 8]>>();
    let mut fds = SmallVec::<[PollFd<'_>; 9]>::new();
    fds.push(PollFd::new(wake_rx, PollFlags::IN));
    fds.extend(
        polled
            .iter()
            .map(|&index| PollFd::new(&panes[index].source.fd, PollFlags::IN)),
    );
    match rustix::event::poll(&mut fds, timeout) {
        Ok(_) => {}
        Err(rustix::io::Errno::INTR) => return Ok((false, SmallVec::new())),
        Err(error) => return Err(error),
    }
    Ok((
        !fds[0].revents().is_empty(),
        polled
            .iter()
            .zip(&fds[1..])
            .filter(|(_, fd)| !fd.revents().is_empty())
            .map(|(&index, _)| index)
            .collect(),
    ))
}

#[cfg(target_os = "linux")]
fn read_gather_passes(panes: &mut [GatherPane]) -> bool {
    let mut filled = false;
    for _ in 0..PTY_GATHER_ADMIT_PASSES {
        let mut active = false;
        for pane in panes.iter_mut().filter(|pane| pane.active) {
            let before = pane.filled;
            pane.active = pane.step();
            filled |= pane.filled != before;
            active |= pane.active;
        }
        if !active {
            break;
        }
    }
    filled
}

#[cfg(target_os = "linux")]
enum GatherRole {
    Home,
    Lent(GatherSender),
    Orphaned,
}

#[cfg(target_os = "linux")]
struct GatherLender {
    give: GatherSender,
    returns: Receiver<GatherPane>,
    idle: Option<Instant>,
}

#[cfg(target_os = "linux")]
struct Gather {
    name: String,
    wake: GatherWake,
    panes: Vec<GatherPane>,
    incoming: Option<Receiver<GatherPane>>,
    lent: Vec<GatherLender>,
    role: GatherRole,
}

#[cfg(target_os = "linux")]
impl Gather {
    const fn new(
        name: String,
        wake: GatherWake,
        incoming: Receiver<GatherPane>,
        role: GatherRole,
    ) -> Self {
        Self {
            name,
            wake,
            panes: Vec::new(),
            incoming: Some(incoming),
            lent: Vec::new(),
            role,
        }
    }

    fn run(mut self) {
        let now = rustix::event::Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        loop {
            self.accept();
            self.release();
            let home = match self.role {
                GatherRole::Home => self.hand_back(),
                _ => None,
            };
            let busy = self.panes.iter().any(|pane| pane.active || pane.waiting);
            let idle = match self.role {
                _ if busy => None,
                GatherRole::Home => self.retire().into_iter().chain(home).min(),
                GatherRole::Lent(_) => self.give_back(),
                GatherRole::Orphaned => None,
            };
            if self.panes.is_empty() && self.lent.is_empty() && self.incoming.is_none() {
                return;
            }
            let timeout = if busy {
                Some(now)
            } else {
                idle.map(|idle| rustix::event::Timespec::try_from(idle).unwrap_or(now))
            };
            let (woken, ready) = match poll_gather(&self.wake.rx, &mut self.panes, timeout.as_ref())
            {
                Ok(polled) => polled,
                Err(error) => {
                    log::error!("Linux PTY gather poll stopped: {error}");
                    return;
                }
            };
            if woken {
                let _ = drain_wake_pipe(&self.wake.rx);
                self.wake.pending.store(false, Ordering::SeqCst);
            }
            for index in ready {
                self.panes[index].active = true;
            }
            for pane in self.panes.iter_mut().filter(|pane| !pane.active) {
                pane.missed();
            }
            if read_gather_passes(&mut self.panes) && matches!(self.role, GatherRole::Home) {
                self.lend();
            }
        }
    }

    fn accept(&mut self) {
        if let Some(incoming) = &self.incoming {
            loop {
                match incoming.try_recv() {
                    Ok(pane) => self.panes.push(pane),
                    Err(crossbeam_channel::TryRecvError::Empty) => break,
                    Err(crossbeam_channel::TryRecvError::Disconnected) => {
                        self.incoming = None;
                        break;
                    }
                }
            }
        }
        self.lent.retain_mut(|lender| {
            loop {
                match lender.returns.try_recv() {
                    Ok(pane) => {
                        self.panes.push(pane);
                        lender.idle = Some(Instant::now());
                    }
                    Err(crossbeam_channel::TryRecvError::Empty) => return true,
                    Err(crossbeam_channel::TryRecvError::Disconnected) => return false,
                }
            }
        });
    }

    fn release(&mut self) {
        for pane in self.panes.extract_if(.., |pane| pane.released()) {
            if let GatherRole::Lent(home) = &self.role {
                let _ = home.send(pane, &self.wake.wake);
            }
        }
    }

    fn lend(&mut self) {
        let now = Instant::now();
        let mut kept = false;
        let mut index = 0;
        while index < self.panes.len() && self.lender_free() {
            if self.panes[index].done || self.panes[index].quiet_for(now) >= PTY_GATHER_LEND_BUSY {
                index += 1;
                continue;
            }
            if !kept {
                kept = true;
                index += 1;
                continue;
            }
            let pane = self.panes.swap_remove(index);
            if let Err(pane) = self.lend_pane(pane) {
                self.panes.push(pane);
                return;
            }
        }
    }

    fn lender_free(&self) -> bool {
        self.lent.len() < PTY_GATHER_LENT_MAX
            || self.lent.iter().any(|lender| lender.idle.is_some())
    }

    fn lend_pane(&mut self, pane: GatherPane) -> Result<(), GatherPane> {
        if self.lent.iter().all(|lender| lender.idle.is_none()) {
            match self.spawn_lent() {
                Ok(lender) => self.lent.push(lender),
                Err(error) => log::debug!("Linux PTY gather could not lend a busy pane: {error}"),
            }
        }
        let Some(lender) = self.lent.iter_mut().find(|lender| lender.idle.is_some()) else {
            return Err(pane);
        };
        lender.give.send(pane, &self.wake.wake)?;
        lender.idle = None;
        Ok(())
    }

    fn spawn_lent(&self) -> Result<GatherLender, WorkerError> {
        let wake = GatherWake::new()?;
        let (give, take) = GatherSender::new(wake.wake.clone());
        let (home, returns) = GatherSender::new(self.wake.wake.clone());
        let name = self.name.clone();
        thread::Builder::new()
            .name(name.clone())
            .spawn(move || Gather::new(name, wake, take, GatherRole::Lent(home)).run())
            .map_err(WorkerError::Io)?;
        Ok(GatherLender {
            give,
            returns,
            idle: Some(Instant::now()),
        })
    }

    fn retire(&mut self) -> Option<Duration> {
        if self.lent.is_empty() {
            return None;
        }
        let linger = if self.incoming.is_some() {
            PTY_GATHER_LEND_LINGER
        } else {
            Duration::ZERO
        };
        let now = Instant::now();
        for lender in &mut self.lent {
            if lender
                .idle
                .is_some_and(|idle| now.saturating_duration_since(idle) >= linger)
            {
                lender.give.close();
                lender.idle = None;
            }
        }
        self.lent
            .iter()
            .filter_map(|lender| lender.idle)
            .map(|idle| linger.saturating_sub(now.saturating_duration_since(idle)))
            .min()
    }

    fn hand_back(&mut self) -> Option<Duration> {
        let now = Instant::now();
        let mut wait = None::<Duration>;
        let mut index = 0;
        while index < self.panes.len() {
            let pane = &mut self.panes[index];
            if pane.done || pane.active || pane.waiting {
                index += 1;
                continue;
            }
            let quiet = pane
                .sent
                .map_or(Duration::MAX, |sent| now.saturating_duration_since(sent));
            if quiet < PTY_GATHER_LEND_IDLE {
                let left = PTY_GATHER_LEND_IDLE.saturating_sub(quiet);
                wait = Some(wait.map_or(left, |wait| wait.min(left)));
                index += 1;
                continue;
            }
            if !pane.has_buffer() {
                index += 1;
                continue;
            }
            if let Err(pane) = self.panes.swap_remove(index).go_home() {
                self.panes.push(pane);
                return Some(
                    wait.map_or(PTY_GATHER_LEND_BUSY, |wait| wait.min(PTY_GATHER_LEND_BUSY)),
                );
            }
        }
        wait
    }

    fn give_back(&mut self) -> Option<Duration> {
        let GatherRole::Lent(home) = &self.role else {
            return None;
        };
        let now = Instant::now();
        let mut wait = PTY_GATHER_LEND_IDLE;
        let mut index = 0;
        while index < self.panes.len() {
            let quiet = self.panes[index].quiet_for(now);
            if quiet < PTY_GATHER_LEND_IDLE {
                wait = wait.min(PTY_GATHER_LEND_IDLE.saturating_sub(quiet));
                index += 1;
                continue;
            }
            let pane = self.panes.swap_remove(index);
            if let Err(pane) = home.send(pane, &self.wake.wake) {
                self.panes.push(pane);
                self.role = GatherRole::Orphaned;
                return None;
            }
        }
        (!self.panes.is_empty()).then_some(wait)
    }
}
