use std::{error::Error, fs};
use zz_client::{
    ChromeAction,
    desktop_input::{DesktopChromeDispatch, desktop_chrome_dispatch},
};

fn code(value: ChromeAction) -> u32 {
    match value {
        ChromeAction::NewSession => 0,
        ChromeAction::NewWindow => 256,
        ChromeAction::SelectWindow(index) => 2 * 256 + u32::from(index),
        ChromeAction::SplitRight => 768,
        ChromeAction::SplitDown => 1024,
        ChromeAction::Detach => 1280,
        ChromeAction::ToggleSidebar => 1536,
        ChromeAction::ClosePane => 1792,
        ChromeAction::OpenSettings => 2048,
        ChromeAction::OpenCommandPalette => 2304,
        ChromeAction::UiZoomIn => 2560,
        ChromeAction::UiZoomOut => 2816,
        ChromeAction::UiZoomReset => 3072,
        ChromeAction::SidebarCancel => 3328,
        ChromeAction::SidebarConfirm => 3584,
        ChromeAction::SidebarRename => 3840,
        ChromeAction::SidebarCommandPalette => 4096,
        ChromeAction::SidebarSelectUp => 4352,
        ChromeAction::SidebarSelectDown => 4608,
        ChromeAction::SidebarSelectLeft => 4864,
        ChromeAction::SidebarSelectRight => 5120,
        ChromeAction::SidebarSelectFirst => 5376,
        ChromeAction::SidebarSelectLast => 5632,
        ChromeAction::TerminalFontIncrease => 5888,
        ChromeAction::TerminalFontDecrease => 6144,
        ChromeAction::TerminalSearch => 6400,
        ChromeAction::TerminalCopy => 6656,
        ChromeAction::TerminalSelectAll => 6912,
        ChromeAction::TerminalClearHistory => 7168,
        ChromeAction::TerminalPaste => 7424,
        ChromeAction::BrowserZoomIn => 7680,
        ChromeAction::BrowserZoomOut => 7936,
        ChromeAction::BrowserZoomReset => 8192,
        ChromeAction::BrowserDevTools => 8448,
        ChromeAction::BrowserNewTab => 8704,
        ChromeAction::BrowserNextTab => 8960,
        ChromeAction::BrowserPreviousTab => 9216,
        ChromeAction::BrowserSelectTab(index) => 37 * 256 + u32::from(index),
        ChromeAction::BrowserSelectLastTab => 9728,
        ChromeAction::BrowserBack => 9984,
        ChromeAction::BrowserForward => 10240,
        ChromeAction::BrowserReload => 10496,
        ChromeAction::BrowserFocusAddress => 10752,
        ChromeAction::BrowserElementSelector => 11008,
        ChromeAction::BrowserUndo => 11264,
        ChromeAction::BrowserRedo => 11520,
        ChromeAction::BrowserCut => 11776,
        ChromeAction::BrowserCopy => 12032,
        ChromeAction::BrowserPaste => 12288,
        ChromeAction::BrowserPasteAndMatchStyle => 12544,
        ChromeAction::BrowserSelectAll => 12800,
    }
}

fn all_actions() -> Vec<ChromeAction> {
    let mut values = vec![
        ChromeAction::NewSession,
        ChromeAction::NewWindow,
        ChromeAction::SplitRight,
        ChromeAction::SplitDown,
        ChromeAction::Detach,
        ChromeAction::ToggleSidebar,
        ChromeAction::ClosePane,
        ChromeAction::OpenSettings,
        ChromeAction::OpenCommandPalette,
        ChromeAction::UiZoomIn,
        ChromeAction::UiZoomOut,
        ChromeAction::UiZoomReset,
        ChromeAction::SidebarCancel,
        ChromeAction::SidebarConfirm,
        ChromeAction::SidebarRename,
        ChromeAction::SidebarCommandPalette,
        ChromeAction::SidebarSelectUp,
        ChromeAction::SidebarSelectDown,
        ChromeAction::SidebarSelectLeft,
        ChromeAction::SidebarSelectRight,
        ChromeAction::SidebarSelectFirst,
        ChromeAction::SidebarSelectLast,
        ChromeAction::TerminalFontIncrease,
        ChromeAction::TerminalFontDecrease,
        ChromeAction::TerminalSearch,
        ChromeAction::TerminalCopy,
        ChromeAction::TerminalSelectAll,
        ChromeAction::TerminalClearHistory,
        ChromeAction::TerminalPaste,
        ChromeAction::BrowserZoomIn,
        ChromeAction::BrowserZoomOut,
        ChromeAction::BrowserZoomReset,
        ChromeAction::BrowserDevTools,
        ChromeAction::BrowserNewTab,
        ChromeAction::BrowserNextTab,
        ChromeAction::BrowserPreviousTab,
        ChromeAction::BrowserSelectLastTab,
        ChromeAction::BrowserBack,
        ChromeAction::BrowserForward,
        ChromeAction::BrowserReload,
        ChromeAction::BrowserFocusAddress,
        ChromeAction::BrowserElementSelector,
        ChromeAction::BrowserUndo,
        ChromeAction::BrowserRedo,
        ChromeAction::BrowserCut,
        ChromeAction::BrowserCopy,
        ChromeAction::BrowserPaste,
        ChromeAction::BrowserPasteAndMatchStyle,
        ChromeAction::BrowserSelectAll,
    ];
    for index in 0..=u8::MAX {
        values.push(ChromeAction::SelectWindow(index));
        values.push(ChromeAction::BrowserSelectTab(index));
    }
    values.sort_by_key(|value| code(*value));
    values
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut bytes = Vec::new();
    for action in all_actions() {
        let (kind, index) = match desktop_chrome_dispatch(action) {
            DesktopChromeDispatch::None => (0, 0),
            DesktopChromeDispatch::NewSession => (1, 0),
            DesktopChromeDispatch::NewWindow => (2, 0),
            DesktopChromeDispatch::SelectWindow(index) => (3, index),
            DesktopChromeDispatch::SplitRight => (4, 0),
            DesktopChromeDispatch::SplitDown => (5, 0),
            DesktopChromeDispatch::Detach => (6, 0),
            DesktopChromeDispatch::ToggleSidebar => (7, 0),
            DesktopChromeDispatch::OpenSettings => (8, 0),
            DesktopChromeDispatch::OpenCommandPalette => (9, 0),
        };
        bytes.extend_from_slice(&code(action).to_le_bytes());
        bytes.extend([kind, index]);
    }
    fs::write(std::env::args().nth(1).ok_or("missing output path")?, bytes)?;
    Ok(())
}
