use super::*;

pub(super) fn engine_with_session() -> (MuxEngine, ExecutionContext, PaneId) {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "customize"]),
        )
        .unwrap();
    let pane = context.pane.unwrap();
    (engine, context, pane)
}

pub(super) fn press(
    engine: &mut MuxEngine,
    context: &mut ExecutionContext,
    pane: PaneId,
    mode: &mut CustomizeMode,
    key: &str,
) -> bool {
    let mut expand = customize_expand;
    let result = engine.customize_key(pane, mode, key, PromptHistories::default(), &mut expand);
    for command in &result.commands {
        engine.execute(context, command).unwrap();
    }
    engine.customize_finish(pane, mode, &mut expand);
    result.close
}

pub(super) fn current_row(engine: &MuxEngine, pane: PaneId, mode: &CustomizeMode) -> Row {
    let mut expand = customize_expand;
    let rows = engine.customize_rows(pane, mode, &mut expand);
    let lines = customize_lines(&rows, mode);
    rows[lines[mode.current]].clone()
}

pub(super) fn select(engine: &MuxEngine, pane: PaneId, mode: &mut CustomizeMode, name: &str) {
    let mut expand = customize_expand;
    let rows = engine.customize_rows(pane, mode, &mut expand);
    mode.current = customize_lines(&rows, mode)
        .iter()
        .position(|line| rows[*line].name == name)
        .unwrap();
}

pub(super) fn type_text(
    engine: &mut MuxEngine,
    context: &mut ExecutionContext,
    pane: PaneId,
    mode: &mut CustomizeMode,
    text: &str,
) {
    for character in text.chars() {
        press(engine, context, pane, mode, &character.to_string());
    }
}

pub(super) fn preview_text(engine: &MuxEngine, pane: PaneId, mode: &CustomizeMode) -> String {
    let mut expand = customize_expand;
    let (_, presentation, _) = engine.customize_presentation(pane, mode, &mut expand);
    match presentation.preview {
        Some(ChooserPreview::Markup { lines }) => lines.join("\n"),
        _ => String::new(),
    }
}

pub(super) fn run(
    engine: &mut MuxEngine,
    context: &mut ExecutionContext,
    name: &str,
    args: &[&str],
) {
    engine
        .execute(context, &CommandInvocation::new(name, args.iter().copied()))
        .unwrap();
}

pub(super) fn output(
    engine: &mut MuxEngine,
    context: &mut ExecutionContext,
    name: &str,
    args: &[&str],
) -> String {
    engine
        .execute(context, &CommandInvocation::new(name, args.iter().copied()))
        .unwrap()
        .output
        .to_string()
}
