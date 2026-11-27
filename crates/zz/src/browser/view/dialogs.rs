use super::*;

impl BrowserView {
    pub(super) fn paste_cookies(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            self.emit_notification(
                "Clipboard does not contain text to import as cookies",
                ClientMessageKind::Error,
                cx,
            );
            return;
        };
        if text.len() > MAX_COOKIE_IMPORT_BYTES {
            self.emit_notification(
                "Clipboard cookie text is too large to import",
                ClientMessageKind::Error,
                cx,
            );
            return;
        }
        match parse_cookie_import(&text) {
            Ok(batch) => {
                let pane = self.pane;
                let tab = self.active_tab;
                self.controller.update(cx, |controller, cx| {
                    controller.import_cookies(pane, tab, batch, cx);
                });
            }
            Err(error) => {
                self.emit_notification(
                    format!("Could not import cookies: {error}"),
                    ClientMessageKind::Error,
                    cx,
                );
            }
        }
    }

    pub(super) fn prompt_save_pdf(&self, window: &mut Window, cx: &mut Context<Self>) {
        let suggested = suggested_pdf_name(&self.title, &self.current_url);
        let selected = cx.prompt_for_new_path(&default_downloads_dir(), Some(&suggested));
        let view = cx.entity();
        cx.spawn_in(window, async move |_, window| {
            let path = selected.await.ok()?.ok()??;
            window
                .update(|_, cx| {
                    view.update(cx, |view, cx| {
                        let pane = view.pane;
                        let tab = view.active_tab;
                        view.controller.update(cx, |controller, cx| {
                            controller.print_to_pdf(pane, tab, &path, cx);
                        });
                    });
                })
                .ok()
        })
        .detach();
    }

    pub(super) fn handle_file_dialog(
        &self,
        tab: TabId,
        request: &FileDialogRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let controller = self.controller.downgrade();
        let pane = self.pane;
        let id = request.id;
        match request.kind {
            FileDialogKind::Save => {
                let (dir, suggested) =
                    save_dialog_defaults(request.default_file_path.as_deref().map(Path::new));
                let selected = cx.prompt_for_new_path(&dir, suggested.as_deref());
                cx.spawn_in(window, async move |_, window| {
                    let path = selected.await.ok().and_then(Result::ok).flatten();
                    let _ = window.update(|_, cx| {
                        if let Some(controller) = controller.upgrade() {
                            controller.update(cx, |controller, cx| {
                                let paths = path.map(|path| vec![path]);
                                controller.respond_file_dialog(pane, tab, id, paths.as_deref(), cx);
                            });
                        }
                    });
                })
                .detach();
            }
            FileDialogKind::Open | FileDialogKind::OpenMultiple | FileDialogKind::OpenFolder => {
                let kind = request.kind;
                let prompt = request
                    .title
                    .as_deref()
                    .filter(|title| !title.is_empty())
                    .map(|title| title.to_owned().into());
                let selected = cx.prompt_for_paths(PathPromptOptions {
                    files: kind != FileDialogKind::OpenFolder,
                    directories: kind == FileDialogKind::OpenFolder,
                    multiple: kind == FileDialogKind::OpenMultiple,
                    prompt,
                });
                cx.spawn_in(window, async move |_, window| {
                    let paths = selected
                        .await
                        .ok()
                        .and_then(Result::ok)
                        .flatten()
                        .filter(|paths| !paths.is_empty());
                    let _ = window.update(|_, cx| {
                        if let Some(controller) = controller.upgrade() {
                            controller.update(cx, |controller, cx| {
                                controller.respond_file_dialog(pane, tab, id, paths.as_deref(), cx);
                            });
                        }
                    });
                })
                .detach();
            }
        }
    }

    pub(super) fn handle_js_dialog(
        &self,
        tab: TabId,
        request: &JsDialogRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let reply = JsDialogReply {
            answered: Rc::new(Cell::new(false)),
            controller: self.controller.downgrade(),
            pane: self.pane,
            tab,
            id: request.id,
        };
        let title = js_dialog_title(request.kind, &request.origin_url);
        let message = if matches!(request.kind, JsDialogKind::BeforeUnload { .. })
            && request.message.is_empty()
        {
            "Changes you made may not be saved.".to_owned()
        } else {
            request.message.to_string()
        };
        match request.kind {
            JsDialogKind::Alert => {
                window.open_alert_dialog(cx, move |alert, _, _| {
                    let ok = reply.clone();
                    let close = reply.clone();
                    browser_js_alert_dialog(alert, title.clone(), message.clone())
                        .on_ok(move |_, _, cx| {
                            ok.respond(true, None, cx);
                            true
                        })
                        .on_close(move |_, _, cx| {
                            close.respond(true, None, cx);
                        })
                });
            }
            JsDialogKind::Confirm | JsDialogKind::BeforeUnload { .. } => {
                let ok_text = match request.kind {
                    JsDialogKind::BeforeUnload { is_reload: true } => "Reload",
                    JsDialogKind::BeforeUnload { is_reload: false } => "Leave",
                    _ => "OK",
                };
                window.open_alert_dialog(cx, move |alert, _, _| {
                    let ok = reply.clone();
                    let cancel = reply.clone();
                    let close = reply.clone();
                    browser_js_confirm_dialog(
                        alert,
                        title.clone(),
                        message.clone(),
                        ok_text,
                        "Cancel",
                    )
                    .on_ok(move |_, _, cx| {
                        ok.respond(true, None, cx);
                        true
                    })
                    .on_cancel(move |_, _, cx| {
                        cancel.respond(false, None, cx);
                        true
                    })
                    .on_close(move |_, _, cx| {
                        close.respond(false, None, cx);
                    })
                });
            }
            JsDialogKind::Prompt => {
                let default_prompt = request
                    .default_prompt
                    .as_deref()
                    .unwrap_or_default()
                    .to_owned();
                let input = cx.new(|cx| InputState::new(window, cx).default_value(default_prompt));
                let dialog_input = input.clone();
                window.open_dialog(cx, move |dialog, _, cx| {
                    let confirm_input = dialog_input.clone();
                    let ok = reply.clone();
                    let cancel = reply.clone();
                    let close = reply.clone();
                    browser_js_prompt_dialog(dialog, title.clone(), &message, &dialog_input, cx)
                        .on_ok(move |_, _, cx| {
                            ok.respond(true, Some(confirm_input.read(cx).value().as_ref()), cx);
                            true
                        })
                        .on_cancel(move |_, _, cx| {
                            cancel.respond(false, None, cx);
                            true
                        })
                        .on_close(move |_, _, cx| {
                            close.respond(false, None, cx);
                        })
                });
                input.read(cx).focus_handle(cx).focus(window, cx);
            }
        }
    }

    pub(super) fn handle_download_request(
        &self,
        tab: TabId,
        request: &DownloadRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let suggested = if request.suggested_name.is_empty() {
            "download".to_owned()
        } else {
            request.suggested_name.to_string()
        };
        let id = request.id;
        let selected = cx.prompt_for_new_path(&default_downloads_dir(), Some(&suggested));
        let view = cx.entity();
        let controller = self.controller.downgrade();
        let pane = self.pane;
        cx.spawn_in(window, async move |_, window| {
            let path = selected.await.ok().and_then(Result::ok).flatten();
            let _ = window.update(|_, cx| match path {
                Some(path) => {
                    let name = path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .map_or_else(|| suggested.clone(), str::to_owned);
                    view.update(cx, |view, cx| {
                        view.active_downloads.insert(id, name.clone());
                        view.emit_notification(
                            format!("Downloading {name}…"),
                            ClientMessageKind::Info,
                            cx,
                        );
                        view.controller.update(cx, |controller, cx| {
                            controller.respond_download(pane, tab, id, Some(&path), cx);
                        });
                    });
                }
                None => {
                    if let Some(controller) = controller.upgrade() {
                        controller.update(cx, |controller, cx| {
                            controller.respond_download(pane, tab, id, None, cx);
                        });
                    }
                }
            });
        })
        .detach();
    }

    pub(super) fn handle_download_updated(
        &mut self,
        progress: &DownloadProgress,
        cx: &mut Context<Self>,
    ) {
        if progress.complete {
            let name = self
                .active_downloads
                .remove(&progress.id)
                .unwrap_or_else(|| {
                    if progress.file_name.is_empty() {
                        Path::new(progress.full_path.as_ref())
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("file")
                            .to_owned()
                    } else {
                        progress.file_name.to_string()
                    }
                });
            self.emit_notification(format!("Downloaded {name}"), ClientMessageKind::Success, cx);
        } else if progress.interrupted {
            if let Some(name) = self.active_downloads.remove(&progress.id) {
                self.emit_notification(
                    format!("Download interrupted: {name}"),
                    ClientMessageKind::Error,
                    cx,
                );
            }
        } else if progress.canceled
            && let Some(name) = self.active_downloads.remove(&progress.id)
        {
            self.emit_notification(
                format!("Download canceled: {name}"),
                ClientMessageKind::Info,
                cx,
            );
        }
    }

    pub(super) fn emit_notification(
        &self,
        message: impl Into<String>,
        kind: ClientMessageKind,
        cx: &mut Context<Self>,
    ) {
        self.mux.update(cx, |_, cx| {
            MuxClient::emit_notification(kind, message, cx);
        });
    }
}
