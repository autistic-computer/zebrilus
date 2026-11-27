use super::*;

impl AppView {
    pub(super) fn open_command_palette(
        &mut self,
        mode: Option<PaletteMode>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_command_palette(
            move |backend, window, cx| CommandPaletteView::new_unified(backend, mode, window, cx),
            window,
            cx,
        );
    }

    fn show_command_palette(
        &mut self,
        build: impl FnOnce(
            Rc<dyn zz_ui::command::PaletteBackend>,
            &mut Window,
            &mut Context<CommandPaletteView>,
        ) -> CommandPaletteView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu.is_some() || self.confirm.is_some() {
            return;
        }
        let mux = self.mux.clone();
        mux.update(cx, |mux, _| mux.send_prefix_cancel());
        self.local_palette_prompt_revision = Some(mux.read(cx).command_prompt_revision());
        self.local_palette_chooser_revision = Some(mux.read(cx).choose_tree_revision());
        if mux.read(cx).command_prompt().is_some() {
            mux.read(cx).send_input(InputMessage::CommandPrompt {
                action: zz_protocol::CommandPromptAction::Close,
            });
        }
        if mux.read(cx).choose_tree().is_some() {
            mux.read(cx).send_input(InputMessage::ChooseTree {
                action: zz_protocol::ChooseTreeAction::Close,
            });
        }
        let offers = crate::command::gui::offers(&self.sidebar, cx);
        let backend = palette_backend_with(mux, offers);
        let palette = cx.new(|cx| build(backend, window, cx));
        self.observe_command_palette(&palette, window, cx);
        palette.read(cx).focus(cx).focus(window, cx);
        self.command_palette = Some(palette);
        self.focused_overlay = Some(OverlayKind::CommandPalette);
        self.synchronized_signature = None;
        cx.notify();
    }

    fn on_gui_event(&mut self, event: &GuiEvent, cx: &mut Context<Self>) {
        match event {
            GuiEvent::Preview { action, value } => {
                self.gui_previewing = true;
                crate::command::gui::preview(action, value, cx);
            }
            GuiEvent::Revert => {
                if std::mem::take(&mut self.gui_previewing) {
                    crate::command::gui::revert(cx);
                }
            }
            GuiEvent::Apply { action, value } => {
                self.gui_previewing = false;
                crate::command::gui::apply(action, value, &self.sidebar, cx);
            }
        }
    }

    pub(super) fn observe_command_palette(
        &self,
        palette: &Entity<CommandPaletteView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe_in(palette, window, |view, palette, event, window, cx| {
            if let CommandPaletteEvent::Gui(event) = event {
                view.on_gui_event(event, cx);
                return;
            }
            if view.command_palette.as_ref() != Some(palette) || palette.read(cx).is_finished() {
                return;
            }
            match event {
                CommandPaletteEvent::ReturnToDefault => {
                    view.pending_palette_chooser_close =
                        Some(view.mux.read(cx).choose_tree_closed_revision());
                    view.local_palette_prompt_revision =
                        Some(view.mux.read(cx).command_prompt_revision());
                    view.local_palette_chooser_revision = None;
                    palette.update(cx, |palette, cx| palette.return_to_default(window, cx));
                    view.synchronized_signature = None;
                    cx.notify();
                }
                CommandPaletteEvent::Gui(_) => {}
            }
        })
        .detach();
        cx.observe_release_in(palette, window, |view, _, _, cx| {
            if std::mem::take(&mut view.gui_previewing) {
                crate::command::gui::revert(cx);
            }
        })
        .detach();
        cx.observe_in(palette, window, move |view, palette, window, cx| {
            if view.command_palette.as_ref() != Some(&palette)
                || !palette.read(cx).is_local()
                || !palette.read(cx).is_finished()
            {
                return;
            }
            view.command_palette = None;
            view.local_palette_prompt_revision = None;
            view.local_palette_chooser_revision = None;
            view.focused_overlay = None;
            view.synchronized_signature = None;
            if view.sidebar.read(cx).route() == WorkspaceRoute::Settings {
                if let Some(settings) = view.sidebar.read(cx).settings_view() {
                    settings.read(cx).focus().focus(window, cx);
                }
            } else {
                view.focus_active_pane(window, cx);
            }
            cx.notify();
        })
        .detach();
    }

    pub(super) fn on_path_picker_update(
        &mut self,
        event: &PathPickerUpdate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let PathPickerUpdate::Open { pane, start_dir } = event {
            self.open_path_palette(*pane, start_dir.as_deref(), window, cx);
        }
    }

    fn open_path_palette(
        &mut self,
        pane: PaneId,
        start_dir: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.command_palette.is_some() || self.sidebar.read(cx).route() != WorkspaceRoute::App {
            return;
        }
        let (attached, snapshot, local) = {
            let mux = self.mux.read(cx);
            (
                mux.attached_session(),
                mux.snapshot(),
                mux.attached_host() == HostId::LOCAL,
            )
        };
        if !pane_in_session(&snapshot, attached, pane) {
            return;
        }
        let home = local.then(std::env::home_dir).flatten();
        let body = start_dir
            .filter(|dir| dir.starts_with(['/', '~']))
            .map(|dir| {
                zz_ui::files::start_dir(Some(dir), home.as_ref().and_then(|home| home.to_str()))
            })
            .unwrap_or_default();
        self.show_command_palette(
            move |backend, window, cx| {
                CommandPaletteView::new_target(backend, &body, Some(pane), window, cx)
            },
            window,
            cx,
        );
    }

    pub(super) fn open_folder_palette(
        &mut self,
        picker: &Entity<PanePickerView>,
        choose: &ChooseFolder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.command_palette.is_some() || self.sidebar.read(cx).route() != WorkspaceRoute::App {
            return;
        }
        let host = choose.host;
        let home = (host == HostId::LOCAL).then(std::env::home_dir).flatten();
        let body = zz_ui::files::start_dir(
            Some(&choose.start),
            home.as_ref().and_then(|home| home.to_str()),
        );
        let picker = picker.downgrade();
        let pick: zz_ui::command::FolderPick = Rc::new(move |picked, folder, window, cx| {
            if HostId::from(picked) != host {
                return;
            }
            let _ = picker.update(cx, |picker, cx| match folder {
                FolderPicked::Followed(folder) => picker.follow_folder(&folder, cx),
                FolderPicked::Chosen(folder) => {
                    picker.pick_folder(&folder, cx);
                    picker.focus().focus(window, cx);
                }
            });
        });
        self.show_command_palette(
            move |backend, window, cx| {
                CommandPaletteView::new_folder_pick_on(
                    backend,
                    host.into(),
                    &body,
                    pick,
                    window,
                    cx,
                )
            },
            window,
            cx,
        );
    }
}
