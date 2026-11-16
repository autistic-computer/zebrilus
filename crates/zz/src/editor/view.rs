use std::path::PathBuf;

use ala::{
    App, Context, Div, Entity, FocusHandle, Focusable, IntoElement, KeyBinding, MouseButton,
    MouseDownEvent, ParentElement as _, Render, Stateful, Styled as _, Subscription, Window, div,
    prelude::*,
};
use penna::{
    ActiveTheme as _, Label, TextStyle,
    blocks::code_editor::{CodeEditor, CodeEditorEvent, CodeEditorState},
};
use zz_client::editor::{EditorDocument, EditorError, Publish, Restore, Retry, language_for_path};
use zz_protocol::{CommandInvocation, EditorDescriptor, PaneId};
use zz_ui::editor::{editor_error, vim_mode_chip};

use crate::{
    config::{self, pane_content_radii},
    file_picker::{FilePickerEvent, FilePickerMode, FilePickerView},
    mux::client::MuxClient,
    window::corners::{WindowCorners, round_div_radii},
};

const EDITOR_KEY_CONTEXT: &str = "Editor";
const MAX_EDITOR_FILE_BYTES: u64 = 8 * 1024 * 1024;

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

#[cfg(target_os = "macos")]
const OPEN_HINT: &str = "⌘O to open a file";
#[cfg(not(target_os = "macos"))]
const OPEN_HINT: &str = "Ctrl+O to open a file";

pub(crate) struct EditorView {
    pane: PaneId,
    mux: Entity<MuxClient>,
    editor: Entity<CodeEditorState>,
    document: EditorDocument,
    picker: Option<Entity<FilePickerView>>,
    window_corners: WindowCorners,
    _subscriptions: Vec<Subscription>,
}

impl EditorView {
    pub(crate) fn new(
        pane: PaneId,
        descriptor: &EditorDescriptor,
        title: &str,
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
            |view, editor, event: &CodeEditorEvent, _, cx| {
                if matches!(event, CodeEditorEvent::Change) {
                    let contents = editor.read(cx).value();
                    let dirty = view.document.is_dirty();
                    let title = view.document.buffer_changed(contents.as_ref());
                    view.publish(Publish { path: None, title }, cx);
                    if dirty != view.document.is_dirty() {
                        cx.notify();
                    }
                }
            },
        );
        let mut document = EditorDocument::new(descriptor, title);
        let restore = document.restore(descriptor);
        let mut view = Self {
            pane,
            mux,
            editor,
            document,
            picker: None,
            window_corners: WindowCorners::NONE,
            _subscriptions: vec![subscription],
        };
        view.apply_restore(restore, window, cx);
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
        title: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(restore) = self.document.synchronize(descriptor, title) {
            self.apply_restore(restore, window, cx);
        }
    }

    fn apply_restore(&mut self, restore: Restore, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            editor.set_value("", window, cx);
            editor.set_language(restore.language, window, cx);
        });
        self.publish(
            Publish {
                path: None,
                title: restore.title,
            },
            cx,
        );
        if let Some(path) = restore.load {
            self.load_file(path, window, cx);
        }
        cx.notify();
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
                .spawn(async move { zz_daemon::read_text_file(&read_path, MAX_EDITOR_FILE_BYTES) })
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

    fn open_file(&mut self, _: &OpenFile, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker.is_some() {
            return;
        }
        let picker = cx.new(|cx| {
            FilePickerView::new(
                FilePickerMode::Files,
                self.document.cwd().to_owned(),
                "Open a file in the editor",
                window,
                cx,
            )
        });
        cx.subscribe_in(
            &picker,
            window,
            |view, _, event: &FilePickerEvent, window, cx| {
                view.picker = None;
                if let FilePickerEvent::Selected(path) = event {
                    view.load_file(path.clone(), window, cx);
                }
                view.focus(cx).focus(window, cx);
                cx.notify();
            },
        )
        .detach();
        self.document.clear_error();
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
        match result {
            Ok(contents) => {
                let language = language_for_path(&path);
                if let Some(publish) = self.document.loaded(path, &contents) {
                    self.editor.update(cx, |editor, cx| {
                        editor.set_value(&contents, window, cx);
                        editor.set_language(language, window, cx);
                    });
                    self.publish(publish, cx);
                }
            }
            Err(message) => self.document.fail(Retry::Open(path), message),
        }
        cx.notify();
    }

    fn save_file(&mut self, _: &SaveFile, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = self.document.path() {
            let path = path.to_owned();
            self.begin_save(path, window, cx);
            return;
        }

        let selected = cx.prompt_for_new_path(self.document.cwd(), Some("untitled"));
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
        let ready = self.document.prepare_save(&path).is_some();
        cx.notify();
        if !ready {
            return;
        }
        let contents = self.editor.read(cx).value().to_string();
        let write_path = path.clone();
        let write_contents = contents.clone();
        let background = cx.background_executor().clone();
        let view = cx.entity();
        cx.spawn_in(window, async move |_, window| {
            let result = background
                .spawn(async move {
                    config::atomic_write(&write_path, write_contents.as_bytes()).map_err(|error| {
                        format!("Could not save {}: {error}", write_path.display())
                    })
                })
                .await;
            window
                .update(|_, cx| {
                    view.update(cx, |view, cx| {
                        view.finish_save(path, contents, result, cx);
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
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(()) => {
                let current = self.editor.read(cx).value();
                if let Some(publish) = self.document.saved(path, saved_contents, &current) {
                    self.publish(publish, cx);
                }
            }
            Err(message) => self.document.fail(Retry::Save(path), message),
        }
        cx.notify();
    }

    fn retry(&mut self, retry: Retry, window: &mut Window, cx: &mut Context<Self>) {
        match retry {
            Retry::Open(path) => self.load_file(path, window, cx),
            Retry::Save(path) => self.begin_save(path, window, cx),
        }
    }

    fn on_mouse_down(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus(cx).focus(window, cx);
        self.mux.read(cx).execute(CommandInvocation::new(
            "select-pane",
            ["-t", &self.pane.to_string()],
        ));
    }

    fn publish(&self, publish: Publish, cx: &App) {
        let pane = self.pane.to_string();
        if let Some(path) = publish.path {
            self.mux.read(cx).execute(CommandInvocation::new(
                "set-editor-path",
                vec!["-t".to_owned(), pane.clone(), path],
            ));
        }
        if let Some(title) = publish.title {
            self.mux.read(cx).execute(CommandInvocation::new(
                "select-pane",
                vec!["-t".to_owned(), pane, "-T".to_owned(), title],
            ));
        }
    }

    fn render_error(&self, error: EditorError, cx: &Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let title = error.title();
        let retry = error.retry;
        editor_error(
            self.pane,
            title,
            &error.message,
            move |window, cx| {
                view.update(cx, |view, cx| view.retry(retry.clone(), window, cx));
            },
            cx,
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
        let error = self.document.error().cloned();
        let scratch_hint = self.document.path().is_none()
            && self.editor.read(cx).value().is_empty()
            && error.is_none()
            && self.picker.is_none();
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
