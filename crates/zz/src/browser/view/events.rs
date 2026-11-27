use super::*;

impl BrowserView {
    pub(super) fn handle_controller_event(
        &mut self,
        controller: &Entity<BrowserController>,
        event: &ControllerEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ControllerEvent::RuntimeReady => {
                self.recoverable = true;
                self.error = None;
            }
            ControllerEvent::Failed(error) => {
                self.element_pick_active = false;
                self.pending_history_uses.clear();
                self.error = Some(error.clone());
                self.recoverable =
                    controller.read(cx).runtime_phase() == Some(RuntimePhase::Running);
            }
            ControllerEvent::Browser {
                pane,
                tab,
                event:
                    BrowserEvent::PopupCreated {
                        popup,
                        url,
                        foreground,
                        ..
                    },
            } if *pane == self.pane => {
                self.adopt_popup(*tab, *popup, url, *foreground, window, cx);
            }
            ControllerEvent::TabClosed { pane, tab } if *pane == self.pane => {
                if self
                    .tabs
                    .iter()
                    .any(|entry| entry.id == *tab && entry.popup)
                {
                    if self.tabs.len() == 1 {
                        self.open_tab(None, true, window, cx);
                    }
                    self.close_tab_by_id(*tab, window, cx);
                }
            }
            ControllerEvent::Browser { pane, tab, event }
                if *pane == self.pane && *tab != self.active_tab =>
            {
                if !self.handle_inactive_tab_event(*tab, event, window, cx) {
                    return;
                }
            }
            ControllerEvent::Browser { pane, event, .. } if *pane == self.pane => match event {
                BrowserEvent::Created { .. } => {
                    self.error = None;
                    self.recoverable = true;
                }
                BrowserEvent::AddressChanged { url, .. } => {
                    log::debug!("browser main-frame address changed to {}", display_url(url));
                    self.current_url = url.to_string();
                    self.status_message = None;
                    if !self.address_editing {
                        let display = if is_blank_url(url) {
                            String::new()
                        } else {
                            url.to_string()
                        };
                        self.address.update(cx, |address, cx| {
                            address.set_value(display, window, cx);
                        });
                    }
                    recent_pages::record_visit(&self.profile, url, cx);
                    self.publish_tabs(cx);
                }
                BrowserEvent::TitleChanged { title, .. } => {
                    self.title = title.to_string();
                    recent_pages::record_title(&self.profile, &self.current_url, title, cx);
                }
                BrowserEvent::FaviconChanged { url, png, .. } => {
                    recent_pages::record_favicon(&self.profile, url, png.clone(), cx);
                }
                BrowserEvent::LoadingChanged {
                    loading,
                    can_go_back,
                    can_go_forward,
                    ..
                } => {
                    self.loading = *loading;
                    self.publish_loading(cx);
                    self.can_go_back = *can_go_back;
                    self.can_go_forward = *can_go_forward;
                    self.address.update(cx, |address, cx| {
                        address.set_loading(*loading, window, cx);
                    });
                    if *loading {
                        self.mark_history_use_started(self.active_tab);
                        self.error = None;
                    } else {
                        self.loading_progress = None;
                        let url = self.current_url.clone();
                        self.complete_history_use(self.active_tab, &url, self.error.is_none(), cx);
                    }
                }
                BrowserEvent::LoadingProgressChanged { progress, .. } => {
                    let next = normalize_loading_progress(*progress);
                    if self.loading_progress == next {
                        return;
                    }
                    self.loading_progress = next;
                }
                BrowserEvent::StatusMessageChanged { message, .. } => {
                    let next =
                        (!message.is_empty()).then(|| SharedString::from(message.to_string()));
                    if self.status_message == next {
                        return;
                    }
                    self.status_message = next;
                }
                BrowserEvent::FindResult {
                    active_ordinal,
                    count,
                    ..
                } => {
                    if !self.find_open || self.find_input.read(cx).value().is_empty() {
                        return;
                    }
                    let next = Some((*active_ordinal, *count));
                    if self.find_matches == next {
                        return;
                    }
                    self.find_matches = next;
                }
                BrowserEvent::FileDialogRequested { request, .. } => {
                    self.handle_file_dialog(self.active_tab, request, window, cx);
                    return;
                }
                BrowserEvent::JsDialogRequested { request, .. } => {
                    self.handle_js_dialog(self.active_tab, request, window, cx);
                    return;
                }
                BrowserEvent::DownloadRequested { request, .. } => {
                    self.handle_download_request(self.active_tab, request, window, cx);
                    return;
                }
                BrowserEvent::DownloadUpdated { progress, .. } => {
                    self.handle_download_updated(progress, cx);
                    return;
                }
                BrowserEvent::FrameReady { .. } if self.visible => {
                    #[cfg(target_os = "macos")]
                    let presented = (self.frame_session, self.mac_surface.is_some());
                    if !self.consume_frame(controller, cx) {
                        return;
                    }
                    if matches!(self.resize_cover, ResizeCover::Awaiting(_))
                        && self.frame_size == (self.viewport.width, self.viewport.height)
                    {
                        self.resize_cover = ResizeCover::None;
                        cx.notify();
                        return;
                    }
                    #[cfg(target_os = "macos")]
                    if self.underlay.is_some()
                        && presented == (self.frame_session, true)
                        && self.mac_surface.is_some()
                        && !self
                            .content_bounds
                            .is_some_and(|bounds| window.underlay_covered(bounds))
                    {
                        return;
                    }
                }
                BrowserEvent::FullscreenModeChanged { .. }
                | BrowserEvent::SharedTextureFailed { .. }
                | BrowserEvent::FrameReady { .. } => {
                    return;
                }
                BrowserEvent::LoadFailed {
                    description, url, ..
                } => {
                    self.loading_progress = None;
                    self.status_message = None;
                    self.error = Some(Arc::from(format!("{}: {}", display_url(url), description)));
                    self.recoverable = true;
                }
                BrowserEvent::CursorChanged { cursor, .. } => {
                    self.cursor = cursor_style(*cursor);
                }
                BrowserEvent::ImeCompositionChanged { bounds, .. } => {
                    self.ime_bounds = Arc::clone(bounds);
                    window.invalidate_character_coordinates();
                    return;
                }
                BrowserEvent::RenderProcessTerminated { status, .. } => {
                    self.pending_history_uses.remove(&self.active_tab);
                    self.element_pick_active = false;
                    self.loading_progress = None;
                    self.status_message = None;
                    self.error = Some(Arc::from(format!("Renderer stopped: {status}")));
                    self.recoverable = true;
                }
                BrowserEvent::ElementPicked {
                    text, screenshot, ..
                } => {
                    self.element_pick_active = false;
                    let (item, message) = match screenshot {
                        Some(png) => (
                            ClipboardItem {
                                entries: vec![
                                    ClipboardEntry::String(ClipboardString::new(text.to_string())),
                                    ClipboardEntry::Image(Image::from_bytes(
                                        ImageFormat::Png,
                                        png.to_vec(),
                                    )),
                                ],
                            },
                            "Element context + screenshot copied",
                        ),
                        None => (
                            ClipboardItem::new_string(text.to_string()),
                            "Element context copied",
                        ),
                    };
                    cx.write_to_clipboard(item);
                    self.emit_notification(message, ClientMessageKind::Success, cx);
                }
                BrowserEvent::ElementPickFailed { .. } => {
                    self.element_pick_active = false;
                    self.emit_notification(
                        "Could not inspect that element",
                        ClientMessageKind::Error,
                        cx,
                    );
                }
                BrowserEvent::ContextMenuRequested { request, .. } => {
                    self.open_context_menu(request, window, cx);
                }
                BrowserEvent::PopupRequested {
                    url, foreground, ..
                } => {
                    self.open_tab(Some(url), *foreground, window, cx);
                }
                BrowserEvent::PopupCreated { .. } => return,
                BrowserEvent::Closed { .. } => {
                    self.pending_history_uses.remove(&self.active_tab);
                    self.element_pick_active = false;
                    self.loading_progress = None;
                    self.status_message = None;
                }
                BrowserEvent::ElementPickCancelled { .. } => {
                    self.element_pick_active = false;
                }
            },
            ControllerEvent::CookiesImported { pane, result } if *pane == self.pane => {
                self.handle_cookie_import_result(controller, *result, cx);
            }
            ControllerEvent::SiteDataCleared { pane, result } if *pane == self.pane => {
                self.handle_site_data_clear_result(controller, *result, cx);
            }
            ControllerEvent::PdfPrinted { pane, result } if *pane == self.pane => {
                self.handle_pdf_print_result(result, cx);
            }
            ControllerEvent::BrowserDataFailed { pane, message } if *pane == self.pane => {
                self.emit_notification(message.to_string(), ClientMessageKind::Error, cx);
            }
            ControllerEvent::BrowserFailed { pane, message } if *pane == self.pane => {
                self.pending_history_uses.clear();
                self.element_pick_active = false;
                self.error = Some(message.clone());
                self.recoverable = true;
            }
            ControllerEvent::Browser { .. }
            | ControllerEvent::TabClosed { .. }
            | ControllerEvent::CookiesImported { .. }
            | ControllerEvent::SiteDataCleared { .. }
            | ControllerEvent::PdfPrinted { .. }
            | ControllerEvent::BrowserDataFailed { .. }
            | ControllerEvent::BrowserFailed { .. } => return,
        }
        if self.visible {
            cx.notify();
        }
    }

    fn handle_inactive_tab_event(
        &mut self,
        tab: TabId,
        event: &BrowserEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let BrowserEvent::PopupRequested {
            url, foreground, ..
        } = event
        {
            self.open_tab(Some(url), *foreground, window, cx);
            return true;
        }
        if matches!(event, BrowserEvent::Closed { .. }) {
            self.pending_history_uses.remove(&tab);
            return true;
        }
        match event {
            BrowserEvent::FileDialogRequested { request, .. } => {
                self.handle_file_dialog(tab, request, window, cx);
                return false;
            }
            BrowserEvent::JsDialogRequested { request, .. } => {
                self.handle_js_dialog(tab, request, window, cx);
                return false;
            }
            BrowserEvent::DownloadRequested { request, .. } => {
                self.handle_download_request(tab, request, window, cx);
                return false;
            }
            BrowserEvent::DownloadUpdated { progress, .. } => {
                self.handle_download_updated(progress, cx);
                return false;
            }
            _ => {}
        }
        let Some(entry) = self.tabs.iter_mut().find(|entry| entry.id == tab) else {
            return false;
        };
        let mut completed_load = None;
        let mut started_load = false;
        match event {
            BrowserEvent::AddressChanged { url, .. } => {
                entry.url = url.to_string();
                recent_pages::record_visit(&self.profile, url, cx);
            }
            BrowserEvent::TitleChanged { title, .. } => {
                recent_pages::record_title(&self.profile, &entry.url, title, cx);
                entry.title = title.to_string();
            }
            BrowserEvent::FaviconChanged { url, png, .. } => {
                recent_pages::record_favicon(&self.profile, url, png.clone(), cx);
            }
            BrowserEvent::LoadingChanged {
                loading,
                can_go_back,
                can_go_forward,
                ..
            } => {
                entry.loading = *loading;
                entry.can_go_back = *can_go_back;
                entry.can_go_forward = *can_go_forward;
                if *loading {
                    started_load = true;
                    entry.error = None;
                } else {
                    entry.loading_progress = None;
                    completed_load = Some((entry.url.clone(), entry.error.is_none()));
                }
            }
            BrowserEvent::LoadingProgressChanged { progress, .. } => {
                entry.loading_progress = normalize_loading_progress(*progress);
                return false;
            }
            BrowserEvent::LoadFailed {
                description, url, ..
            } => {
                entry.loading_progress = None;
                entry.error = Some(Arc::from(format!(
                    "Could not load {}: {}",
                    display_url(url),
                    description
                )));
            }
            _ => return false,
        }
        if matches!(event, BrowserEvent::AddressChanged { .. }) {
            self.publish_tabs(cx);
        }
        if started_load {
            self.mark_history_use_started(tab);
        }
        if let Some((url, succeeded)) = completed_load {
            self.complete_history_use(tab, &url, succeeded, cx);
        }
        true
    }

    fn handle_cookie_import_result(
        &mut self,
        controller: &Entity<BrowserController>,
        result: ala_cef::CookieImportResult,
        cx: &mut Context<Self>,
    ) {
        let ignored = result.skipped + result.rejected;
        let app_bound = result.app_bound_skipped;
        if result.imported > 0 {
            self.pending_history_uses.remove(&self.active_tab);
            controller.update(cx, |controller, cx| {
                controller.reload(self.pane, self.active_tab, cx);
            });
        }
        let app_bound_note = if app_bound > 0 {
            format!("; {app_bound} Chrome v20 app-bound skipped — use Import cookies to copy them")
        } else {
            String::new()
        };
        let (message, kind) = if result.imported == 0 {
            if app_bound > 0 && ignored == 0 {
                (
                    format!(
                        "All {app_bound} cookies use Chrome v20 App-Bound Encryption — use Import cookies → Paste from clipboard or Import from file"
                    ),
                    ClientMessageKind::Error,
                )
            } else {
                (
                    format!("Chromium rejected all {ignored} cookies{app_bound_note}"),
                    ClientMessageKind::Error,
                )
            }
        } else if !result.persisted {
            (
                format!(
                    "Imported {} cookies, but could not flush the profile{}{app_bound_note}",
                    result.imported,
                    ignored_suffix(ignored)
                ),
                ClientMessageKind::Warning,
            )
        } else if app_bound > 0 {
            (
                format!(
                    "Imported {} cookies{}{app_bound_note}",
                    result.imported,
                    ignored_suffix(ignored)
                ),
                ClientMessageKind::Warning,
            )
        } else {
            (
                format!(
                    "Imported {} cookies{}",
                    result.imported,
                    ignored_suffix(ignored)
                ),
                ClientMessageKind::Success,
            )
        };
        self.emit_notification(message, kind, cx);
    }

    fn handle_pdf_print_result(&self, result: &PdfPrintResult, cx: &mut Context<Self>) {
        if result.success {
            let name = Path::new(result.path.as_ref())
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("page.pdf");
            self.emit_notification(
                format!("Saved PDF to {name}"),
                ClientMessageKind::Success,
                cx,
            );
        } else {
            self.emit_notification("Could not save page as PDF", ClientMessageKind::Error, cx);
        }
    }

    fn handle_site_data_clear_result(
        &mut self,
        controller: &Entity<BrowserController>,
        result: ala_cef::SiteDataClearResult,
        cx: &mut Context<Self>,
    ) {
        if result.success {
            self.pending_history_uses.remove(&self.active_tab);
            controller.update(cx, |controller, cx| {
                controller.reload(self.pane, self.active_tab, cx);
            });
            self.emit_notification("Site data cleared", ClientMessageKind::Success, cx);
        } else {
            self.emit_notification(
                "Chromium could not clear this site's data",
                ClientMessageKind::Error,
                cx,
            );
        }
    }
}
