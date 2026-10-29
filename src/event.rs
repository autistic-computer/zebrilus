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
    pub selection_text: Option<Arc<str>>,
    pub editable: bool,
    pub edit_flags: EditFlags,
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
            | Self::FrameReady { session, .. }
            | Self::SharedTextureFailed { session, .. }
            | Self::LoadFailed { session, .. }
            | Self::CursorChanged { session, .. }
            | Self::ImeCompositionChanged { session, .. }
            | Self::ElementPicked { session, .. }
            | Self::ElementPickCancelled { session }
            | Self::ElementPickFailed { session }
            | Self::ContextMenuRequested { session, .. }
            | Self::PopupCreated { session, .. }
            | Self::PopupRequested { session, .. }
            | Self::RenderProcessTerminated { session, .. }
            | Self::Closed { session } => *session,
        }
    }
}
