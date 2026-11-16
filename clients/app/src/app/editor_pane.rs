use std::{
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use ala::{
    App, Context, Corners, Entity, FocusHandle, Focusable as _, IntoElement, Pixels, Render,
    Subscription, Window, div, prelude::*, px,
};
use penna::{
    ActiveTheme as _,
    blocks::code_editor::{CodeEditor, CodeEditorEvent, CodeEditorState},
    components::button::{Button, ButtonVariants as _},
};
use zz_client::{
    CoreEvent,
    editor::{
        SyncAction, descriptor_sync_action, editor_title, language_for_path,
        validated_descriptor_path,
    },
};
use zz_protocol::{EditorDescriptor, PaneId, ProtocolMessage};
use zz_ui::{
    editor::{editor_error, vim_mode_chip},
    pane::PaneKindButton,
};

use crate::connection::Connection;

pub(crate) const EDITOR_OPEN: &str = "editor-open";
pub(crate) const EDITOR_SAVE: &str = "editor-save";
const EDITOR_KEY_CONTEXT: &str = "Editor";
const EDITOR_FONT_SIZE: f32 = 13.0;

ala::actions!(thin_editor, [OpenFile, SaveFile]);

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        ala::KeyBinding::new("cmd-o", OpenFile, Some(EDITOR_KEY_CONTEXT)),
        ala::KeyBinding::new("cmd-s", SaveFile, Some(EDITOR_KEY_CONTEXT)),
    ]);
    #[cfg(not(target_os = "ios"))]
    cx.bind_keys([
        ala::KeyBinding::new("ctrl-o", OpenFile, Some(EDITOR_KEY_CONTEXT)),
        ala::KeyBinding::new("ctrl-s", SaveFile, Some(EDITOR_KEY_CONTEXT)),
    ]);
}

pub(crate) type OpenHandler = Rc<dyn Fn(PaneId, String, &mut Window, &mut App)>;

#[derive(Clone)]
enum Retry {
    Open(PathBuf),
    Save(PathBuf),
}

#[derive(Clone)]
struct EditorError {
    message: Arc<str>,
    retry: Retry,
}

enum Request {
    Read {
        id: u64,
        path: PathBuf,
    },
    Write {
        id: u64,
        path: PathBuf,
        text: String,
    },
}

pub(crate) struct EditorPane {
    pane: PaneId,
    connection: Entity<Connection>,
    editor: Entity<CodeEditorState>,
    cwd: PathBuf,
    path: Option<PathBuf>,
    pending_path: Option<String>,
    snapshot_path: Option<String>,
    saved_contents: String,
    dirty: bool,
    last_title: String,
    error: Option<EditorError>,
    request: Option<Request>,
    on_open: Option<OpenHandler>,
    _subscriptions: Vec<Subscription>,
}

impl EditorPane {
    pub(crate) fn new(
        pane: PaneId,
        descriptor: &EditorDescriptor,
        connection: Entity<Connection>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = cx.new(|cx| {
            let mut editor = CodeEditorState::new(window, cx)
                .language("text")
                .soft_wrap(true);
            editor.set_line_numbers(true, cx);
            editor.set_relative_line_numbers(false, cx);
            editor
        });
        let changes = cx.subscribe(&editor, |pane, editor, event: &CodeEditorEvent, cx| {
            if matches!(event, CodeEditorEvent::Change) {
                let contents = editor.read(cx).value();
                pane.on_buffer_changed(contents.as_ref(), cx);
            }
        });
        let answers = cx.subscribe_in(
            &connection,
            window,
            |pane, _, event: &CoreEvent, window, cx| {
                if let CoreEvent::Message(message) = event {
                    pane.on_message(message, window, cx);
                }
            },
        );
        let mut pane = Self {
            pane,
            connection,
            editor,
            cwd: PathBuf::from(&descriptor.cwd),
            path: None,
            pending_path: None,
            snapshot_path: descriptor.path.clone(),
            saved_contents: String::new(),
            dirty: false,
            last_title: String::new(),
            error: None,
            request: None,
            on_open: None,
            _subscriptions: vec![changes, answers],
        };
        pane.restore_descriptor(descriptor, window, cx);
        pane
    }

    pub(crate) fn set_open_handler(&mut self, open: OpenHandler) {
        self.on_open = Some(open);
    }

    pub(crate) fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.read(cx).focus_handle(cx)
    }

    pub(crate) fn set_corner_radii(&self, radii: Corners<Pixels>, cx: &mut App) {
        self.editor
            .update(cx, |editor, cx| editor.set_corner_radii(radii, cx));
    }

    pub(crate) fn header_buttons(&self) -> Vec<PaneKindButton> {
        let mut buttons = vec![PaneKindButton {
            key: EDITOR_OPEN,
            icon: penna::IconName::Folder,
            label: "Open a file",
        }];
        if self.dirty && self.path.is_some() {
            buttons.push(PaneKindButton {
                key: EDITOR_SAVE,
                icon: penna::IconName::Check,
                label: "Save",
            });
        }
        buttons
    }

    pub(crate) fn header_action(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        match key {
            EDITOR_OPEN => self.open_file(&OpenFile, window, cx),
            EDITOR_SAVE => self.save_file(&SaveFile, window, cx),
            _ => {}
        }
    }

    pub(crate) fn synchronize(
        &mut self,
        descriptor: &EditorDescriptor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cwd = PathBuf::from(&descriptor.cwd);
        self.snapshot_path.clone_from(&descriptor.path);
        match descriptor_sync_action(
            self.pending_path.as_deref(),
            descriptor.path.as_deref(),
            self.path.as_deref(),
        ) {
            SyncAction::Ignore => {}
            SyncAction::ClearPending => self.pending_path = None,
            SyncAction::Restore => self.restore_descriptor(descriptor, window, cx),
        }
    }

    fn restore_descriptor(
        &mut self,
        descriptor: &EditorDescriptor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cwd = PathBuf::from(&descriptor.cwd);
        let Some(path) = descriptor.path.as_deref().map(PathBuf::from) else {
            self.path = None;
            self.saved_contents.clear();
            self.dirty = false;
            self.error = None;
            self.editor.update(cx, |editor, cx| {
                editor.set_value("", window, cx);
                editor.set_language("text", window, cx);
            });
            self.publish_title(cx);
            return;
        };
        self.path = Some(path.clone());
        self.editor.update(cx, |editor, cx| {
            editor.set_language(language_for_path(&path), window, cx);
        });
        self.publish_title(cx);
        self.load_file(path, cx);
    }

    fn load_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let Some(text) = path.to_str().map(str::to_owned) else {
            return;
        };
        let pane = self.pane;
        let id = self.connection.update(cx, |connection, cx| {
            connection
                .edits_files()
                .then(|| connection.read_file(pane, text, cx))
        });
        match id {
            Some(id) => self.request = Some(Request::Read { id, path }),
            None => self.fail(
                Retry::Open(path),
                "This host's zz cannot open files here",
                cx,
            ),
        }
    }

    fn fail(&mut self, retry: Retry, message: impl Into<Arc<str>>, cx: &mut Context<Self>) {
        self.error = Some(EditorError {
            message: message.into(),
            retry,
        });
        cx.notify();
    }

    fn on_message(
        &mut self,
        message: &ProtocolMessage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match (message, self.request.take()) {
            (
                ProtocolMessage::FileReadResponse { request_id, result },
                Some(Request::Read { id, path }),
            ) if *request_id == id => match result {
                Ok(contents) => self.apply_loaded_file(path, contents.clone(), window, cx),
                Err(message) => self.fail(Retry::Open(path), message.as_str(), cx),
            },
            (
                ProtocolMessage::FileWriteResponse { request_id, result },
                Some(Request::Write { id, path, text }),
            ) if *request_id == id => match result {
                Ok(()) => self.finish_save(path, text, cx),
                Err(message) => self.fail(Retry::Save(path), message.as_str(), cx),
            },
            (_, request) => self.request = request,
        }
    }

    fn on_buffer_changed(&mut self, contents: &str, cx: &mut Context<Self>) {
        let dirty = contents != self.saved_contents.as_str();
        if self.dirty == dirty {
            return;
        }
        self.dirty = dirty;
        self.publish_title(cx);
        cx.notify();
    }

    fn open_file(&mut self, _: &OpenFile, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.on_open.clone() else {
            return;
        };
        let folder = self
            .path
            .as_deref()
            .and_then(Path::parent)
            .unwrap_or(&self.cwd)
            .to_string_lossy()
            .into_owned();
        self.error = None;
        cx.notify();
        open(self.pane, folder, window, cx);
    }

    fn apply_loaded_file(
        &mut self,
        path: PathBuf,
        contents: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path_string = match validated_descriptor_path(&path, &self.cwd) {
            Ok(path_string) => path_string,
            Err(message) => {
                self.fail(Retry::Open(path), message, cx);
                return;
            }
        };
        self.editor.update(cx, |editor, cx| {
            editor.set_value(&contents, window, cx);
            editor.set_language(language_for_path(&path), window, cx);
        });
        self.path = Some(path);
        self.saved_contents = contents;
        self.dirty = false;
        self.error = None;
        self.publish_path(path_string, cx);
        self.publish_title(cx);
        cx.notify();
    }

    fn save_file(&mut self, _: &SaveFile, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = self.path.clone() {
            self.begin_save(path, cx);
        }
    }

    fn begin_save(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let path_string = match validated_descriptor_path(&path, &self.cwd) {
            Ok(path_string) => path_string,
            Err(message) => {
                self.fail(Retry::Save(path), message, cx);
                return;
            }
        };
        let text = self.editor.read(cx).value().to_string();
        if text.len() > zz_protocol::MAX_FILE_EDIT_BYTES {
            self.fail(
                Retry::Save(path),
                format!(
                    "{path_string} is larger than the {} MiB limit for saving from here",
                    zz_protocol::MAX_FILE_EDIT_BYTES / (1024 * 1024)
                ),
                cx,
            );
            return;
        }
        let pane = self.pane;
        let sent = text.clone();
        let id = self.connection.update(cx, |connection, cx| {
            connection
                .edits_files()
                .then(|| connection.write_file(pane, path_string, sent, cx))
        });
        match id {
            Some(id) => {
                self.request = Some(Request::Write { id, path, text });
                self.error = None;
                cx.notify();
            }
            None => self.fail(
                Retry::Save(path),
                "This host's zz cannot save files from here",
                cx,
            ),
        }
    }

    fn finish_save(&mut self, path: PathBuf, saved_contents: String, cx: &mut Context<Self>) {
        let path_string = match validated_descriptor_path(&path, &self.cwd) {
            Ok(path_string) => path_string,
            Err(message) => {
                self.fail(Retry::Save(path), message, cx);
                return;
            }
        };
        self.path = Some(path);
        self.saved_contents = saved_contents;
        self.dirty = self.editor.read(cx).value().as_ref() != self.saved_contents.as_str();
        self.error = None;
        self.publish_path(path_string, cx);
        self.publish_title(cx);
        cx.notify();
    }

    fn retry(&mut self, retry: Retry, cx: &mut Context<Self>) {
        match retry {
            Retry::Open(path) => self.load_file(path, cx),
            Retry::Save(path) => self.begin_save(path, cx),
        }
    }

    fn publish_path(&mut self, path: String, cx: &mut Context<Self>) {
        if self.snapshot_path.as_deref() == Some(path.as_str()) {
            self.pending_path = None;
            return;
        }
        self.pending_path = Some(path.clone());
        let pane = self.pane.to_string();
        self.connection.update(cx, |connection, cx| {
            connection.command("set-editor-path", vec!["-t".to_owned(), pane, path], cx);
        });
    }

    fn publish_title(&mut self, cx: &mut Context<Self>) {
        let title = editor_title(self.path.as_deref(), self.dirty);
        if title == self.last_title {
            return;
        }
        self.last_title.clone_from(&title);
        let pane = self.pane.to_string();
        self.connection.update(cx, |connection, cx| {
            connection.command(
                "select-pane",
                vec!["-t".to_owned(), pane, "-T".to_owned(), title],
                cx,
            );
        });
    }
}

impl Render for EditorPane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus_handle(cx);
        let empty =
            self.path.is_none() && self.editor.read(cx).value().is_empty() && self.error.is_none();
        let vim_mode = self.editor.read(cx).vim_mode();
        let entity = cx.entity();
        let error = self.error.clone().map(|error| {
            let title = match error.retry {
                Retry::Open(_) => "Couldn’t open this file",
                Retry::Save(_) => "Couldn’t save this file",
            };
            let retry = error.retry;
            let view = entity.clone();
            editor_error(
                self.pane,
                title,
                &error.message,
                move |_, cx| view.update(cx, |pane, cx| pane.retry(retry.clone(), cx)),
                cx,
            )
        });
        let open = empty.then(|| {
            let view = entity.clone();
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Button::new(("editor-open-file", self.pane.0))
                        .primary()
                        .label("Open a file")
                        .on_click(move |_, window, cx| {
                            view.update(cx, |pane, cx| pane.open_file(&OpenFile, window, cx));
                            cx.stop_propagation();
                        }),
                )
        });
        editor_text(div().id(("editor-pane", self.pane.0)), cx)
            .key_context(EDITOR_KEY_CONTEXT)
            .track_focus(&focus)
            .relative()
            .flex()
            .size_full()
            .overflow_hidden()
            .on_action(cx.listener(Self::open_file))
            .on_action(cx.listener(Self::save_file))
            .child(CodeEditor::new(&self.editor).background_opacity(0.0))
            .children(open)
            .when_some(vim_mode, |this, mode| {
                this.child(vim_mode_chip(self.pane, mode))
            })
            .children(error)
    }
}

#[allow(
    clippy::disallowed_methods,
    reason = "file content takes the code font at the editor font size"
)]
fn editor_text(root: ala::Stateful<ala::Div>, cx: &App) -> ala::Stateful<ala::Div> {
    root.font_family(cx.theme().mono_font_family.clone())
        .text_size(px(EDITOR_FONT_SIZE))
}
