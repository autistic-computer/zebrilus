use std::{
    path::{Path, PathBuf},
    rc::Rc,
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
    editor::{EditorDocument, Publish, Restore, Retry, language_for_path},
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
const CONNECTION_LOST: &str = "The connection to the host dropped before it answered";

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

enum Request {
    Read {
        id: u64,
        epoch: u64,
        path: PathBuf,
    },
    Write {
        id: u64,
        epoch: u64,
        path: PathBuf,
        text: String,
    },
}

impl Request {
    const fn epoch(&self) -> u64 {
        match self {
            Self::Read { epoch, .. } | Self::Write { epoch, .. } => *epoch,
        }
    }
}

pub(crate) struct EditorPane {
    pane: PaneId,
    connection: Entity<Connection>,
    editor: Entity<CodeEditorState>,
    document: EditorDocument,
    request: Option<Request>,
    on_open: Option<OpenHandler>,
    _subscriptions: Vec<Subscription>,
}

impl EditorPane {
    pub(crate) fn new(
        pane: PaneId,
        descriptor: &EditorDescriptor,
        title: &str,
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
                let dirty = pane.document.is_dirty();
                let title = pane.document.buffer_changed(contents.as_ref());
                pane.publish(Publish { path: None, title }, cx);
                if dirty != pane.document.is_dirty() {
                    cx.notify();
                }
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
        let epochs = cx.observe(&connection, |pane, connection, cx| {
            let epoch = connection.read(cx).epoch();
            pane.drop_stale_request(epoch, cx);
        });
        let mut document = EditorDocument::new(descriptor, title);
        let restore = document.restore(descriptor);
        let mut pane = Self {
            pane,
            connection,
            editor,
            document,
            request: None,
            on_open: None,
            _subscriptions: vec![changes, answers, epochs],
        };
        pane.apply_restore(restore, window, cx);
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
        if self.document.is_dirty() && self.document.path().is_some() && self.document.holds_file()
        {
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
        title: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(restore) = self.document.synchronize(descriptor, title) {
            self.apply_restore(restore, window, cx);
        }
    }

    fn apply_restore(&mut self, restore: Restore, window: &mut Window, cx: &mut Context<Self>) {
        self.request = None;
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
            self.load_file(path, cx);
        }
        cx.notify();
    }

    fn load_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let Some(text) = path.to_str().map(str::to_owned) else {
            self.document
                .fail(Retry::Open(path), "Editor paths must be valid UTF-8");
            cx.notify();
            return;
        };
        let pane = self.pane;
        let sent = self.connection.update(cx, |connection, cx| {
            connection
                .read_file(pane, text, cx)
                .map(|id| (id, connection.epoch()))
        });
        if let Some((id, epoch)) = sent {
            self.request = Some(Request::Read { id, epoch, path });
        } else {
            let message = self.unsent(cx);
            self.document.fail(Retry::Open(path), message);
        }
        cx.notify();
    }

    fn unsent(&self, cx: &App) -> &'static str {
        if self.connection.read(cx).edits_files() {
            "Not connected to the host"
        } else {
            "This host's zz cannot open or save files here"
        }
    }

    fn drop_stale_request(&mut self, epoch: u64, cx: &mut Context<Self>) {
        let Some(request) = self.request.take_if(|request| request.epoch() != epoch) else {
            return;
        };
        let retry = match request {
            Request::Read { path, .. } => Retry::Open(path),
            Request::Write { path, .. } => Retry::Save(path),
        };
        self.document.fail(retry, CONNECTION_LOST);
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
                Some(Request::Read { id, path, .. }),
            ) if *request_id == id => match result {
                Ok(contents) => self.apply_loaded_file(path, contents, window, cx),
                Err(message) => self.document.fail(Retry::Open(path), message.as_str()),
            },
            (
                ProtocolMessage::FileWriteResponse { request_id, result },
                Some(Request::Write { id, path, text, .. }),
            ) if *request_id == id => match result {
                Ok(()) => {
                    let current = self.editor.read(cx).value();
                    if let Some(publish) = self.document.saved(path, text, &current) {
                        self.publish(publish, cx);
                    }
                }
                Err(message) => self.document.fail(Retry::Save(path), message.as_str()),
            },
            (_, request) => {
                self.request = request;
                return;
            }
        }
        cx.notify();
    }

    fn open_file(&mut self, _: &OpenFile, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.on_open.clone() else {
            return;
        };
        let folder = self
            .document
            .path()
            .and_then(Path::parent)
            .unwrap_or(self.document.cwd())
            .to_string_lossy()
            .into_owned();
        self.document.clear_error();
        cx.notify();
        open(self.pane, folder, window, cx);
    }

    fn apply_loaded_file(
        &mut self,
        path: PathBuf,
        contents: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let language = language_for_path(&path);
        let Some(publish) = self.document.loaded(path, contents) else {
            return;
        };
        self.editor.update(cx, |editor, cx| {
            editor.set_value(contents, window, cx);
            editor.set_language(language, window, cx);
        });
        self.publish(publish, cx);
    }

    fn save_file(&mut self, _: &SaveFile, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = self.document.path() {
            let path = path.to_owned();
            self.begin_save(path, cx);
        }
    }

    fn begin_save(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.request.is_some() {
            return;
        }
        let Some(path_string) = self.document.prepare_save(&path) else {
            cx.notify();
            return;
        };
        let text = self.editor.read(cx).value().to_string();
        if text.len() > zz_protocol::MAX_FILE_EDIT_BYTES {
            self.document.fail(
                Retry::Save(path),
                format!(
                    "{path_string} is larger than the {} MiB limit for saving from here",
                    zz_protocol::MAX_FILE_EDIT_BYTES / (1024 * 1024)
                ),
            );
            cx.notify();
            return;
        }
        let pane = self.pane;
        let sent = text.clone();
        let request = self.connection.update(cx, |connection, cx| {
            connection
                .write_file(pane, path_string, sent, cx)
                .map(|id| (id, connection.epoch()))
        });
        if let Some((id, epoch)) = request {
            self.request = Some(Request::Write {
                id,
                epoch,
                path,
                text,
            });
        } else {
            let message = self.unsent(cx);
            self.document.fail(Retry::Save(path), message);
        }
        cx.notify();
    }

    fn retry(&mut self, retry: Retry, cx: &mut Context<Self>) {
        match retry {
            Retry::Open(path) => self.load_file(path, cx),
            Retry::Save(path) => self.begin_save(path, cx),
        }
    }

    fn publish(&self, publish: Publish, cx: &mut Context<Self>) {
        let pane = self.pane.to_string();
        self.connection.update(cx, |connection, cx| {
            if let Some(path) = publish.path {
                connection.command(
                    "set-editor-path",
                    vec!["-t".to_owned(), pane.clone(), path],
                    cx,
                );
            }
            if let Some(title) = publish.title {
                connection.command(
                    "select-pane",
                    vec!["-t".to_owned(), pane, "-T".to_owned(), title],
                    cx,
                );
            }
        });
    }
}

impl Render for EditorPane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus_handle(cx);
        let error = self.document.error().cloned();
        let empty = self.document.path().is_none()
            && self.editor.read(cx).value().is_empty()
            && error.is_none();
        let waiting = !self.document.holds_file();
        let vim_mode = self.editor.read(cx).vim_mode();
        let entity = cx.entity();
        let error = error.map(|error| {
            let retry = error.retry.clone();
            let view = entity.clone();
            editor_error(
                self.pane,
                error.title(),
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
            .child(
                CodeEditor::new(&self.editor)
                    .background_opacity(0.0)
                    .disabled(waiting),
            )
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
