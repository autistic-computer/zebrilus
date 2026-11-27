use super::*;
use ala::Keystroke;

#[test]
fn ime_bounds_map_cef_view_rects_into_window_coordinates() {
    let content = Bounds::new(point(px(120.0), px(64.0)), ala::size(px(800.0), px(600.0)));
    let hidpi_zoomed = [
        ImeCharacterBounds::from_view(60, 30, 30, 45, 1.5 * 2.0),
        ImeCharacterBounds::from_view(90, 30, 30, 45, 1.5 * 2.0),
    ];
    assert_eq!(
        ime_window_bounds(content, &hidpi_zoomed, 0..2),
        Bounds::new(point(px(140.0), px(74.0)), ala::size(px(20.0), px(15.0)))
    );
    assert_eq!(
        ime_window_bounds(content, &hidpi_zoomed, 2..2),
        Bounds::new(point(px(160.0), px(74.0)), ala::size(px(1.0), px(15.0)))
    );
    let logical = [ImeCharacterBounds::from_view(5, 7, 9, 18, 1.0)];
    assert_eq!(
        ime_window_bounds(content, &logical, 0..1),
        Bounds::new(point(px(125.0), px(71.0)), ala::size(px(9.0), px(18.0)))
    );
    assert_eq!(
        ime_window_bounds(content, &[], 0..0),
        Bounds::new(content.origin, ala::size(px(1.0), px(18.0)))
    );
}

#[test]
fn routes_plain_and_ime_keys_through_ala_text_input() {
    assert!(allows_text_input(
        BrowserKey::Character('a'),
        ala::Modifiers::default()
    ));
    assert!(allows_text_input(
        BrowserKey::Space,
        ala::Modifiers {
            shift: true,
            ..Default::default()
        }
    ));
    assert!(allows_text_input(
        BrowserKey::Unidentified,
        ala::Modifiers::default()
    ));
}

#[test]
fn ala_browser_emits_one_correlatable_key_and_text_pair() {
    let keystroke = Keystroke {
        key: "x".to_owned(),
        key_char: Some("x".to_owned()),
        modifiers: ala::Modifiers::default(),
    };
    let (key, text_follows) =
        browser_surface_key_input(PaneId(7), &keystroke, TerminalKeyAction::Press);
    assert!(text_follows);
    assert!(matches!(
        key,
        InputMessage::BrowserSurfaceKey {
            pane: PaneId(7),
            input: TerminalKeyInput {
                action: TerminalKeyAction::Press,
                ..
            },
            text_follows: true,
        }
    ));
    assert_eq!(
        browser_surface_text_input(PaneId(7), "x"),
        InputMessage::BrowserSurfaceText {
            pane: PaneId(7),
            text: "x".to_owned(),
        }
    );
}

#[test]
fn modified_and_navigation_keys_do_not_emit_committed_text() {
    assert!(!allows_text_input(
        BrowserKey::Character('c'),
        ala::Modifiers {
            control: true,
            ..Default::default()
        }
    ));
    assert!(!allows_text_input(
        BrowserKey::Character('v'),
        ala::Modifiers {
            platform: true,
            ..Default::default()
        }
    ));
    assert!(!allows_text_input(
        BrowserKey::ArrowLeft,
        ala::Modifiers::default()
    ));
}
