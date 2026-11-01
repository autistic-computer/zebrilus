use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum FlipWhenBusy {
    #[default]
    Refuse,
    Now,
    WhenIdle,
    Cancel,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct FlipAgentArgs {
    pub(super) target: Option<String>,
    pub(super) when_busy: FlipWhenBusy,
}

pub(super) fn parse_flip_agent_args(args: &[RawText]) -> Result<FlipAgentArgs, ServerError> {
    let spec = zz_protocol::catalog_command_spec("flip-agent").expect("registered command");
    let parsed = zz_protocol::parse_tmux_options(spec, args)?;
    if !parsed.positionals.is_empty() {
        return Err(ServerError::CommandParse(
            "flip-agent takes no positional arguments".to_owned(),
        ));
    }
    let mut flip = FlipAgentArgs::default();
    for option in parsed.options {
        let when_busy = match option {
            zz_protocol::TmuxOption::Value("-t", target) => {
                flip.target = Some(target.to_owned());
                continue;
            }
            zz_protocol::TmuxOption::Flag("--now") => FlipWhenBusy::Now,
            zz_protocol::TmuxOption::Flag("--when-idle") => FlipWhenBusy::WhenIdle,
            zz_protocol::TmuxOption::Flag("--cancel") => FlipWhenBusy::Cancel,
            zz_protocol::TmuxOption::Flag(name) | zz_protocol::TmuxOption::Value(name, _) => {
                return Err(ServerError::CommandParse(format!(
                    "flip-agent: unknown option {name}"
                )));
            }
        };
        if flip.when_busy != FlipWhenBusy::Refuse && flip.when_busy != when_busy {
            return Err(ServerError::CommandParse(
                "flip-agent takes at most one of --now, --when-idle, --cancel".to_owned(),
            ));
        }
        flip.when_busy = when_busy;
    }
    Ok(flip)
}

impl Shared {
    pub(super) fn flip_agent(
        self: &Arc<Self>,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let flip = parse_flip_agent_args(args)?;
        let pane = self.inner.lock().engine.resolve_pane(
            flip.target.as_deref(),
            context.window,
            context.pane,
        )?;
        Err(ServerError::InvalidCommand(format!("flip-agent: pane {pane} cannot flip yet")).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<FlipAgentArgs, ServerError> {
        let args = args
            .iter()
            .map(|arg| RawText::from(*arg))
            .collect::<Vec<_>>();
        parse_flip_agent_args(&args)
    }

    #[test]
    fn flip_agent_parses_target_and_one_busy_mode() {
        assert_eq!(parse(&[]).unwrap(), FlipAgentArgs::default());
        assert_eq!(
            parse(&["-t", "%3", "--when-idle"]).unwrap(),
            FlipAgentArgs {
                target: Some("%3".to_owned()),
                when_busy: FlipWhenBusy::WhenIdle,
            }
        );
        assert_eq!(parse(&["--now"]).unwrap().when_busy, FlipWhenBusy::Now);
        assert_eq!(
            parse(&["--cancel"]).unwrap().when_busy,
            FlipWhenBusy::Cancel
        );
        assert_eq!(
            parse(&["--now", "--now"]).unwrap().when_busy,
            FlipWhenBusy::Now
        );
        for args in [
            &["--now", "--cancel"][..],
            &["--when-idle", "--now"],
            &["claude"],
            &["--bogus"],
            &["-t"],
        ] {
            assert!(parse(args).is_err(), "{args:?}");
        }
    }
}
