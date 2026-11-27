use super::*;
use zz_daemon_client::DaemonError;

#[test]
fn recorded_keys_save_as_tmux_spellings() {
    let recorded = |source: &str, key_char: Option<&str>| {
        let mut keystroke = ala::Keystroke::parse(source).unwrap();
        keystroke.key_char = key_char.map(str::to_owned);
        recorded_tmux_key(&keystroke)
    };
    assert_eq!(recorded("ctrl-b", None).as_deref(), Some("C-b"));
    assert_eq!(recorded("\"", Some("\"")).as_deref(), Some("\""));
    assert_eq!(recorded("shift-l", Some("L")).as_deref(), Some("L"));
    assert_eq!(recorded("alt-a", Some("å")).as_deref(), Some("M-a"));
    assert_eq!(recorded("ctrl-space", None).as_deref(), Some("C-Space"));
    assert_eq!(recorded("alt-up", None).as_deref(), Some("M-Up"));
    assert_eq!(recorded("f5", None).as_deref(), Some("F5"));
    assert_eq!(recorded("cmd-b", None), None);
}

#[test]
fn percentage_inputs_convert_and_reject_invalid_edits() {
    for (key, default, max) in [(ConfigKey::PaneGlowStrength, "100", 200.0)] {
        assert_eq!(numeric_range(key), (0.0, max));
        assert_eq!(validate_numeric_value(key, &max.to_string()), Ok(max));
        assert!(validate_numeric_value(key, &(max + 1.0).to_string()).is_err());
        for (factor, displayed) in [(0.0, "0"), (0.15, "15"), (0.5, "50"), (1.0, "100")] {
            assert_eq!(numeric_input_text(key, factor), displayed);
            let parsed = validate_numeric_value(key, displayed).expect("valid percentage");
            assert!((parsed / numeric_input_scale(key) - factor).abs() < f32::EPSILON);
        }
        for value in ["", "-1", "NaN", "inf", "invalid"] {
            assert!(validate_numeric_value(key, value).is_err(), "{value}");
        }
        assert!(numeric_input_matches_value(key, "50.", 0.5));
        assert!(numeric_input_matches_value(key, "0.", 0.0));
        assert!(!numeric_input_matches_value(key, "", 0.5));
        assert!(!numeric_input_matches_value(key, "50.", 1.0));
        assert_eq!(
            numeric_input_text(key, numeric_config_value(&AppConfig::default(), key).value),
            default,
        );
    }
    assert_eq!(numeric_range(ConfigKey::PaneInactiveOpacity), (0.0, 1.0));
    assert_eq!(
        numeric_input_text(ConfigKey::PaneInactiveOpacity, 0.7),
        "0.7"
    );
}

#[ala::test]
fn the_zoom_field_applies_complete_values_and_ignores_partial_ones(cx: &mut ala::TestAppContext) {
    use std::{cell::RefCell, rc::Rc};

    cx.update(zz_ui::init);
    let captured = Rc::new(RefCell::new(None));
    let captured_for_window = Rc::clone(&captured);
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("test client".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        captured_for_window.replace(Some(cx.entity()));
        SettingsView::new(mux, window, cx)
    });
    let settings = captured.borrow().clone().expect("captured settings view");
    let input = settings.read_with(cx, |settings, _| settings.ui_zoom.clone());

    let typed = |value: &str, cx: &mut ala::VisualTestContext| {
        let value = value.to_owned();
        cx.update(|window, cx| {
            input.update(cx, |input, cx| input.set_value(value, window, cx));
            settings.update(cx, |settings, cx| {
                SettingsView::preview_ui_zoom(&input, cx);
                settings.synchronize_ui_zoom_input(window, cx);
                (crate::ui_scale::percent(cx), input.read(cx).value().clone())
            })
        })
    };

    assert_eq!(
        typed("1", cx).0,
        100,
        "a prefix must not zoom to the minimum"
    );
    assert_eq!(typed("15", cx), (100, "15".into()), "nor be rewritten");
    assert_eq!(typed("150", cx), (150, "150".into()));
    assert_eq!(typed("", cx), (150, "".into()), "clearing must not zoom");
    assert_eq!(
        typed("4000", cx),
        (150, "4000".into()),
        "out of range is left to the commit"
    );

    cx.update(|window, cx| {
        settings.update(cx, |_, cx| {
            SettingsView::commit_ui_zoom_input(&input, window, cx);
        });
    });
    cx.update(|_, cx| {
        let percent = crate::ui_scale::percent(cx);
        assert!(percent < 4000, "the commit clamps what the field holds");
        assert_eq!(input.read(cx).value().as_ref(), percent.to_string());
    });

    typed("250", cx);
    cx.update(|window, cx| {
        crate::ui_scale::reset(cx);
        settings.update(cx, |settings, cx| {
            settings.synchronize_ui_zoom_input(window, cx);
        });
    });
    cx.update(|_, cx| {
        assert_eq!(crate::ui_scale::percent(cx), 100);
        assert_eq!(input.read(cx).value().as_ref(), "100");
    });
}
