use super::*;

impl Shared {
    pub(super) fn signal_wait_channel(&self, name: &str) {
        let wakes = {
            let mut inner = self.inner.lock();
            let channel = inner
                .waits
                .wait_channels
                .entry(name.to_owned())
                .or_default();
            if channel.waiters.is_empty() && !channel.woken {
                channel.woken = true;
                return;
            }
            let wakes = channel
                .waiters
                .drain(..)
                .map(|item| item.continuation)
                .collect::<Vec<_>>();
            remove_wait_channel_if_unused(&mut inner.waits.wait_channels, name);
            wakes
        };
        self.wake_wait_items(wakes);
    }

    pub(super) fn wake_wait_items(&self, wakes: impl IntoIterator<Item = cmdq::WaitContinuation>) {
        let mut completed = false;
        for wake in wakes {
            completed |= wake.complete();
        }
        if completed {
            self.accept_wake.wake();
        }
    }

    #[cfg(test)]
    pub(super) fn wait_for(
        &self,
        client: ClientId,
        kind: ClientKind,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        self.wait_for_in(client, kind, &ExecutionContext::default(), args)
    }

    pub(super) fn wait_for_in(
        &self,
        client: ClientId,
        kind: ClientKind,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let parsed = parse_buffer_command_args(
            "wait-for",
            args,
            &['F', 'w'],
            &['E', 'L', 'S', 'U', 'l', 'v'],
        )?;
        let [name] = parsed.positional.as_slice() else {
            return Err(ServerError::CommandParse(WAIT_FOR_USAGE.to_owned()).into());
        };
        if parsed.has('E') {
            return self.wait_for_event(
                client,
                context,
                name,
                parsed.value('F'),
                parsed.has('v'),
                parsed.has('l'),
                parsed.value('w'),
            );
        }
        if parsed.has('l') {
            let inner = self.inner.lock();
            let lines = inner
                .waits
                .wait_channels
                .get(name)
                .map(|channel| {
                    channel
                        .waiters
                        .iter()
                        .chain(&channel.lockers)
                        .map(|item| client_format_name(&inner, item.client) + "\n")
                        .collect::<String>()
                })
                .unwrap_or_default();
            return Ok(Execution {
                output: lines.into(),
                ..Execution::default()
            });
        }
        if let Some(waiter) = parsed.value('w') {
            let wake = {
                let mut inner = self.inner.lock();
                let names = inner.waits.wait_channels.get(name).map(|channel| {
                    (
                        channel
                            .waiters
                            .iter()
                            .position(|item| client_format_name(&inner, item.client) == waiter),
                        channel
                            .lockers
                            .iter()
                            .position(|item| client_format_name(&inner, item.client) == waiter),
                    )
                });
                let channel = inner.waits.wait_channels.get_mut(name);
                let item = match (channel, names) {
                    (Some(channel), Some((Some(index), _))) => channel.waiters.remove(index),
                    (Some(channel), Some((None, Some(index)))) => channel.lockers.remove(index),
                    _ => None,
                };
                if inner.waits.wait_channels.get(name).is_some_and(|channel| {
                    !channel.locked
                        && !channel.woken
                        && channel.waiters.is_empty()
                        && channel.lockers.is_empty()
                }) {
                    inner.waits.wait_channels.remove(name);
                }
                item.map(|item| item.continuation)
            };
            self.wake_wait_items(wake);
            return Ok(Execution::default());
        }
        if parsed.has('S') {
            self.signal_wait_channel(name);
            return Ok(Execution::default());
        }
        let wake_owner = self.client_writers.lock().get(&client).map(Arc::downgrade);
        if parsed.has('L') {
            if !matches!(kind, ClientKind::Command | ClientKind::Control)
                || client == ClientId(u64::MAX)
            {
                return Err(ServerError::InvalidCommand("not able to lock".to_owned()).into());
            }
            let continuation = {
                let mut inner = self.inner.lock();
                if self.stopping.load(Ordering::Acquire)
                    || self.shutdown_pending.load(Ordering::Acquire)
                    || inner.client(client).is_none_or(|c| c.instance_id.is_none())
                {
                    return Ok(Execution::default());
                }
                let channel = inner.waits.wait_channels.entry(name.clone()).or_default();
                if !channel.locked {
                    channel.locked = true;
                    return Ok(Execution::default());
                }
                let continuation = cmdq::WaitContinuation::new(
                    self.command_item
                        .as_ref()
                        .and_then(|item| item.lock().wait()),
                    wake_owner.clone(),
                );
                channel.lockers.push_back(WaitItem {
                    client,
                    continuation: continuation.clone(),
                });
                continuation
            };
            self.park_wait_for(name, continuation)?;
            return Ok(Execution::default());
        }
        if parsed.has('U') {
            let wake = {
                let mut inner = self.inner.lock();
                let Some(channel) = inner.waits.wait_channels.get_mut(name) else {
                    return Err(
                        ServerError::InvalidCommand(format!("channel {name} not locked")).into(),
                    );
                };
                if !channel.locked {
                    return Err(
                        ServerError::InvalidCommand(format!("channel {name} not locked")).into(),
                    );
                }
                channel.granted = None;
                if let Some(item) = channel.lockers.pop_front() {
                    let continuation = item.continuation.clone();
                    channel.granted = Some(item);
                    Some(continuation)
                } else {
                    channel.locked = false;
                    remove_wait_channel_if_unused(&mut inner.waits.wait_channels, name);
                    None
                }
            };
            self.wake_wait_items(wake);
            return Ok(Execution::default());
        }
        if !matches!(kind, ClientKind::Command | ClientKind::Control)
            || client == ClientId(u64::MAX)
        {
            return Err(ServerError::InvalidCommand("not able to wait".to_owned()).into());
        }
        let continuation = {
            let mut inner = self.inner.lock();
            if self.stopping.load(Ordering::Acquire)
                || self.shutdown_pending.load(Ordering::Acquire)
                || inner.client(client).is_none_or(|c| c.instance_id.is_none())
            {
                return Ok(Execution::default());
            }
            let channel = inner.waits.wait_channels.entry(name.clone()).or_default();
            if channel.woken {
                remove_wait_channel_if_unused(&mut inner.waits.wait_channels, name);
                return Ok(Execution::default());
            }
            let continuation = cmdq::WaitContinuation::new(
                self.command_item
                    .as_ref()
                    .and_then(|item| item.lock().wait()),
                wake_owner.clone(),
            );
            channel.waiters.push_back(WaitItem {
                client,
                continuation: continuation.clone(),
            });
            continuation
        };
        self.park_wait_for(name, continuation)?;
        Ok(Execution::default())
    }

    #[cfg_attr(test, allow(clippy::unnecessary_wraps))]
    fn park_wait_for(
        &self,
        name: &str,
        continuation: cmdq::WaitContinuation,
    ) -> Result<(), DaemonError> {
        self.report_command_queue_park();
        if let Some(item) = &self.command_item {
            let mut item = item.lock();
            if item.loop_wait {
                item.pending_wait = Some(Box::new(RegisteredWait {
                    name: name.to_owned(),
                    continuation,
                    #[cfg(unix)]
                    shell: None,
                    overlay: None,
                    leaf: None,
                    guard: None,
                    terminal: None,
                    file: None,
                }));
                return Ok(());
            }
        }
        #[cfg(all(unix, not(test)))]
        if self.loop_active.load(Ordering::Acquire) {
            let next = remove_wait_item(&mut self.inner.lock(), name, continuation.token, true);
            self.wake_wait_items(next);
            return Err(ServerError::InvalidCommand(
                "not able to wait outside a command queue".to_owned(),
            )
            .into());
        }
        #[cfg(any(test, windows))]
        {
            continuation.wait();
            remove_wait_item(&mut self.inner.lock(), name, continuation.token, false);
            Ok(())
        }
        #[cfg(all(unix, not(test)))]
        {
            let next = remove_wait_item(&mut self.inner.lock(), name, continuation.token, true);
            self.wake_wait_items(next);
            Err(
                ServerError::InvalidCommand("not able to wait outside a command queue".to_owned())
                    .into(),
            )
        }
    }
}

const WAIT_FOR_USAGE: &str = "usage: wait-for [-ELSUlv] [-F format] [-w waiter] name";
