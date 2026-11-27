use super::*;

impl BrowserView {
    pub(super) fn prompt_cookie_import(window: &mut Window, cx: &mut Context<Self>) {
        let selected = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose a Cookie-Editor JSON or cookies.txt file".into()),
        });
        let background = cx.background_executor().clone();
        let view = cx.entity();
        cx.spawn_in(window, async move |_, window| {
            let path = selected.await.ok()?.ok()??.into_iter().next()?;
            let result = background
                .spawn(async move { read_cookie_import(&path) })
                .await;

            window
                .update(|_, cx| {
                    view.update(cx, |view, cx| match result {
                        Ok(batch) => {
                            let pane = view.pane;
                            let tab = view.active_tab;
                            view.controller.update(cx, |controller, cx| {
                                controller.import_cookies(pane, tab, batch, cx);
                            });
                        }
                        Err(error) => {
                            view.emit_notification(error, ClientMessageKind::Error, cx);
                        }
                    });
                })
                .ok()
        })
        .detach();
    }

    pub(super) fn import_chrome_data(
        &self,
        source_profile: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let background = cx.background_executor().clone();
        let view = cx.entity();
        let destination_profile = self.profile.clone();
        self.emit_notification("Reading cookies and history…", ClientMessageKind::Info, cx);
        cx.spawn_in(window, async move |_, window| {
            let (history_result, cookie_result) = background
                .spawn(async move {
                    let limits = chrome_import::history::ImportLimits {
                        max_count: chrome_import::history::MAX_HISTORY_IMPORT_COUNT,
                        max_url_bytes: recent_pages::MAX_URL_BYTES,
                        max_title_bytes: recent_pages::MAX_TITLE_BYTES,
                    };
                    (
                        chrome_import::history::import_history(&source_profile, limits),
                        chrome_import::cookie::import_all_cookies(&source_profile),
                    )
                })
                .await;
            window
                .update(|window, cx| {
                    #[cfg(not(target_os = "macos"))]
                    let _ = &window;
                    let permission_denied = cfg!(target_os = "macos")
                        && (history_result.as_ref().is_err_and(
                            chrome_import::history::ChromeHistoryImportError::is_permission_denied,
                        ) || cookie_result.as_ref().is_err_and(
                            chrome_import::cookie::ChromeCookieImportError::is_permission_denied,
                        ));
                    view.update(cx, |view, cx| {
                        match history_result {
                            Ok(imported) => {
                                let pages = imported
                                    .pages
                                    .into_iter()
                                    .map(|page| {
                                        RecentPage::imported(
                                            destination_profile.clone(),
                                            page.url,
                                            page.title,
                                            page.visited_at,
                                            page.visit_count,
                                            page.typed_count,
                                        )
                                    })
                                    .collect();
                                let changed = recent_pages::import_history(pages, cx);
                                let message = if changed == 0 {
                                    format!(
                                        "Chrome history is already up to date{}",
                                        ignored_suffix(imported.skipped)
                                    )
                                } else {
                                    format!(
                                        "Imported {changed} history entries{}",
                                        ignored_suffix(imported.skipped)
                                    )
                                };
                                view.emit_notification(message, ClientMessageKind::Success, cx);
                            }
                            Err(error) if permission_denied && error.is_permission_denied() => {}
                            Err(error) => view.emit_notification(
                                format!("Could not import Chrome history: {error}"),
                                ClientMessageKind::Error,
                                cx,
                            ),
                        }
                        match cookie_result {
                            Ok(batch) => {
                                let pane = view.pane;
                                let tab = view.active_tab;
                                view.controller.update(cx, |controller, cx| {
                                    controller.import_cookies(pane, tab, batch, cx);
                                });
                            }
                            Err(error) if permission_denied && error.is_permission_denied() => {}
                            Err(error) => view.emit_notification(
                                format!("Could not import Chrome cookies: {error}"),
                                ClientMessageKind::Error,
                                cx,
                            ),
                        }
                    });
                    #[cfg(target_os = "macos")]
                    if permission_denied {
                        prompt_for_chrome_data_access(window, cx);
                    }
                })
                .ok()
        })
        .detach();
    }

    pub(super) fn refresh_chrome_profiles(
        &mut self,
        notify_completion: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.chrome_profile_discovery == ChromeProfileDiscovery::Loading {
            return;
        }
        self.chrome_profile_discovery = ChromeProfileDiscovery::Loading;
        let background = cx.background_executor().clone();
        let view = cx.entity();
        cx.spawn_in(window, async move |_, window| {
            let result = background
                .spawn(async move {
                    let cache_dir = zz_protocol::app_identity::browser_data_dir();
                    match chrome_import::profiles::discover_profiles() {
                        Ok(profiles) => {
                            if let Err(error) = cache_dir.map_err(Into::into).and_then(|dir| {
                                chrome_import::profiles::save_cached_profiles(&dir, &profiles)
                            }) {
                                log::warn!("could not cache Chrome profile labels: {error}");
                            }
                            (profiles, None)
                        }
                        Err(error) => {
                            log::warn!("could not discover Chrome profiles: {error}");
                            let profiles = cache_dir
                                .map_err(Into::into)
                                .and_then(|dir| chrome_import::profiles::load_cached_profiles(&dir))
                                .unwrap_or_else(|cache_error| {
                                    log::warn!(
                                        "could not load cached Chrome profile labels: {cache_error}"
                                    );
                                    Vec::new()
                                });
                            (profiles, Some(error.to_string()))
                        }
                    }
                })
                .await;
            window
                .update(|_, cx| {
                    view.update(cx, |view, cx| {
                        let (profiles, error) = result;
                        view.chrome_profiles = profiles;
                        view.chrome_profile_discovery = if error.is_some() {
                            ChromeProfileDiscovery::Failed
                        } else {
                            ChromeProfileDiscovery::Loaded
                        };
                        if notify_completion {
                            if let Some(error) = error {
                                view.emit_notification(error, ClientMessageKind::Error, cx);
                            } else {
                                let message = match view.chrome_profiles.len() {
                                    0 => "No browser profiles found".to_owned(),
                                    1 => "Loaded 1 browser profile".to_owned(),
                                    count => format!("Loaded {count} browser profiles"),
                                };
                                view.emit_notification(message, ClientMessageKind::Success, cx);
                            }
                        }
                        cx.notify();
                    });
                })
                .ok()
        })
        .detach();
    }

    pub(super) fn confirm_clear_site_data(window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity();
        window.open_alert_dialog(cx, move |alert, _, cx| {
            let view = view.clone();
            browser_clear_site_data_alert(alert, cx).on_ok(move |_, _, cx| {
                view.update(cx, |view, cx| {
                    let pane = view.pane;
                    let tab = view.active_tab;
                    view.controller.update(cx, |controller, cx| {
                        controller.clear_site_data(pane, tab, cx);
                    });
                });
                true
            })
        });
    }
}
