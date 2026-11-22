use std::sync::Arc;

use crate::ImeCharacterBounds;

pub const MAX_FAVICON_BYTES: usize = 8 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SessionId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BrowserCursor {
    #[default]
    Arrow,
    IBeam,
    PointingHand,
    Crosshair,
    Wait,
    Help,
    Move,
    ResizeHorizontal,
    ResizeVertical,
    ResizeNorthEastSouthWest,
    ResizeNorthWestSouthEast,
    NotAllowed,
    Grab,
    Grabbing,
    None,
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "one field per Chromium edit-state flag keeps the menu mapping literal"
)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EditFlags {
    pub can_cut: bool,
    pub can_copy: bool,
    pub can_paste: bool,
    pub can_select_all: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContextMenuRequest {
    pub x: i32,
    pub y: i32,
    pub link_url: Option<Arc<str>>,
    pub src_url: Option<Arc<str>>,
    pub is_image: bool,
    pub selection_text: Option<Arc<str>>,
    pub misspelled_word: Option<Arc<str>>,
    pub dictionary_suggestions: Arc<[Arc<str>]>,
    pub editable: bool,
    pub edit_flags: EditFlags,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileDialogKind {
    Open,
    OpenMultiple,
    OpenFolder,
    Save,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDialogRequest {
    pub id: u64,
    pub kind: FileDialogKind,
    pub title: Option<Arc<str>>,
    pub default_file_path: Option<Arc<str>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsDialogKind {
    Alert,
    Confirm,
    Prompt,
    BeforeUnload { is_reload: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsDialogRequest {
    pub id: u64,
    pub kind: JsDialogKind,
    pub origin_url: Arc<str>,
    pub message: Arc<str>,
    pub default_prompt: Option<Arc<str>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadRequest {
    pub id: u32,
    pub url: Arc<str>,
    pub suggested_name: Arc<str>,
    pub total_bytes: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadProgress {
    pub id: u32,
    pub file_name: Arc<str>,
    pub full_path: Arc<str>,
    pub received_bytes: i64,
    pub total_bytes: i64,
    pub percent_complete: i32,
    pub complete: bool,
    pub canceled: bool,
    pub interrupted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdfPrintResult {
    pub path: Arc<str>,
    pub success: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BrowserEvent {
    Created {
        session: SessionId,
    },
    AddressChanged {
        session: SessionId,
        url: Arc<str>,
    },
    TitleChanged {
        session: SessionId,
        title: Arc<str>,
    },
    FaviconChanged {
        session: SessionId,
        url: Arc<str>,
        png: Arc<[u8]>,
    },
    LoadingChanged {
        session: SessionId,
        loading: bool,
        can_go_back: bool,
        can_go_forward: bool,
    },
    LoadingProgressChanged {
        session: SessionId,
        progress: f32,
    },
    StatusMessageChanged {
        session: SessionId,
        message: Arc<str>,
    },
    FullscreenModeChanged {
        session: SessionId,
        fullscreen: bool,
    },
    FindResult {
        session: SessionId,
        count: i32,
        active_ordinal: i32,
        final_update: bool,
    },
    FrameReady {
        session: SessionId,
        generation: u64,
    },
    SharedTextureFailed {
        session: SessionId,
        reason: Arc<str>,
    },
    LoadFailed {
        session: SessionId,
        code: i32,
        description: Arc<str>,
        url: Arc<str>,
    },
    CursorChanged {
        session: SessionId,
        cursor: BrowserCursor,
    },
    ImeCompositionChanged {
        session: SessionId,
        bounds: Arc<[ImeCharacterBounds]>,
    },
    ElementPicked {
        session: SessionId,
        text: Arc<str>,
        screenshot: Option<Arc<[u8]>>,
    },
    ElementPickCancelled {
        session: SessionId,
    },
    ElementPickFailed {
        session: SessionId,
    },
    ContextMenuRequested {
        session: SessionId,
        request: ContextMenuRequest,
    },
    FileDialogRequested {
        session: SessionId,
        request: FileDialogRequest,
    },
    JsDialogRequested {
        session: SessionId,
        request: JsDialogRequest,
    },
    DownloadRequested {
        session: SessionId,
        request: DownloadRequest,
    },
    DownloadUpdated {
        session: SessionId,
        progress: DownloadProgress,
    },
    PopupCreated {
        session: SessionId,
        popup: SessionId,
        url: Arc<str>,
        foreground: bool,
    },
    PopupRequested {
        session: SessionId,
        url: Arc<str>,
        foreground: bool,
    },
    RenderProcessTerminated {
        session: SessionId,
        status: Arc<str>,
        error_code: i32,
    },
    Closed {
        session: SessionId,
    },
}

impl BrowserEvent {
    #[must_use]
    pub fn session(&self) -> SessionId {
        match self {
            Self::Created { session }
            | Self::AddressChanged { session, .. }
            | Self::TitleChanged { session, .. }
            | Self::FaviconChanged { session, .. }
            | Self::LoadingChanged { session, .. }
            | Self::LoadingProgressChanged { session, .. }
            | Self::StatusMessageChanged { session, .. }
            | Self::FullscreenModeChanged { session, .. }
            | Self::FindResult { session, .. }
            | Self::FrameReady { session, .. }
            | Self::SharedTextureFailed { session, .. }
            | Self::LoadFailed { session, .. }
            | Self::CursorChanged { session, .. }
            | Self::ImeCompositionChanged { session, .. }
            | Self::ElementPicked { session, .. }
            | Self::ElementPickCancelled { session }
            | Self::ElementPickFailed { session }
            | Self::ContextMenuRequested { session, .. }
            | Self::FileDialogRequested { session, .. }
            | Self::JsDialogRequested { session, .. }
            | Self::DownloadRequested { session, .. }
            | Self::DownloadUpdated { session, .. }
            | Self::PopupCreated { session, .. }
            | Self::PopupRequested { session, .. }
            | Self::RenderProcessTerminated { session, .. }
            | Self::Closed { session } => *session,
        }
    }
}
