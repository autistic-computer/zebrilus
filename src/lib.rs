#[cfg(feature = "host")]
mod artifact;
mod cookie;
#[cfg(not(target_family = "wasm"))]
pub mod element_picker;
mod event;
#[cfg(feature = "host")]
#[cfg_attr(not(feature = "cef-runtime"), allow(dead_code))]
mod frame;
mod ime;
mod input;
mod lifecycle;
#[cfg(any(feature = "cef-runtime", test))]
mod page_zoom;
#[cfg(any(feature = "cef-runtime", all(test, feature = "host")))]
mod popup;
#[cfg_attr(not(feature = "cef-runtime"), allow(dead_code, unused_imports))]
mod profile;

#[cfg(all(feature = "host", any(target_os = "linux", target_os = "freebsd")))]
mod readback_texture;

#[cfg(all(feature = "host", any(target_os = "linux", target_os = "freebsd")))]
pub use readback_texture::ReadbackTexture;

#[cfg(feature = "cef-runtime")]
#[allow(
    unsafe_code,
    clippy::transmute_ptr_to_ptr,
    reason = "CEF callbacks expose a scoped raw BGRA paint buffer"
)]
mod cef_runtime;

#[cfg(all(feature = "cef-runtime", target_os = "linux"))]
#[allow(unsafe_code)]
mod linux_dmabuf;

#[cfg(feature = "host")]
pub use artifact::{
    ARTIFACT_PROFILE, ArtifactFile, ArtifactReply, ArtifactRequest, MAX_ARTIFACT_PATH_BYTES,
    MAX_ARTIFACT_REPLY_BYTES, artifact_replies, artifact_requests, drop_local_page,
    host_local_page, is_local_page, local_scheme, parse_artifact_url,
};
pub use cookie::{
    BrowserCookie, BrowserCookiePriority, BrowserCookieSameSite, CookieImportBatch,
    CookieImportError, CookieImportResult, MAX_COOKIE_IMPORT_BYTES, MAX_COOKIE_IMPORT_COUNT,
    SiteDataClearResult, parse_cookie_import,
};
#[cfg(not(target_family = "wasm"))]
pub use element_picker::ElementPickerAppearance;
pub use event::{
    BrowserCursor, BrowserEvent, ContextMenuRequest, EditFlags, MAX_FAVICON_BYTES, SessionId,
};
#[cfg(feature = "host")]
pub use frame::{
    BrowserGpuContext, FrameError, FrameMailbox, FrameMailboxDiagnostics, FrameTier, GpuFrame,
    OsrFrame, OwnedBgraFrame,
};
#[cfg(all(feature = "host", target_os = "macos"))]
pub use frame::{MacFramePresenter, MacGpuFrame, MacIoSurface};
#[cfg(all(feature = "host", target_os = "windows"))]
pub use frame::{WinGpuFrame, WinGpuTexture};
pub use ime::{ImeCharacterBounds, ime_range_bounds};
pub use input::{
    BrowserKey, EditCommand, KeyAction, KeyInput, Modifiers, PointerButton, PointerEvent,
    PointerPhase, Viewport, WheelEvent, named_key_input,
};
pub use lifecycle::{RuntimePhase, SessionPhase};
pub use profile::{
    BrowserProfileError, BrowserProfileNameError, BrowserProfilePaths, DEFAULT_BROWSER_PROFILE,
    MAX_BROWSER_PROFILE_NAME_BYTES, normalize_browser_profile_name,
};

#[cfg(feature = "cef-runtime")]
pub use cef_runtime::{
    AcceleratedPaintDiagnostics, AcceleratedPaintHandleDiagnostics, AcceleratedPaintObservation,
    AcceleratedPaintPlaneDiagnostics, BrowserBootstrap, BrowserCommandSink, BrowserError,
    BrowserMessagePump, BrowserRuntime, BrowserSession, RuntimeConfig, RuntimeSignal, bootstrap,
    bootstrap_with_profile_paths, run_subprocess,
};

#[cfg(all(feature = "cef-runtime", target_os = "windows"))]
pub use cef_runtime::bootstrap_windows;
