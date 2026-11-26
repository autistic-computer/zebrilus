use super::*;

impl Shared {
    pub(super) fn simulator_command(
        self: &Arc<Self>,
        context: &ExecutionContext,
        name: &str,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let spec = zz_protocol::catalog_command_spec(name).expect("simulator verbs are cataloged");
        let name = spec.name;
        let parsed = zz_protocol::parse_tmux_options(spec, args)?;
        let mut target = None;
        let mut output = None;
        for option in parsed.options {
            match option {
                zz_protocol::TmuxOption::Value("-t", value) => target = Some(value.to_owned()),
                zz_protocol::TmuxOption::Value("-o", value) => output = Some(value.to_owned()),
                _ => {}
            }
        }
        let words = parsed
            .positionals
            .iter()
            .map(RawText::as_str)
            .collect::<Vec<_>>();
        let number = |word: &str| {
            word.parse::<u32>().map_err(|_| {
                ServerError::CommandParse(format!("{name} takes whole pixel numbers, not {word:?}"))
            })
        };
        let screenshot = match name {
            "capture-simulator" => {
                let output = output.ok_or_else(|| {
                    ServerError::CommandParse(format!("{name} needs an output path (-o)"))
                })?;
                let path = expand_path(&output);
                if !path.is_absolute() {
                    return Err(ServerError::InvalidCommand(format!(
                        "{name} output path must be absolute: {output}"
                    ))
                    .into());
                }
                Some(
                    path.to_str()
                        .ok_or_else(|| {
                            ServerError::InvalidCommand(format!(
                                "{name} output path is not valid UTF-8"
                            ))
                        })?
                        .to_owned(),
                )
            }
            _ => None,
        };
        let command = match (name, words.as_slice()) {
            ("capture-simulator", []) => {
                let path = screenshot.clone().unwrap_or_default();
                Box::new(move |request_id| SimulatorCommand::Screenshot { request_id, path })
                    as Box<dyn FnOnce(u64) -> SimulatorCommand>
            }
            ("tap-simulator", [x, y]) => {
                let (x, y) = (number(x)?, number(y)?);
                Box::new(move |request_id| SimulatorCommand::Tap { request_id, x, y })
            }
            ("swipe-simulator", [x1, y1, x2, y2]) => {
                let from = (number(x1)?, number(y1)?);
                let to = (number(x2)?, number(y2)?);
                Box::new(move |request_id| SimulatorCommand::Swipe {
                    request_id,
                    from,
                    to,
                })
            }
            ("type-simulator", words) if !words.is_empty() => {
                let text = words.join(" ");
                if text.len() > zz_protocol::MAX_GUI_TEXT_BYTES {
                    return Err(ServerError::InvalidCommand(format!(
                        "{name} text must be at most {} bytes",
                        zz_protocol::MAX_GUI_TEXT_BYTES
                    ))
                    .into());
                }
                Box::new(move |request_id| SimulatorCommand::Type { request_id, text })
            }
            ("press-simulator", [button]) => {
                let button = match *button {
                    "home" => SimulatorButton::Home,
                    "lock" => SimulatorButton::Lock,
                    other => {
                        return Err(ServerError::CommandParse(format!(
                            "{name} takes home or lock, not {other:?}"
                        ))
                        .into());
                    }
                };
                Box::new(move |request_id| SimulatorCommand::Press { request_id, button })
            }
            _ => {
                return Err(
                    ServerError::CommandParse(format!("usage: {name} {}", spec.usage)).into(),
                );
            }
        };
        let pane = {
            let inner = self.inner.lock();
            let pane =
                inner
                    .engine
                    .resolve_pane(target.as_deref(), context.window, context.pane)?;
            if !matches!(
                inner.engine.state.pane(pane).map(|pane| &pane.kind),
                Some(PaneKind::Simulator(_))
            ) {
                return Err(
                    ServerError::InvalidTarget(format!("{pane} is not a simulator pane")).into(),
                );
            }
            pane
        };
        self.request_from_gui(
            pane,
            |missing| {
                format!(
                    "{name}: {missing}, so {pane} is not showing. Open that session in the zz \
                     app, then retry."
                )
            },
            screenshot,
            |request_id| EventPayload::SimulatorCommand {
                pane,
                command: command(request_id),
            },
        )
    }

    pub(super) fn send_simulator_resync(&self, outbound: &OutboundMailbox) {
        let panes = simulator_panes(&self.inner.lock());
        for (pane, descriptor) in panes {
            Self::send_event(outbound, EventPayload::SimulatorPane { pane, descriptor });
        }
    }
}
