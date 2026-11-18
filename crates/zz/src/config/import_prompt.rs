use std::path::{Path, PathBuf};

use ala::{App, Window};
use penna::WindowExt as _;
use zz_ui::dialogs::import_configuration_file_alert;

use crate::{config, user_data::platform_data_dir};

const MARKER_FILE_NAME: &str = "import-prompted-v2";

fn marker_path() -> Option<PathBuf> {
    platform_data_dir().map(|data| {
        data.join(zz_protocol::app_identity::DIRECTORY)
            .join(MARKER_FILE_NAME)
    })
}

pub(crate) fn mark_prompted() {
    let Some(path) = marker_path() else {
        return;
    };
    if let Err(error) = config::atomic_write(&path, b"") {
        log::warn!(
            target: "zz::config",
            "could not persist the import prompt marker path={} error={error}",
            path.display(),
        );
    }
}

pub(crate) fn choose_tmux_config(window: &mut Window, cx: &mut App) {
    let selected = cx.prompt_for_paths(ala::PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: Some("Choose configuration".into()),
    });
    window
        .spawn(cx, async move |window| {
            let path = selected.await.ok()?.ok()??.into_iter().next()?;
            window
                .update(|window, cx| {
                    let Some(target) = zz_daemon_client::mux_config_write_path() else {
                        crate::window::toast::push(
                            penna::blocks::notification::Notification::error(
                                "Could not resolve the zz multiplexer configuration path",
                            ),
                            cx,
                        );
                        return;
                    };
                    window.open_alert_dialog(cx, move |alert, _, _| {
                        let path = path.clone();
                        import_configuration_file_alert(
                            alert,
                            "Import from tmux?",
                            format!(
                                "Import {} into {}? You can choose other paths in Settings.",
                                path.display(),
                                target.display(),
                            ),
                        )
                        .on_ok(move |_, window, cx| {
                            run_tmux_import(path.clone(), window, cx);
                            true
                        })
                    });
                })
                .ok()
        })
        .detach();
}

pub(crate) fn run_tmux_import(path: PathBuf, window: &mut Window, cx: &mut App) {
    let task = cx
        .background_executor()
        .spawn(async move { config::import_tmux_config(&path) });
    window
        .spawn(cx, async move |window| {
            let result = task.await;
            let _ = window.update(|window, cx| match result {
                Ok(output) => crate::window::toast::push(
                    penna::blocks::notification::Notification::success(output),
                    cx,
                ),
                Err(error) => {
                    crate::window::toast::push(
                        penna::blocks::notification::Notification::info(error),
                        cx,
                    );
                    cx.set_global(config::settings::PendingMuxImport(true));
                    window.dispatch_action(Box::new(config::settings::OpenSettings), cx);
                }
            });
        })
        .detach();
}

pub(crate) fn pending() -> bool {
    !marker_path().as_deref().is_some_and(Path::exists)
}
