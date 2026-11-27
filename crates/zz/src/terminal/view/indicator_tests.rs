use super::*;

#[test]
fn mode_indicator_uses_tags_without_bracket_decoration() {
    assert_eq!(
        mode_indicator(
            TerminalMode::Copy {
                position: 42,
                total: 900,
                hide_position: false,
            },
            0,
        ),
        Some(ModeIndicator {
            label: Some("Copy mode"),
            detail: "42 / 900".to_owned(),
        })
    );
    assert_eq!(
        mode_indicator(
            TerminalMode::View {
                position: 7,
                total: 12,
            },
            0,
        ),
        Some(ModeIndicator {
            label: Some("View mode"),
            detail: "7 / 12".to_owned(),
        })
    );
}

#[test]
fn copy_mode_indicator_keeps_unseen_output_visible() {
    assert_eq!(
        mode_indicator(
            TerminalMode::Copy {
                position: 4,
                total: 20,
                hide_position: false,
            },
            3,
        ),
        Some(ModeIndicator {
            label: Some("Copy mode"),
            detail: "4 / 20 · +3 output".to_owned(),
        })
    );
    assert_eq!(mode_indicator(TerminalMode::Live, 0), None);
}

#[test]
fn hide_position_drops_the_copy_position_and_keeps_the_rest() {
    assert_eq!(
        mode_indicator(
            TerminalMode::Copy {
                position: 42,
                total: 900,
                hide_position: true,
            },
            0,
        ),
        Some(ModeIndicator {
            label: Some("Copy mode"),
            detail: String::new(),
        })
    );
    assert_eq!(
        mode_indicator(
            TerminalMode::Copy {
                position: 42,
                total: 900,
                hide_position: true,
            },
            3,
        ),
        Some(ModeIndicator {
            label: Some("Copy mode"),
            detail: "+3 output".to_owned(),
        })
    );
}
