use super::mux_effects::CommandAftermath;
use super::*;

pub(super) fn apply_flag_effect(
    inner: &mut ServerState,
    after: &mut CommandAftermath,
    pane_format_output: &mut Option<(PaneId, String, Option<SessionId>, FormatClient)>,
    effect: &MuxEffect,
) {
    match effect {
        MuxEffect::SourceFile {
            path,
            quiet,
            parse_only,
            verbose,
            context,
            stdin,
        } => after.source_files.push(SourceFileRequest {
            path: path.clone(),
            quiet: *quiet,
            parse_only: *parse_only,
            verbose: *verbose,
            context: context.clone(),
            stdin: stdin.clone(),
        }),
        MuxEffect::PaneStreamInput { pane, bytes } => {
            after.pane_stream_inputs.push((*pane, bytes.clone()));
        }
        MuxEffect::RunHook {
            name,
            commands,
            context,
        } => {
            inner
                .engine
                .note_event_hook_fired(context, name, unix_timestamp());
            after
                .immediate_hooks
                .push((name.clone(), commands.clone(), context.clone()));
        }
        MuxEffect::PaneFormatOutput {
            pane,
            format,
            active_session,
            format_client,
        } => {
            *pane_format_output = Some((*pane, format.clone(), *active_session, *format_client));
        }
        MuxEffect::ReloadConfig => {
            after.reload_config = true;
        }
        MuxEffect::ImportTmuxConfig(path) => {
            after.import_tmux_config = Some(path.clone());
        }
        MuxEffect::KillServer => after.force_shutdown_requested = true,
        MuxEffect::SnapshotChanged => after.snapshot_changed = true,
        MuxEffect::ModeStylesChanged => after.mode_styles_changed = true,
        MuxEffect::UserOptionChanged { channel } => {
            after.option_signals.push(channel.clone());
        }
        MuxEffect::PaneModeChanged { pane, mode } => {
            let changed = match mode {
                Some(mode) => push_pane_mode(inner, *pane, mode.clone()),
                None => clear_pane_modes(inner, *pane),
            };
            if changed {
                after.snapshot_changed = true;
            }
        }
        _ => unreachable!(),
    }
}
