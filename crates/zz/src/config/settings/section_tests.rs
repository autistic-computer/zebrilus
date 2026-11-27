use super::*;
use zz_daemon_client::{DaemonError, Endpoint};

#[ala::test]
fn heavy_section_state_is_initialized_on_first_visit(cx: &mut ala::TestAppContext) {
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

    settings.read_with(cx, |settings, _| {
        assert!(settings.terminal_config_editor.is_some());
        assert!(settings.mux_config_editor.is_none());
        assert!(settings.hosts_state.is_none());
    });

    let initialized = cx.update(|window, cx| {
        settings.update(cx, |settings, cx| {
            settings.ensure_section_state(SettingsSection::Hosts, window, cx)
        })
    });
    assert!(initialized);
    settings.read_with(cx, |settings, _| {
        assert!(settings.hosts_state.is_some());
        assert!(settings.mux_config_editor.is_none());
    });

    let initialized = cx.update(|window, cx| {
        settings.update(cx, |settings, cx| {
            settings.ensure_section_state(SettingsSection::Hosts, window, cx)
        })
    });
    assert!(!initialized);

    let initialized = cx.update(|window, cx| {
        settings.update(cx, |settings, cx| {
            settings.ensure_section_state(SettingsSection::Terminal, window, cx)
        })
    });
    assert!(!initialized);
    settings.read_with(cx, |settings, _| {
        assert!(settings.terminal_config_editor.is_some());
        assert!(settings.mux_config_editor.is_none());
    });

    let initialized = cx.update(|window, cx| {
        settings.update(cx, |settings, cx| {
            settings.ensure_section_state(SettingsSection::Multiplexer, window, cx)
        })
    });
    assert!(initialized);
    settings.read_with(cx, |settings, _| {
        assert!(settings.terminal_config_editor.is_some());
        assert!(settings.mux_config_editor.is_some());
    });

    let initialized = cx.update(|window, cx| {
        settings.update(cx, |settings, cx| {
            settings.ensure_section_state(SettingsSection::Multiplexer, window, cx)
        })
    });
    assert!(!initialized);
}

#[ala::test]
fn ui_font_select_follows_live_config_reloads(cx: &mut ala::TestAppContext) {
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
    let family = "Test UI font".to_owned();
    let config = AppConfig::default();

    cx.update(|window, cx| {
        cx.set_global(config);
        cx.set_global(config::UiFontConfig(zz_config::UiFontConfig {
            family: ConfigValue {
                value: Some(family.clone()),
                provenance: ConfigProvenance::Override,
            },
            ..zz_config::UiFontConfig::default()
        }));
        settings.update(cx, |settings, cx| {
            settings.ui_font_family.update(cx, |select, cx| {
                *select = SelectState::new(
                    vec![
                        SettingsSelectItem::new("System default", ""),
                        SettingsSelectItem::new(family.clone(), family.clone()),
                        SettingsSelectItem::new(
                            zz_config::DEFAULT_UI_FONT_FAMILY,
                            zz_config::DEFAULT_UI_FONT_FAMILY,
                        ),
                    ],
                    Some(IndexPath::new(0)),
                    window,
                    cx,
                );
            });
            settings.synchronize_numeric_inputs(window, cx);
            settings.synchronize_ui_font(window, cx);
        });
    });

    settings.read_with(cx, |settings, cx| {
        assert_eq!(
            settings
                .ui_font_family
                .read(cx)
                .selected_value()
                .map(String::as_str),
            Some(family.as_str()),
        );
    });
    cx.update(|window, cx| {
        cx.set_global(config::UiFontConfig::default());
        settings.update(cx, |settings, cx| settings.synchronize_ui_font(window, cx));
    });
    settings.read_with(cx, |settings, cx| {
        assert_eq!(
            settings
                .ui_font_family
                .read(cx)
                .selected_value()
                .map(String::as_str),
            Some(zz_config::DEFAULT_UI_FONT_FAMILY),
        );
    });
}

#[ala::test]
fn submit_add_host_rejects_a_duplicate_without_changing_config(cx: &mut ala::TestAppContext) {
    use std::{cell::RefCell, rc::Rc};

    cx.update(zz_ui::init);
    let configured = config::HostEntry {
        name: "desktop".to_owned(),
        endpoint: Endpoint::parse("ssh://desktop").expect("valid test endpoint"),
    };
    cx.update(|cx| config::set_fleet_hosts_for_test(vec![configured.clone()], cx));
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

    cx.update(|window, cx| {
        settings.update(cx, |settings, cx| {
            assert!(settings.ensure_section_state(SettingsSection::Hosts, window, cx));
            let input = settings
                .hosts_state
                .as_ref()
                .expect("initialized Hosts state")
                .input
                .clone();
            input.update(cx, |input, cx| {
                input.set_value("fabrico@desktop", window, cx);
            });
            settings.submit_add_host(window, cx);
        });
    });

    settings.read_with(cx, |settings, _| {
        assert_eq!(
            settings
                .hosts_state
                .as_ref()
                .and_then(|state| state.error.as_deref()),
            Some("A host named `desktop` already exists."),
        );
    });
    cx.update(|_, cx| {
        assert_eq!(config::fleet_hosts(cx), vec![configured]);
    });
}
