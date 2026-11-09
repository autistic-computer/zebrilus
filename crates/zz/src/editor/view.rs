use std::path::{Path, PathBuf};

use ala::{
    App, Context, Div, Entity, FocusHandle, Focusable, IntoElement, KeyBinding, MouseButton,
    MouseDownEvent, ParentElement as _, Render, Stateful, Styled as _, Subscription, Window, div,
    prelude::*, px,
};
use penna::{
    ActiveTheme as _, Chip, ChipLook, Emphasis, Label, Panel, PanelLook, Sizable as _, Size,
    TextStyle, Tone,
    blocks::code_editor::{CodeEditor, CodeEditorEvent, CodeEditorState, VimMode},
    blocks::feedback::{alert_actions, inline_alert},
    components::button::{Button, ButtonVariants as _},
};
use zz_client::editor_pane::{
    EditorEffect, EditorError, EditorModel, OPEN_HINT, PICKER_PLACEHOLDER, RETRY_LABEL, Retry,
    read_editor_file, save_error,
};
use zz_protocol::{CommandInvocation, EditorDescriptor, PaneId};

use crate::{
    config::{self, pane_content_radii},
    file_picker::{FilePickerEvent, FilePickerMode, FilePickerView},
    mux::client::MuxClient,
    window::corners::{WindowCorners, round_div_radii},
};

const EDITOR_KEY_CONTEXT: &str = "Editor";

ala::actions!(editor_pane, [OpenFile, SaveFile]);

pub fn init(cx: &mut App) {
    cx.bind_keys(editor_key_bindings());
}

#[cfg(target_os = "macos")]
fn editor_key_bindings() -> [KeyBinding; 2] {
    [
        KeyBinding::new("cmd-o", OpenFile, Some(EDITOR_KEY_CONTEXT)),
        KeyBinding::new("cmd-s", SaveFile, Some(EDITOR_KEY_CONTEXT)),
    ]
}

#[cfg(not(target_os = "macos"))]
fn editor_key_bindings() -> [KeyBinding; 2] {
    [
        KeyBinding::new("ctrl-o", OpenFile, Some(EDITOR_KEY_CONTEXT)),
        KeyBinding::new("ctrl-s", SaveFile, Some(EDITOR_KEY_CONTEXT)),
    ]
}

pub(crate) struct EditorView {
    pane: PaneId,
    mux: Entity<MuxClient>,
    editor: Entity<CodeEditorState>,
    model: EditorModel,
    picker: Option<Entity<FilePickerView>>,
    window_corners: WindowCorners,
    _subscriptions: Vec<Subscription>,
}

impl EditorView {
    pub(crate) fn new(
        pane: PaneId,
        descriptor: &EditorDescriptor,
        mux: Entity<MuxClient>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = cx.new(|cx| {
            CodeEditorState::new(window, cx)
                .language("text")
                .soft_wrap(true)
        });
        let subscription = cx.subscribe_in(
            &editor,
            window,
            |view, editor, event: &CodeEditorEvent, window, cx| {
                if matches!(event, CodeEditorEvent::Change) {
                    let contents = editor.read(cx).value();
                    view.on_buffer_changed(contents.as_ref(), window, cx);
                }
            },
        );
        let (model, effects) = EditorModel::new(descriptor);
        let mut view = Self {
            pane,
            mux,
            editor,
            model,
            picker: None,
            window_corners: WindowCorners::NONE,
            _subscriptions: vec![subscription],
        };
        view.apply(effects, window, cx);
        view
    }

    pub(crate) fn focus(&self, cx: &App) -> FocusHandle {
        self.editor.read(cx).focus_handle(cx)
    }

    pub(crate) fn set_window_corners(&mut self, corners: WindowCorners, cx: &mut Context<Self>) {
        if self.window_corners != corners {
            self.window_corners = corners;
            cx.notify();
        }
    }

    pub(crate) fn synchronize_descriptor(
        &mut self,
        descriptor: &EditorDescriptor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let effects = self.model.synchronize(descriptor);
        self.apply(effects, window, cx);
    }

    fn apply(&mut self, effects: Vec<EditorEffect>, window: &mut Window, cx: &mut Context<Self>) {
        for effect in effects {
            match effect {
                EditorEffect::SetBuffer { contents, language } => {
                    self.editor.update(cx, |editor, cx| {
                        editor.set_value(&contents, window, cx);
                        editor.set_language(language, window, cx);
                    });
                }
                EditorEffect::SetLanguage(language) => {
                    self.editor.update(cx, |editor, cx| {
                        editor.set_language(language, window, cx);
                    });
                }
                EditorEffect::Load(path) => self.load_file(path, window, cx),
                EditorEffect::Write { path, contents } => {
                    self.write_file(path, contents, window, cx);
                }
                EditorEffect::PromptNewPath {
                    directory,
                    suggested,
                } => self.prompt_new_path(&directory, suggested, window, cx),
                EditorEffect::PublishPath(path) => {
                    self.mux.read(cx).execute(CommandInvocation::new(
                        "set-editor-path",
                        vec!["-t".to_owned(), self.pane.to_string(), path],
                    ));
                }
                EditorEffect::PublishTitle(title) => {
                    self.mux.read(cx).execute(CommandInvocation::new(
                        "select-pane",
                        vec![
                            "-t".to_owned(),
                            self.pane.to_string(),
                            "-T".to_owned(),
                            title,
                        ],
                    ));
                }
            }
        }
    }

    #[allow(
        clippy::unused_self,
        reason = "kept as a method so open, restore, and retry share one call shape"
    )]
    fn load_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let read_path = path.clone();
        let background = cx.background_executor().clone();
        let view = cx.entity();
        cx.spawn_in(window, async move |_, window| {
            let result = background
                .spawn(async move { read_editor_file(&read_path) })
                .await;
            window
                .update(|window, cx| {
                    view.update(cx, |view, cx| {
                        view.finish_load(path, result, window, cx);
                    });
                })
                .ok()
        })
        .detach();
    }

    fn on_buffer_changed(&mut self, contents: &str, window: &mut Window, cx: &mut Context<Self>) {
        let dirty = self.model.dirty();
        let effects = self.model.buffer_changed(contents);
        if self.model.dirty() == dirty {
            return;
        }
        self.apply(effects, window, cx);
        cx.notify();
    }

    fn open_file(&mut self, _: &OpenFile, window: &mut Window, cx: &mut Context<Self>) {
        if !self.model.open_picker() {
            return;
        }
        let picker = cx.new(|cx| {
            FilePickerView::new(
                FilePickerMode::Files,
                self.model.cwd().to_path_buf(),
                PICKER_PLACEHOLDER,
                window,
                cx,
            )
        });
        cx.subscribe_in(
            &picker,
            window,
            |view, _, event: &FilePickerEvent, window, cx| {
                view.picker = None;
                let selected = match event {
                    FilePickerEvent::Selected(path) => Some(path.clone()),
                    FilePickerEvent::Dismissed => None,
                };
                let effects = view.model.picker_closed(selected);
                view.apply(effects, window, cx);
                view.focus(cx).focus(window, cx);
                cx.notify();
            },
        )
        .detach();
        self.picker = Some(picker);
        cx.notify();
    }

    fn finish_load(
        &mut self,
        path: PathBuf,
        result: Result<String, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let effects = self.model.finish_load(path, result);
        self.apply(effects, window, cx);
        cx.notify();
    }

    fn save_file(&mut self, _: &SaveFile, window: &mut Window, cx: &mut Context<Self>) {
        let contents = self.editor.read(cx).value();
        let effects = self.model.save(contents.as_ref());
        self.apply(effects, window, cx);
        cx.notify();
    }

    #[allow(
        clippy::unused_self,
        reason = "kept as a method so every file effect shares one call shape"
    )]
    fn prompt_new_path(
        &mut self,
        directory: &Path,
        suggested: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected = cx.prompt_for_new_path(directory, Some(suggested));
        let view = cx.entity();
        cx.spawn_in(window, async move |_, window| {
            let path = selected.await.ok()?.ok()??;
            window
                .update(|window, cx| {
                    view.update(cx, |view, cx| view.begin_save(path, window, cx));
                })
                .ok()
        })
        .detach();
    }

    fn begin_save(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let contents = self.editor.read(cx).value();
        let effects = self.model.begin_save(path, contents.as_ref());
        self.apply(effects, window, cx);
        cx.notify();
    }

    #[allow(
        clippy::unused_self,
        reason = "kept as a method so every file effect shares one call shape"
    )]
    fn write_file(
        &mut self,
        path: PathBuf,
        contents: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let write_path = path.clone();
        let write_contents = contents.clone();
        let background = cx.background_executor().clone();
        let view = cx.entity();
        cx.spawn_in(window, async move |_, window| {
            let result = background
                .spawn(async move {
                    config::atomic_write(&write_path, write_contents.as_bytes())
                        .map_err(|error| save_error(&write_path, &error))
                })
                .await;
            window
                .update(|window, cx| {
                    view.update(cx, |view, cx| {
                        view.finish_save(path, contents, result, window, cx);
                    });
                })
                .ok()
        })
        .detach();
    }

    fn finish_save(
        &mut self,
        path: PathBuf,
        saved_contents: String,
        result: Result<(), String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let contents = self.editor.read(cx).value();
        let effects = self
            .model
            .finish_save(path, saved_contents, result, contents.as_ref());
        self.apply(effects, window, cx);
        cx.notify();
    }

    fn retry(&mut self, retry: Retry, window: &mut Window, cx: &mut Context<Self>) {
        let contents = self.editor.read(cx).value();
        let effects = self.model.retry(retry, contents.as_ref());
        self.apply(effects, window, cx);
        cx.notify();
    }

    fn on_mouse_down(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus(cx).focus(window, cx);
        self.mux.read(cx).execute(CommandInvocation::new(
            "select-pane",
            ["-t", &self.pane.to_string()],
        ));
    }

    fn render_error(&self, error: EditorError, cx: &Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let title = error.retry.title();
        let retry = error.retry;
        Panel::new(("editor-error", self.pane.0), PanelLook::Plain)
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .px(px(12.0))
            .child(
                inline_alert(Some(title.into()), error.message.to_string(), cx)
                    .max_w(px(360.0))
                    .child(
                        alert_actions(cx).child(
                            Button::new(format!("editor-retry-{}", self.pane.0))
                                .primary()
                                .on(penna::blocks::feedback::alert_ground(cx))
                                .label(RETRY_LABEL)
                                .on_click(move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.retry(retry.clone(), window, cx);
                                    });
                                    cx.stop_propagation();
                                }),
                        ),
                    ),
            )
    }
}

impl Focusable for EditorView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.focus(cx)
    }
}

impl Render for EditorView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus(cx);
        let scratch_hint = self
            .model
            .scratch_hint(self.editor.read(cx).value().is_empty());
        let error = self.model.error().cloned();
        let line_numbers = config::editor_line_numbers(cx);
        let relative_line_numbers = config::editor_relative_line_numbers(cx);
        let soft_wrap = config::editor_soft_wrap(cx);
        let vim = config::editor_vim_mode(cx);
        let radii = pane_content_radii(cx, self.window_corners);
        self.editor.update(cx, |editor, cx| {
            editor.set_line_numbers(line_numbers, cx);
            editor.set_relative_line_numbers(relative_line_numbers, cx);
            editor.set_soft_wrap(soft_wrap, cx);
            editor.set_vim_enabled(vim, cx);
            editor.set_corner_radii(radii, cx);
        });
        let vim_mode = self.editor.read(cx).vim_mode();
        let root = editor_text(div().id(("editor-pane", self.pane.0)), cx)
            .key_context(EDITOR_KEY_CONTEXT)
            .track_focus(&focus)
            .relative()
            .flex()
            .size_full()
            .overflow_hidden()
            .on_action(cx.listener(Self::open_file))
            .on_action(cx.listener(Self::save_file))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .child(CodeEditor::new(&self.editor).background_opacity(0.0))
            .when(scratch_hint, |this| {
                this.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Label::new(TextStyle::Lead, OPEN_HINT).muted()),
                )
            })
            .when_some(vim_mode, |this, mode| {
                this.child(vim_mode_chip(self.pane, mode))
            })
            .when_some(error, |this, error| {
                this.child(self.render_error(error, cx))
            })
            .when_some(self.picker.clone(), ala::ParentElement::child);
        round_div_radii(root, radii)
    }
}

#[allow(
    clippy::disallowed_methods,
    reason = "file content takes the code font at the editor font size the user set"
)]
fn editor_text(root: Stateful<Div>, cx: &App) -> Stateful<Div> {
    root.font_family(cx.theme().mono_font_family.clone())
        .text_size(config::editor_font_size(cx))
}

fn vim_mode_chip(pane: PaneId, mode: VimMode) -> Chip {
    let chip = Chip::new(("editor-vim-mode", pane.0), mode.label())
        .look(ChipLook::Floating)
        .with_size(Size::XSmall);
    let chip = if matches!(mode, VimMode::Normal) {
        chip
    } else {
        chip.tone(Tone::Primary).emphasis(Emphasis::Solid)
    };
    chip.absolute()
        .right(px(8.0))
        .bottom(px(8.0))
        .h(px(18.0))
        .px(px(6.0))
}
