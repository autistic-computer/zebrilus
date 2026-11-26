use super::*;

pub(super) fn command(name: &str, args: &[&str]) -> CommandInvocation {
    CommandInvocation::new(name, args.iter().copied())
}

pub(super) fn window_layout(engine: &MuxEngine, session: SessionId) -> Vec<String> {
    engine.state.sessions[&session]
        .windows
        .iter()
        .map(|window| {
            let window = &engine.state.windows[window];
            format!("{}:{}", window.index, window.name)
        })
        .collect()
}
