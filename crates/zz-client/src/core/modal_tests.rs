use super::test_support::{drain, event, snapshot_with};
use super::*;

#[test]
fn a_new_chooser_state_drops_the_presentation_the_last_one_left() {
    let tree_state = || ChooseTreeState {
        items: Vec::new(),
        search: None,
        selected: 0,
        kind: zz_protocol::ChooseTreeKind::Windows,
        filter_no_matches: false,
        prompt: String::new(),
        help: false,
    };
    let presentation = |rows: &str| ChooserPresentation {
        selected: 0,
        rows: vec![zz_protocol::ChooserRow {
            name: String::new(),
            text: rows.to_owned(),
            align: false,
        }],
        sort: "index".to_owned(),
        view: "preview".to_owned(),
        filter: false,
        selection_style: String::new(),
        border_style: String::new(),
        prompt_style: String::new(),
        preview_size: zz_protocol::ChooserPreviewSize::Normal,
        preview: None,
        prompt_cursor: zz_protocol::PromptCursor::default(),
        prompt_column: 0,
        prompt_line: String::new(),
    };

    let mut core = ClientCore::new();
    core.handle_message(event(EventPayload::ChooseTree {
        state: Some(tree_state()),
    }));
    core.handle_message(event(EventPayload::ChooserPresentation {
        presentation: Some(Box::new(presentation("ZZTREE<%1>"))),
    }));
    assert!(core.chooser_presentation().is_some());

    core.handle_message(event(EventPayload::ChooseTree { state: None }));
    assert!(core.chooser_presentation().is_none());

    core.handle_message(event(EventPayload::ChooseTree {
        state: Some(tree_state()),
    }));
    assert!(core.chooser_presentation().is_none());

    core.handle_message(event(EventPayload::ChooserPresentation {
        presentation: Some(Box::new(presentation("bash"))),
    }));
    assert_eq!(
        core.chooser_presentation()
            .map(|presentation| presentation.rows[0].text.as_str()),
        Some("bash")
    );
}

#[test]
fn chooser_deltas_preserve_static_filter_fallback_state() {
    let mut core = ClientCore::new();
    core.handle_message(event(EventPayload::ChooseTree {
        state: Some(ChooseTreeState {
            items: Vec::new(),
            search: None,
            selected: 0,
            kind: zz_protocol::ChooseTreeKind::Windows,
            filter_no_matches: true,
            prompt: String::new(),
            help: false,
        }),
    }));
    core.handle_message(event(EventPayload::ChooseTreeUpdate {
        search: Some(ChooseTreeSearchState {
            query: "tree".to_owned(),
            reverse: false,
        }),
        selected: 4,
    }));
    let tree = core.choose_tree().expect("retained tree chooser");
    assert!(tree.filter_no_matches);
    assert_eq!(
        tree.search.as_ref().map(|search| search.query.as_str()),
        Some("tree")
    );

    core.handle_message(event(EventPayload::ChooseBuffer {
        state: Some(ChooseBufferState {
            items: Vec::new(),
            search: None,
            selected: 0,
            filter_no_matches: true,
            help: false,
            prompt: String::new(),
        }),
    }));
    core.handle_message(event(EventPayload::ChooseBufferUpdate {
        search: Some(ChooseBufferSearchState {
            query: "buffer".to_owned(),
            reverse: true,
        }),
        selected: 5,
    }));
    let buffer = core.choose_buffer().expect("retained buffer chooser");
    assert!(buffer.filter_no_matches);
    assert_eq!(
        buffer.search.as_ref().map(|search| search.query.as_str()),
        Some("buffer")
    );
}

fn modal_core(capture_keys: bool) -> (ClientCore, PaneId, PaneId) {
    let tiled = PaneId(1);
    let modal = PaneId(2);
    let mut snapshot = snapshot_with(&[tiled, modal]);
    let window = &mut snapshot.sessions[0].windows[0];
    window.active_pane = modal;
    window.floating = vec![zz_protocol::FloatingPaneSnapshot {
        pane: modal,
        xoff: 10,
        yoff: 5,
        sx: 30,
        sy: 8,
        visible: true,
        border_lines: zz_protocol::PaneBorderLines::Single,
        border_status: zz_protocol::PaneBorderStatus::Off,
    }];
    window.modal = Some(zz_protocol::ModalPaneSnapshot {
        pane: modal,
        capture_keys,
        close_on_click: false,
        close_on_cancel: true,
    });
    let mut core = ClientCore::new();
    core.handle_message(ProtocolMessage::Attached {
        session: SessionId(0),
        snapshot,
        read_only: false,
        client_flags: String::new(),
    });
    (core, tiled, modal)
}

#[test]
fn a_focused_capture_modal_sends_the_prefix_key_to_the_pane() {
    use zz_terminal::{KeyAction, KeyInput, Modifiers};

    use crate::{ChromeKeymap, ChromeProfile, Disposition, Effect, InputEvent, InputRouter};

    let key = |ch: char, control: bool, platform: bool| KeyInput {
        action: KeyAction::Press,
        key: zz_terminal::KeyCode::Character(ch),
        modifiers: Modifiers::new(false, control, false, platform),
        text: None,
        unshifted_codepoint: None,
    };
    let prefix = key('b', true, false);
    let palette = key('k', false, true);
    let claimed = crate::PrefixView {
        armed: false,
        claimed: true,
    };

    let (core, tiled, modal) = modal_core(true);
    assert_eq!(core.modal_capture(), Some(modal));
    let mut router = InputRouter::new(ChromeKeymap::for_profile(ChromeProfile::DesktopApple));
    router.event(InputEvent::ActivatePane(
        tiled,
        crate::SurfaceKind::Terminal,
    ));
    router.drain_effects();
    router.set_capture(core.modal_capture());
    for input in [&prefix, &palette] {
        assert_eq!(router.key(input, claimed), Disposition::Consumed);
        assert_eq!(
            router.drain_effects(),
            vec![Effect::ForwardKey {
                pane: modal,
                input: input.clone(),
            }]
        );
    }

    let (core, _, _) = modal_core(false);
    assert_eq!(core.modal_capture(), None);
    router.set_capture(core.modal_capture());
    assert_eq!(router.key(&prefix, claimed), Disposition::Consumed);
    assert_eq!(
        router.drain_effects(),
        vec![Effect::ForwardKey {
            pane: tiled,
            input: prefix.clone(),
        }]
    );
    assert_eq!(router.key(&palette, claimed), Disposition::Consumed);
    assert!(matches!(
        router.drain_effects().as_slice(),
        [Effect::Chrome(_)]
    ));
}

#[test]
fn a_retired_popup_event_changes_nothing() {
    let mut core = ClientCore::new();
    core.handle_message(event(EventPayload::RetiredPopup { state: None }));
    assert!(drain(&mut core).is_empty());
}

#[test]
fn menu_and_confirm_descriptors_reduce_and_clear_with_attachment_state() {
    let menu = MenuState {
        left: 2,
        top: 3,
        width: 20,
        height: 3,
        client_columns: 80,
        client_rows: 24,
        cell_width_px: 8,
        cell_height_px: 18,
        title: "menu".to_owned(),
        style: "default".to_owned(),
        selected_style: "default".to_owned(),
        border_style: "default".to_owned(),
        border_lines: zz_protocol::PopupBorderLines::Single,
        items: vec![Some(zz_protocol::MenuItem {
            name: "Item".to_owned(),
            key: Some("i".to_owned()),
            annotation: Some("i".to_owned()),
            enabled: true,
        })],
        selected: Some(0),
        stay_open: false,
        mouse_keys: false,
    };
    let confirm = ConfirmState {
        prompt: "Confirm? ".to_owned(),
        confirm_key: b'y',
        default_yes: false,
        prompt_cursor: zz_protocol::PromptCursor::default(),
    };
    let mut core = ClientCore::new();

    core.handle_message(event(EventPayload::Menu {
        state: Some(menu.clone()),
    }));
    core.handle_message(event(EventPayload::Confirm {
        state: Some(confirm.clone()),
    }));
    assert_eq!(
        drain(&mut core),
        vec![CoreEvent::MenuChanged, CoreEvent::ConfirmChanged]
    );
    assert_eq!(core.menu(), Some(&menu));
    assert_eq!(core.confirm(), Some(&confirm));

    core.handle_message(ProtocolMessage::Attached {
        session: SessionId(0),
        snapshot: snapshot_with(&[]),
        read_only: false,
        client_flags: String::new(),
    });
    assert_eq!(core.menu(), None);
    assert_eq!(core.confirm(), None);
    assert_eq!(
        drain(&mut core),
        vec![
            CoreEvent::Attached {
                session: SessionId(0)
            },
            CoreEvent::SnapshotChanged,
            CoreEvent::MenuChanged,
            CoreEvent::ConfirmChanged,
        ]
    );
}
