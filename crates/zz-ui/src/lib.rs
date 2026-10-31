pub mod tmux_style;

pub(crate) use penna::{
    badge, button, check, feedback, icon, input, kbd, list, menu, motion, notification, picker,
    pulse, scroll, separator, sidebar, spinner, text, tooltip, touch,
};

pub(crate) use penna::{
    ActiveTheme, CHROME_GAP, Colorize, Disableable, Edge, ElementExt, Elevation, Emphasis, Fill,
    Ground, Highlight, Icon, IconName, InteractiveSurfaceExt, Layer, PANE_HEADER_CONTROL_HEIGHT,
    Paint, PaintExt, PaintState, RULE_WIDTH, Selectable, Shape, Sizable, StatefulSurfaceExt,
    StyledExt, Surface, SurfaceRole, ThemeMode, Tone, WindowExt, h_flex, list_row, muted_on,
    oklab_lightness, oklch, rems_from_px, row_corners, row_state, row_surface, to_oklch, v_flex,
};

#[cfg(feature = "agent")]
pub mod agent;
pub mod attachment;
pub mod browser;
pub mod chooser;
pub mod command;
pub mod compact;
pub mod dialogs;
pub mod files;
pub mod interface_theme;
pub mod mend;
pub mod navigation;
pub mod pane;
pub mod pane_look;
pub mod pip;
pub mod program;
pub mod roles;
pub mod settings;
pub mod shell;
pub mod target;
pub mod terminal;
pub mod terminal_images;

pub use program::{Assets, CHROME_ICON, CLAUDE_ICON, OPENAI_ICON, ZZ_LOGO};

pub fn register() {
    penna::register(roles::ZZ, &roles::RETIRED, &interface_theme::LAYERS);
}

pub fn init(cx: &mut ala::App) {
    register();
    #[cfg(feature = "tree-sitter")]
    register_tmux_language();
    penna::init(cx);
}

#[cfg(feature = "tree-sitter")]
fn register_tmux_language() {
    let tmux = penna::highlighter::LanguageConfig::new(
        "tmux",
        tree_sitter_tmux::LANGUAGE,
        Vec::new(),
        tree_sitter_tmux::HIGHLIGHTS_QUERY,
        "",
        "",
    );
    let registry = penna::highlighter::LanguageRegistry::singleton();
    registry.register("tmux", &tmux);
    registry.register("tmux.conf", &tmux);
}

#[cfg(test)]
pub(crate) fn init_without_theme(cx: &mut ala::App) {
    init(cx);
    interface_theme::InterfaceTheme::default().activate(Some(ThemeMode::Light), None, cx);
}

pub const LABEL_FADE: ala::Pixels = ala::px(28.0);

#[cfg(all(test, feature = "tree-sitter"))]
mod tests {
    use penna::{
        code_editor::Rope,
        highlighter::{HighlightTheme, SyntaxHighlighter},
    };

    #[ala::test]
    fn a_tmux_fence_highlights_after_init(cx: &mut ala::TestAppContext) {
        cx.update(super::init);
        let source = "# Prefix\nunbind C-b\nset -g prefix C-a\nset -g history-limit 10000\n";
        let theme = HighlightTheme::default_dark();
        for fence in ["tmux", "tmux.conf"] {
            let mut highlighter = SyntaxHighlighter::new(fence);
            assert_eq!(highlighter.language().as_ref(), "tmux", "{fence}");
            assert!(
                highlighter.update(None, &Rope::from(source), None),
                "{fence}"
            );
            assert!(
                highlighter
                    .styles(&(0..source.len()), &theme)
                    .iter()
                    .any(|(_, style)| style.color.is_some()),
                "{fence} did not produce a colored capture"
            );
        }
    }
}
