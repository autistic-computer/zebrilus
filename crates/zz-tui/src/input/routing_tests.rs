use super::test_support::{parse_tty, tty_sequence_samples, tty_text_samples};
use super::*;
use zz_client::{ClientCore, InputEvent, InputOwner, SIDEBAR_TABLE, SurfaceKind};

fn routing_model() -> Model {
    use zz_protocol::{
        LayoutNode, MuxSnapshot, PaneId, PaneSnapshot, PaneSummary, SessionId, SessionSnapshot,
        WindowId, WindowSnapshot,
    };

    let mut model = base_model();
    model.size.columns = 120;
    model.attached_session = Some(SessionId(1));
    let panes = [
        (PaneId(1), PaneKindSnapshot::Terminal),
        (
            PaneId(2),
            PaneKindSnapshot::Browser(zz_protocol::BrowserDescriptor::single(
                "about:blank".to_owned(),
                "default".to_owned(),
            )),
        ),
    ]
    .into_iter()
    .map(|(id, kind)| {
        (
            id,
            PaneSnapshot {
                id,
                title: id.to_string(),
                kind,
                synchronized_input: false,
                bell: false,
                dead: false,
                dead_status: None,
                border_colour: None,
                active_border_colour: None,
                border_status_text: String::new(),
                mode: None,
                status: None,
                summary: PaneSummary::default(),
            },
        )
    })
    .collect();
    model.update_snapshot(std::sync::Arc::new(MuxSnapshot {
        generation: 1,
        sessions: vec![SessionSnapshot {
            id: SessionId(1),
            name: "session".to_owned(),
            active_window: WindowId(1),
            windows: vec![WindowSnapshot {
                id: WindowId(1),
                index: 0,
                name: "window".to_owned(),
                automatic_rename: true,
                active_pane: PaneId(1),
                zoomed_pane: None,
                layout: LayoutNode::Pane(PaneId(1)),
                panes,
                layout_dump: String::new(),
                visible_layout_dump: String::new(),
                status_label: String::new(),
                activity: false,
                silence: false,
                pane_border_status: zz_protocol::PaneBorderStatus::Off,
                pane_border_lines: zz_protocol::PaneBorderLines::Single,
                pane_border_indicators: zz_protocol::PaneBorderIndicators::Colour,
                pane_order: vec![PaneId(1), PaneId(2)],
                pane_z_order: Vec::new(),
                floating: Vec::new(),
                modal: None,
                sx: 0,
                sy: 0,
                pane_base_index: 0,
            }],
            viewers: Vec::new(),
        }],
        focused_window: Some(WindowId(1)),
    }));
    model
}

#[test]
fn a_plain_key_pane_is_offered_only_while_handle_would_forward_the_key_unchanged() {
    let mut model = routing_model();
    let browser = BrowserState::new(None);
    let pane = zz_protocol::PaneId(1);
    assert_eq!(plain_key_pane(&model, &browser, false, false), Some(pane));
    let samples = (0..0x80_u8)
        .map(|byte| vec![byte])
        .chain(
            tty_text_samples()
                .into_iter()
                .map(|character| character.to_string().into_bytes()),
        )
        .chain(tty_sequence_samples())
        .filter_map(|sample| {
            let (daemon, width) = zz_protocol::tty_input_key(&sample)?;
            (width == sample.len()).then_some((sample, daemon))
        })
        .collect::<Vec<_>>();
    assert!(samples.len() > 1000, "{}", samples.len());
    for (sample, daemon) in samples {
        let (events, _) = parse_tty(&sample);
        let [TerminalEvent::Key(event)] = events.as_slice() else {
            panic!("{sample:?}: {events:?}");
        };
        let owner = model.router.owner();
        assert_eq!(
            route_test_key(&mut model, *event, PrefixView::default()),
            [InputMessage::Key {
                pane,
                input: daemon,
                text_follows: false,
            }],
            "{sample:?}"
        );
        assert_eq!(model.router.owner(), owner);
        assert_eq!(plain_key_pane(&model, &browser, false, false), Some(pane));
    }
    assert_eq!(plain_key_pane(&model, &browser, true, false), None);
    model.menu_action_pending = true;
    assert_eq!(plain_key_pane(&model, &browser, false, false), None);
    model.menu_action_pending = false;
    model.focus_sidebar();
    assert_eq!(plain_key_pane(&model, &browser, false, false), None);
}

fn route_test_key(model: &mut Model, event: KeyEvent, prefix: PrefixView) -> Vec<InputMessage> {
    let mut sent = Vec::new();
    handle_routed_key(
        model,
        &mut BrowserState::new(None),
        event,
        prefix,
        |input| {
            sent.push(input);
            Ok(())
        },
        |model, target| apply_sidebar_target(model, target, |_| Ok(())),
    )
    .unwrap();
    sent
}

#[test]
fn sidebar_down_moves_selection_and_unbound_keys_stay_local() {
    let mut model = routing_model();
    model.focus_sidebar();
    let selected = model.sidebar.selected;
    assert!(
        route_test_key(
            &mut model,
            KeyEvent::new(TerminalKeyCode::Down, KeyModifiers::NONE),
            PrefixView::default()
        )
        .is_empty()
    );
    assert_eq!(model.sidebar.selected, selected + 1);
    assert!(
        route_test_key(
            &mut model,
            KeyEvent::new(TerminalKeyCode::Char('x'), KeyModifiers::NONE),
            PrefixView::default()
        )
        .is_empty()
    );
    assert_eq!(model.sidebar.selected, selected + 1);
    assert_eq!(model.router.owner(), InputOwner::Sidebar);
}

#[test]
fn pane_plain_key_forwards_but_unclaimed_release_keeps_the_kitty_gate() {
    let mut model = routing_model();
    let mut event = KeyEvent::new(TerminalKeyCode::Char('x'), KeyModifiers::NONE);
    assert_eq!(
        route_test_key(&mut model, event, PrefixView::default()),
        vec![InputMessage::Key {
            pane: zz_protocol::PaneId(1),
            input: key_input(event),
            text_follows: false,
        }]
    );
    event.kind = KeyEventKind::Release;
    assert!(route_test_key(&mut model, event, PrefixView::default()).is_empty());
}

#[test]
fn prefix_claim_precedes_sidebar_and_pairs_release_without_kitty_keyboard() {
    let mut model = routing_model();
    model.focus_sidebar();
    let selected = model.sidebar.selected;
    let mut core = ClientCore::new();
    core.handle_message(zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
        sequence: 1,
        payload: zz_protocol::EventPayload::PrefixArmed { armed: true },
    }));
    let mut event = KeyEvent::new(TerminalKeyCode::Down, KeyModifiers::NONE);
    for (kind, forwards) in [(KeyEventKind::Press, true), (KeyEventKind::Repeat, false)] {
        event.kind = kind;
        let input = key_input(event);
        let prefix = PrefixView {
            armed: core.prefix_armed(),
            claimed: core.claims_prefix_input(&input),
        };
        assert!(prefix.armed && prefix.claimed);
        let sent = route_test_key(&mut model, event, prefix);
        if forwards {
            assert_eq!(
                sent,
                vec![InputMessage::Key {
                    pane: zz_protocol::PaneId(1),
                    input,
                    text_follows: false
                }]
            );
        } else {
            assert!(sent.is_empty());
        }
    }
    assert_eq!(model.sidebar.selected, selected);
    model.activate_pane(zz_protocol::PaneId(2));
    event.kind = KeyEventKind::Release;
    assert_eq!(
        route_test_key(&mut model, event, PrefixView::default()),
        vec![InputMessage::Key {
            pane: zz_protocol::PaneId(1),
            input: key_input(event),
            text_follows: false,
        }]
    );
}

#[test]
fn sidebar_confirm_keeps_window_focus_and_activates_pane_targets() {
    let mut model = routing_model();
    for (target, owner) in [
        (
            SidebarTarget::Window(zz_protocol::WindowId(1)),
            InputOwner::Sidebar,
        ),
        (
            SidebarTarget::Pane(zz_protocol::PaneId(2)),
            InputOwner::Pane(zz_protocol::PaneId(2), SurfaceKind::Browser),
        ),
        (
            SidebarTarget::NewPane(zz_protocol::PaneId(1)),
            InputOwner::Pane(zz_protocol::PaneId(1), SurfaceKind::Terminal),
        ),
    ] {
        model.focus_sidebar();
        model.sidebar.selected = model
            .sidebar_rows()
            .iter()
            .position(|row| row.target == Some(target))
            .unwrap();
        let mut activated = Vec::new();
        let mut sent = Vec::new();
        handle_routed_key(
            &mut model,
            &mut BrowserState::new(None),
            KeyEvent::new(TerminalKeyCode::Enter, KeyModifiers::NONE),
            PrefixView::default(),
            |input| {
                sent.push(input);
                Ok(())
            },
            |model, target| {
                apply_sidebar_target(model, target, |target| {
                    activated.push(target);
                    Ok(())
                })
            },
        )
        .unwrap();
        assert!(sent.is_empty());
        assert_eq!(activated, vec![target]);
        assert_eq!(model.router.owner(), owner);
    }
}

#[test]
fn snapshot_changes_preserve_sidebar_owner_and_update_pane_surface() {
    let mut model = routing_model();
    model.focus_sidebar();
    let mut snapshot = (*model.snapshot).clone();
    snapshot.sessions[0].windows[0].active_pane = zz_protocol::PaneId(2);
    model.update_snapshot(std::sync::Arc::new(snapshot));
    assert_eq!(model.router.owner(), InputOwner::Sidebar);
    assert_eq!(
        model.router.active_pane(),
        Some((zz_protocol::PaneId(2), SurfaceKind::Browser))
    );
    model.focus_active_pane();
    let mut snapshot = (*model.snapshot).clone();
    snapshot.sessions[0].windows[0].active_pane = zz_protocol::PaneId(1);
    model.update_snapshot(std::sync::Arc::new(snapshot));
    assert_eq!(
        model.router.owner(),
        InputOwner::Pane(zz_protocol::PaneId(1), SurfaceKind::Terminal)
    );
}

#[test]
fn focus_loss_pane_removal_and_detach_clear_claimed_releases() {
    for event in [
        InputEvent::WindowDeactivated,
        InputEvent::PaneRemoved(zz_protocol::PaneId(1)),
        InputEvent::Detached,
    ] {
        let mut model = routing_model();
        let mut key = KeyEvent::new(TerminalKeyCode::Char('a'), KeyModifiers::CONTROL);
        assert_eq!(
            route_test_key(
                &mut model,
                key,
                PrefixView {
                    armed: true,
                    claimed: true
                }
            )
            .len(),
            1
        );
        if event == InputEvent::WindowDeactivated {
            model.client_focus_changed(false);
        } else {
            model.input_event(event);
        }
        key.kind = KeyEventKind::Release;
        assert!(route_test_key(&mut model, key, PrefixView::default()).is_empty());
    }
}

#[test]
fn unavailable_browser_zoom_forwards_the_key() {
    let mut model = routing_model();
    let mut snapshot = (*model.snapshot).clone();
    snapshot.sessions[0].windows[0].active_pane = zz_protocol::PaneId(2);
    model.update_snapshot(std::sync::Arc::new(snapshot));
    let event = KeyEvent::new(TerminalKeyCode::Char('='), KeyModifiers::CONTROL);
    assert_eq!(
        route_test_key(&mut model, event, PrefixView::default()),
        vec![InputMessage::Key {
            pane: zz_protocol::PaneId(2),
            input: key_input(event),
            text_follows: false,
        }]
    );
}

fn base_model() -> Model {
    let core = zz_client::ClientCore::new();
    let endpoint = Endpoint::parse("unix:///tmp/zz-input-base-test.sock").expect("test endpoint");
    Model::new(
        &core,
        crate::tty::TerminalSize {
            columns: 40,
            rows: 20,
            cell_width_px: 8,
            cell_height_px: 16,
        },
        "host".to_owned(),
        "host".to_owned(),
        endpoint.clone(),
        endpoint,
        Vec::new(),
    )
}

#[test]
fn browser_surfaces_forward_key_releases_through_the_daemon() {
    assert!(should_forward_key(KeyEventKind::Release, true, false));
    assert!(should_forward_key(KeyEventKind::Release, false, true));
    assert!(!should_forward_key(KeyEventKind::Release, false, false));
    assert!(should_forward_key(KeyEventKind::Press, false, false));
}

#[test]
fn chrome_leaves_every_pane_key_to_the_daemon() {
    let chrome = zz_client::ChromeKeymap::new();
    assert!(chrome.bindings().iter().all(|(_, _, action)| matches!(
        action,
        ChromeAction::SidebarSelectUp
            | ChromeAction::SidebarSelectDown
            | ChromeAction::SidebarConfirm
            | ChromeAction::SidebarRename
            | ChromeAction::SidebarCancel
            | ChromeAction::ToggleSidebar
            | ChromeAction::BrowserZoomIn
            | ChromeAction::BrowserZoomOut
            | ChromeAction::BrowserZoomReset
    )));
    for event in [
        KeyEvent::new(TerminalKeyCode::Char('\\'), KeyModifiers::CONTROL),
        KeyEvent::new(TerminalKeyCode::Char('s'), KeyModifiers::ALT),
        KeyEvent::new(TerminalKeyCode::Char('S'), KeyModifiers::ALT),
        KeyEvent::new(TerminalKeyCode::Char('x'), KeyModifiers::NONE),
    ] {
        assert_eq!(chrome.resolve("ui", &key_input(event)), None);
    }
    assert_eq!(
        chrome.resolve(
            SIDEBAR_TABLE,
            &key_input(KeyEvent::new(
                TerminalKeyCode::Char('q'),
                KeyModifiers::NONE
            ))
        ),
        Some(ChromeAction::ToggleSidebar)
    );
}

#[test]
fn sidebar_keys_come_from_the_chrome_table() {
    let mut chrome = zz_client::ChromeKeymap::new();
    let key = KeyEvent::new(TerminalKeyCode::Char('k'), KeyModifiers::NONE);
    assert_eq!(
        chrome.resolve(SIDEBAR_TABLE, &key_input(key)),
        Some(ChromeAction::SidebarSelectUp)
    );
    assert!(chrome.unbind(SIDEBAR_TABLE, "k"));
    assert_eq!(chrome.resolve(SIDEBAR_TABLE, &key_input(key)), None);
    chrome
        .bind(SIDEBAR_TABLE, "x", "sidebar-select-up")
        .unwrap();
    let rebound = KeyEvent::new(TerminalKeyCode::Char('x'), KeyModifiers::NONE);
    assert_eq!(
        chrome.resolve(SIDEBAR_TABLE, &key_input(rebound)),
        Some(ChromeAction::SidebarSelectUp)
    );
}

#[test]
fn focus_events_forward_to_terminals_only() {
    assert_eq!(pane_focus_input(None, true), None);
    assert_eq!(
        pane_focus_input(
            Some((zz_protocol::PaneId(7), &PaneKindSnapshot::Terminal)),
            false,
        ),
        Some(InputMessage::TerminalView {
            pane: zz_protocol::PaneId(7),
            action: TerminalViewAction::Focus(false),
        })
    );
    assert_eq!(
        pane_focus_input(
            Some((zz_protocol::PaneId(8), &PaneKindSnapshot::Picker)),
            true,
        ),
        None
    );
}

#[test]
fn rename_commands_use_stable_session_and_window_targets() {
    let session = rename_command(
        SidebarEditKind::RenameSession(zz_protocol::SessionId(7)),
        "work tree",
    )
    .unwrap();
    assert_eq!(session.name, "rename-session");
    assert_eq!(session.args, ["-t", "$7", "work tree"]);

    let window = rename_command(
        SidebarEditKind::RenameWindow(zz_protocol::WindowId(9)),
        "logs",
    )
    .unwrap();
    assert_eq!(window.name, "rename-window");
    assert_eq!(window.args, ["-t", "@9", "logs"]);
    assert!(rename_command(SidebarEditKind::RenameWindow(zz_protocol::WindowId(9)), "").is_none());
    assert!(rename_command(SidebarEditKind::AddHost, "ignored").is_none());
}

#[test]
fn add_host_input_matches_fleet_add_endpoint_and_validation_rules() {
    assert_eq!(
        parse_add_host_input("box user@box:2222").unwrap(),
        ("box".to_owned(), "ssh://user@box:2222".to_owned())
    );
    assert_eq!(
        parse_add_host_input("box ssh://box").unwrap(),
        ("box".to_owned(), "ssh://box".to_owned())
    );
    assert_eq!(
        parse_add_host_input("local box").unwrap_err(),
        "invalid `host-local`: host name `local` is reserved"
    );
    assert_eq!(
        parse_add_host_input("box -oProxyCommand=nope").unwrap_err(),
        "ssh destination must not start with `-`"
    );
    assert!(parse_add_host_input("box").is_err());
    assert!(parse_add_host_input("box user@@box").is_err());
}
