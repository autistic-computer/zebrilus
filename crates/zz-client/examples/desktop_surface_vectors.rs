use std::{error::Error, fs};
use zz_client::{
    ChromeAction,
    desktop_surface::{
        BrowserNativeBinding, TerminalChromeKeyContext, TerminalFontSizeAdjustment,
        TerminalFontState, adjusted_terminal_font_state, browser_native_binding,
        sidebar_native_binding, terminal_chrome_key_action, terminal_font_size_offset,
        terminal_native_binding,
    },
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

fn browser_code(binding: BrowserNativeBinding) -> (u8, u8) {
    match binding {
        BrowserNativeBinding::None => (0, 0),
        BrowserNativeBinding::ZoomIn => (1, 0),
        BrowserNativeBinding::ZoomOut => (2, 0),
        BrowserNativeBinding::ZoomReset => (3, 0),
        BrowserNativeBinding::DevTools => (4, 0),
        BrowserNativeBinding::NewTab => (5, 0),
        BrowserNativeBinding::NextTab => (6, 0),
        BrowserNativeBinding::PreviousTab => (7, 0),
        BrowserNativeBinding::SelectTab(index) => (8, index),
        BrowserNativeBinding::SelectLastTab => (9, 0),
        BrowserNativeBinding::Back => (10, 0),
        BrowserNativeBinding::Forward => (11, 0),
        BrowserNativeBinding::Reload => (12, 0),
        BrowserNativeBinding::FocusAddress => (13, 0),
        BrowserNativeBinding::ElementSelector => (14, 0),
        BrowserNativeBinding::Undo => (15, 0),
        BrowserNativeBinding::Redo => (16, 0),
        BrowserNativeBinding::Cut => (17, 0),
        BrowserNativeBinding::Copy => (18, 0),
        BrowserNativeBinding::Paste => (19, 0),
        BrowserNativeBinding::PasteAndMatchStyle => (20, 0),
        BrowserNativeBinding::SelectAll => (21, 0),
        BrowserNativeBinding::ClosePane => (22, 0),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let actions = all_actions();
    let mut output = Vec::new();
    output.extend_from_slice(&u32::try_from((actions.len() + 1) * 8)?.to_le_bytes());
    for action in std::iter::once(None).chain(actions.iter().copied().map(Some)) {
        for flags in 0..8_u8 {
            output.extend_from_slice(&action.map_or(u32::MAX, code).to_le_bytes());
            output.push(flags);
            output.push(terminal_chrome_key_action(TerminalChromeKeyContext {
                action,
                swallowed: flags & 1 != 0,
                search_open: flags & 2 != 0,
                command_output: flags & 4 != 0,
            }) as u8);
        }
    }
    output.extend_from_slice(&u32::try_from(actions.len())?.to_le_bytes());
    for action in actions {
        output.extend_from_slice(&code(action).to_le_bytes());
        output.push(terminal_native_binding(action) as u8);
        output.push(sidebar_native_binding(action) as u8);
        let (kind, index) = browser_code(browser_native_binding(action));
        output.extend([kind, index]);
    }
    let sizes: [f32; 18] = [
        0.0,
        -0.0,
        f32::MIN_POSITIVE,
        f32::from_bits(1),
        1.0,
        f32::from_bits(1.0_f32.to_bits() + 1),
        2.0,
        7.0,
        13.0,
        13.5,
        48.0,
        128.0,
        255.0,
        256.0,
        f32::from_bits(256.0_f32.to_bits() + 1),
        512.0,
        -16.0,
        1024.0,
    ];
    let offsets: [f32; 11] = [
        -1024.0,
        -255.0,
        -1.0,
        -f32::EPSILON,
        -0.0,
        0.0,
        f32::from_bits(1),
        f32::EPSILON,
        1.0,
        255.0,
        1024.0,
    ];
    output.extend_from_slice(&u32::try_from(sizes.len() * offsets.len() * 2)?.to_le_bytes());
    for size in sizes {
        for offset in offsets {
            for increase in [false, true] {
                let state = TerminalFontState { size, offset };
                let adjustment = if increase {
                    TerminalFontSizeAdjustment::Increase
                } else {
                    TerminalFontSizeAdjustment::Decrease
                };
                let next = adjusted_terminal_font_state(state, adjustment);
                let result = next.unwrap_or(state);
                output.extend_from_slice(&size.to_bits().to_le_bytes());
                output.extend_from_slice(&offset.to_bits().to_le_bytes());
                output.extend([u8::from(increase), u8::from(next.is_some())]);
                output.extend_from_slice(&result.size.to_bits().to_le_bytes());
                output.extend_from_slice(&result.offset.to_bits().to_le_bytes());
                let (effective, adjusted_offset) = terminal_font_size_offset(size, offset);
                output.extend_from_slice(&effective.to_bits().to_le_bytes());
                output.extend_from_slice(&adjusted_offset.to_bits().to_le_bytes());
            }
        }
    }
    output.extend_from_slice(&4096_u32.to_le_bytes());
    let mut state = TerminalFontState {
        size: 13.0,
        offset: 0.0,
    };
    for step in 0..4096_u32 {
        let operation = u8::try_from(step % 11)?;
        let value = sizes[usize::try_from(step % u32::try_from(sizes.len())?)?];
        let changed = if operation < 2 {
            let (size, offset) = terminal_font_size_offset(value, state.offset);
            state = TerminalFontState { size, offset };
            false
        } else {
            let adjustment = if operation & 1 == 0 {
                TerminalFontSizeAdjustment::Increase
            } else {
                TerminalFontSizeAdjustment::Decrease
            };
            let next = adjusted_terminal_font_state(state, adjustment);
            state = next.unwrap_or(state);
            next.is_some()
        };
        output.push(operation);
        output.extend_from_slice(&value.to_bits().to_le_bytes());
        output.push(u8::from(changed));
        output.extend_from_slice(&state.size.to_bits().to_le_bytes());
        output.extend_from_slice(&state.offset.to_bits().to_le_bytes());
    }
    fs::write(std::env::args().nth(1).ok_or("missing output")?, output)?;
    Ok(())
}
