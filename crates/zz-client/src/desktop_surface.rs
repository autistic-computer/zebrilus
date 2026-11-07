use crate::ChromeAction;

pub const TERMINAL_RAW_CHORDS: [&str; 2] = ["tab", "shift-tab"];

pub const MIN_TERMINAL_FONT_SIZE_POINTS: f32 = 1.0;
pub const MAX_TERMINAL_FONT_SIZE_POINTS: f32 = 256.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalChromeKeyAction {
    Native,
    FontIncrease,
    FontDecrease,
    Swallow,
    Search,
    FocusSearch,
    Copy,
    SelectAll,
    ClearHistory,
    Paste,
    Consumed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalChromeKeyContext {
    pub action: Option<ChromeAction>,
    pub swallowed: bool,
    pub search_open: bool,
    pub command_output: bool,
}

#[must_use]
pub fn terminal_chrome_key_action(context: TerminalChromeKeyContext) -> TerminalChromeKeyAction {
    match context.action {
        Some(ChromeAction::TerminalFontIncrease) => return TerminalChromeKeyAction::FontIncrease,
        Some(ChromeAction::TerminalFontDecrease) => return TerminalChromeKeyAction::FontDecrease,
        _ => {}
    }
    if context.swallowed {
        return TerminalChromeKeyAction::Swallow;
    }
    if context.action == Some(ChromeAction::TerminalSearch) {
        return TerminalChromeKeyAction::Search;
    }
    if context.search_open {
        return TerminalChromeKeyAction::FocusSearch;
    }
    match context.action {
        Some(ChromeAction::TerminalCopy) => TerminalChromeKeyAction::Copy,
        Some(ChromeAction::TerminalSelectAll) => TerminalChromeKeyAction::SelectAll,
        Some(ChromeAction::TerminalClearHistory) if context.command_output => {
            TerminalChromeKeyAction::Consumed
        }
        Some(ChromeAction::TerminalClearHistory) => TerminalChromeKeyAction::ClearHistory,
        Some(ChromeAction::TerminalPaste) => TerminalChromeKeyAction::Paste,
        _ => TerminalChromeKeyAction::Native,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalFontSizeAdjustment {
    Increase,
    Decrease,
}

impl TerminalFontSizeAdjustment {
    #[must_use]
    pub const fn delta_points(self) -> f32 {
        match self {
            Self::Increase => 1.0,
            Self::Decrease => -1.0,
        }
    }
}

#[must_use]
pub fn adjusted_terminal_font_size(current: f32, adjustment: TerminalFontSizeAdjustment) -> f32 {
    (current + adjustment.delta_points())
        .clamp(MIN_TERMINAL_FONT_SIZE_POINTS, MAX_TERMINAL_FONT_SIZE_POINTS)
}

#[must_use]
pub fn terminal_font_size_offset(configured: f32, offset: f32) -> (f32, f32) {
    let effective =
        (configured + offset).clamp(MIN_TERMINAL_FONT_SIZE_POINTS, MAX_TERMINAL_FONT_SIZE_POINTS);
    (effective, effective - configured)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerminalFontState {
    pub size: f32,
    pub offset: f32,
}

#[must_use]
pub fn adjusted_terminal_font_state(
    state: TerminalFontState,
    adjustment: TerminalFontSizeAdjustment,
) -> Option<TerminalFontState> {
    let next = adjusted_terminal_font_size(state.size, adjustment);
    let applied_delta = next - state.size;
    if applied_delta.abs() < f32::EPSILON {
        return None;
    }
    Some(TerminalFontState {
        size: next,
        offset: state.offset + applied_delta,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalNativeBinding {
    None,
    Suppress,
    Copy,
    Paste,
    SelectAll,
    Search,
    ClearHistory,
}

#[must_use]
pub const fn terminal_native_binding(action: ChromeAction) -> TerminalNativeBinding {
    match action {
        ChromeAction::TerminalFontIncrease | ChromeAction::TerminalFontDecrease => {
            TerminalNativeBinding::Suppress
        }
        ChromeAction::TerminalCopy => TerminalNativeBinding::Copy,
        ChromeAction::TerminalPaste => TerminalNativeBinding::Paste,
        ChromeAction::TerminalSelectAll => TerminalNativeBinding::SelectAll,
        ChromeAction::TerminalSearch => TerminalNativeBinding::Search,
        ChromeAction::TerminalClearHistory => TerminalNativeBinding::ClearHistory,
        _ => TerminalNativeBinding::None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarNativeBinding {
    None,
    Cancel,
    Confirm,
    Rename,
    CommandPalette,
    SelectUp,
    SelectDown,
    SelectLeft,
    SelectRight,
    SelectFirst,
    SelectLast,
}

#[must_use]
pub const fn sidebar_native_binding(action: ChromeAction) -> SidebarNativeBinding {
    match action {
        ChromeAction::SidebarCancel => SidebarNativeBinding::Cancel,
        ChromeAction::SidebarConfirm => SidebarNativeBinding::Confirm,
        ChromeAction::SidebarRename => SidebarNativeBinding::Rename,
        ChromeAction::SidebarCommandPalette => SidebarNativeBinding::CommandPalette,
        ChromeAction::SidebarSelectUp => SidebarNativeBinding::SelectUp,
        ChromeAction::SidebarSelectDown => SidebarNativeBinding::SelectDown,
        ChromeAction::SidebarSelectLeft => SidebarNativeBinding::SelectLeft,
        ChromeAction::SidebarSelectRight => SidebarNativeBinding::SelectRight,
        ChromeAction::SidebarSelectFirst => SidebarNativeBinding::SelectFirst,
        ChromeAction::SidebarSelectLast => SidebarNativeBinding::SelectLast,
        _ => SidebarNativeBinding::None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserNativeBinding {
    None,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    DevTools,
    NewTab,
    NextTab,
    PreviousTab,
    SelectTab(u8),
    SelectLastTab,
    Back,
    Forward,
    Reload,
    FocusAddress,
    ElementSelector,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    PasteAndMatchStyle,
    SelectAll,
    ClosePane,
}

#[must_use]
pub const fn browser_native_binding(action: ChromeAction) -> BrowserNativeBinding {
    match action {
        ChromeAction::BrowserZoomIn => BrowserNativeBinding::ZoomIn,
        ChromeAction::BrowserZoomOut => BrowserNativeBinding::ZoomOut,
        ChromeAction::BrowserZoomReset => BrowserNativeBinding::ZoomReset,
        ChromeAction::BrowserDevTools => BrowserNativeBinding::DevTools,
        ChromeAction::BrowserNewTab => BrowserNativeBinding::NewTab,
        ChromeAction::BrowserNextTab => BrowserNativeBinding::NextTab,
        ChromeAction::BrowserPreviousTab => BrowserNativeBinding::PreviousTab,
        ChromeAction::BrowserSelectTab(index) => BrowserNativeBinding::SelectTab(index),
        ChromeAction::BrowserSelectLastTab => BrowserNativeBinding::SelectLastTab,
        ChromeAction::BrowserBack => BrowserNativeBinding::Back,
        ChromeAction::BrowserForward => BrowserNativeBinding::Forward,
        ChromeAction::BrowserReload => BrowserNativeBinding::Reload,
        ChromeAction::BrowserFocusAddress => BrowserNativeBinding::FocusAddress,
        ChromeAction::BrowserElementSelector => BrowserNativeBinding::ElementSelector,
        ChromeAction::BrowserUndo => BrowserNativeBinding::Undo,
        ChromeAction::BrowserRedo => BrowserNativeBinding::Redo,
        ChromeAction::BrowserCut => BrowserNativeBinding::Cut,
        ChromeAction::BrowserCopy => BrowserNativeBinding::Copy,
        ChromeAction::BrowserPaste => BrowserNativeBinding::Paste,
        ChromeAction::BrowserPasteAndMatchStyle => BrowserNativeBinding::PasteAndMatchStyle,
        ChromeAction::BrowserSelectAll => BrowserNativeBinding::SelectAll,
        ChromeAction::ClosePane => BrowserNativeBinding::ClosePane,
        _ => BrowserNativeBinding::None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiNativeBinding {
    None,
    Increase,
    Decrease,
    Reset,
}

#[must_use]
pub const fn ui_native_binding(action: ChromeAction) -> UiNativeBinding {
    match action {
        ChromeAction::UiZoomIn => UiNativeBinding::Increase,
        ChromeAction::UiZoomOut => UiNativeBinding::Decrease,
        ChromeAction::UiZoomReset => UiNativeBinding::Reset,
        _ => UiNativeBinding::None,
    }
}

#[must_use]
pub fn ui_zoom_for_percent(percent: f32) -> f32 {
    percent.clamp(50.0, 300.0) / 100.0
}

#[must_use]
pub fn ui_percent_for_zoom(zoom: f32) -> f32 {
    zoom * 100.0
}
