use std::sync::Arc;

use super::test_support::block_model;
use super::*;

#[test]
fn the_chooser_key_gutter_sizes_once_and_pads_keyless_rows() {
    assert_eq!(chooser_key_column(["0", "M-a", ""].into_iter()), 6);
    assert_eq!(chooser_key_column(["", ""].into_iter()), 0);
    assert_eq!(chooser_key_cell("0", 6), "(0)   ");
    assert_eq!(chooser_key_cell("M-a", 6), "(M-a) ");
    assert_eq!(chooser_key_cell("", 6), "      ");
    assert_eq!(chooser_key_cell("", 0), "");
    assert_eq!(chooser_key_cell("0", 0), "");
}

#[test]
fn forced_repaints_preserve_a_chooser_until_its_presentation_arrives() {
    let mut model = block_model(60, 12);
    Arc::make_mut(&mut model.status).rows = vec!["CHOOSER-STATUS".to_owned()];
    Arc::make_mut(&mut model.status).customized = true;
    model.choose_tree = Some(zz_protocol::ChooseTreeState {
        items: ["alpha", "beta"]
            .into_iter()
            .enumerate()
            .map(|(index, name)| zz_protocol::ChooseTreeItem {
                label: name.to_owned(),
                detail: String::new(),
                target: zz_protocol::ChooseTreeTarget::Session(zz_protocol::SessionId(
                    index as u64,
                )),
                depth: 0,
                flags: 0,
                pane_kind: None,
                key: index.to_string(),
                text: String::new(),
            })
            .collect(),
        search: None,
        selected: 1,
        kind: zz_protocol::ChooseTreeKind::Windows,
        filter_no_matches: false,
        prompt: String::new(),
        help: false,
    });
    let presentation = zz_protocol::ChooserPresentation {
        selected: 1,
        rows: ["alpha", "beta"]
            .into_iter()
            .map(|name| zz_protocol::ChooserRow {
                name: name.to_owned(),
                ..Default::default()
            })
            .collect(),
        sort: "index".to_owned(),
        view: "preview".to_owned(),
        filter: false,
        selection_style: String::new(),
        border_style: String::new(),
        prompt_style: String::new(),
        preview_size: zz_protocol::ChooserPreviewSize::Off,
        preview: None,
        prompt_cursor: zz_protocol::PromptCursor::default(),
        prompt_column: 0,
        prompt_line: String::new(),
    };
    let (send, receive) = std::sync::mpsc::channel();
    let mut renderer = Renderer::with_sink(Box::new(move |bytes| {
        send.send(bytes.to_vec()).unwrap();
        Ok(())
    }));
    let read_paint = || {
        String::from_utf8(
            receive
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
        )
        .unwrap()
    };
    let painted = "\u{1b}[?2026h\u{1b}[?25l\u{1b}[1;1H\u{1b}[0m\u{1b}[38;2;179;179;179m\u{1b}[49m(0) \u{1b}[0m\u{1b}[39m\u{1b}[49malpha\u{1b}[0m\u{1b}[38;2;179;179;179m\u{1b}[49m:\u{1b}[0m\u{1b}[K\u{1b}[2;1H\u{1b}[0m\u{1b}[39m\u{1b}[49m(1) beta:\u{1b}[0m\u{1b}[K\u{1b}[3;1H\u{1b}[0m\u{1b}[K\u{1b}[4;1H\u{1b}[0m\u{1b}[K\u{1b}[5;1H\u{1b}[0m\u{1b}[K\u{1b}[6;1H\u{1b}[0m\u{1b}[K\u{1b}[7;1H\u{1b}[0m\u{1b}[K\u{1b}[8;1H\u{1b}[0m\u{1b}[K\u{1b}[9;1H\u{1b}[0m\u{1b}[K\u{1b}[10;1H\u{1b}[0m\u{1b}[K\u{1b}[11;1H\u{1b}[0m\u{1b}[K\u{1b}[12;1H\u{1b}[0m\u{1b}[39m\u{1b}[49mCHOOSER-STATUS                                              \u{1b}[0m\u{1b}[2;1H\u{1b}[?25l\u{1b}[?2026l";
    let mut completes = Vec::new();
    for missing in [false, true] {
        model.chooser_presentation = Some(presentation.clone());
        renderer.paint(&model, true).unwrap();
        completes.push(read_paint());

        if missing {
            model.chooser_presentation = None;
        } else {
            model.chooser_presentation.as_mut().unwrap().selected = 0;
        }
        renderer.paint(&model, true).unwrap();
        assert_eq!(
            read_paint(),
            "\u{1b}[?2026h\u{1b}[?25l\u{1b}[2;1H\u{1b}[?25l\u{1b}[?2026l",
            "missing presentation: {missing}"
        );
    }
    assert_eq!(completes, [painted, painted]);
    model.chooser_presentation = Some(presentation);
    renderer.paint(&model, true).unwrap();
    assert_eq!(read_paint(), painted);
    model.choose_tree = None;
    model.chooser_presentation = None;
    renderer.paint(&model, true).unwrap();
    assert_eq!(
        read_paint(),
        "\u{1b}[?2026h\u{1b}[?25l\u{1b}[0m\u{1b}[2J\u{1b}[48;2;16;19;24m\u{1b}[1;1H\u{1b}[60X\u{1b}[2;1H\u{1b}[60X\u{1b}[3;1H\u{1b}[60X\u{1b}[4;1H\u{1b}[60X\u{1b}[5;1H\u{1b}[60X\u{1b}[6;1H\u{1b}[60X\u{1b}[7;1H\u{1b}[60X\u{1b}[8;1H\u{1b}[60X\u{1b}[9;1H\u{1b}[60X\u{1b}[10;1H\u{1b}[60X\u{1b}[11;1H\u{1b}[60X\u{1b}[0m\u{1b}[12;1H\u{1b}[0m\u{1b}[39m\u{1b}[49mCHOOSER-STATUS                                              \u{1b}[0m\u{1b}[?25l\u{1b}[?2026l"
    );
}

#[test]
fn the_chooser_paints_every_row_key_in_its_gutter() {
    let mut model = block_model(60, 12);
    model.choose_tree = Some(zz_protocol::ChooseTreeState {
        items: vec![
            zz_protocol::ChooseTreeItem {
                label: "alpha".to_owned(),
                detail: "1 window".to_owned(),
                target: zz_protocol::ChooseTreeTarget::Session(zz_protocol::SessionId(1)),
                depth: 0,
                flags: 0,
                pane_kind: None,
                key: "0".to_owned(),
                text: String::new(),
            },
            zz_protocol::ChooseTreeItem {
                label: "beta".to_owned(),
                detail: "1 window".to_owned(),
                target: zz_protocol::ChooseTreeTarget::Session(zz_protocol::SessionId(2)),
                depth: 0,
                flags: 0,
                pane_kind: None,
                key: "M-a".to_owned(),
                text: String::new(),
            },
            zz_protocol::ChooseTreeItem {
                label: "gamma".to_owned(),
                detail: "1 window".to_owned(),
                target: zz_protocol::ChooseTreeTarget::Session(zz_protocol::SessionId(3)),
                depth: 0,
                flags: 0,
                pane_kind: None,
                key: String::new(),
                text: String::new(),
            },
        ],
        search: None,
        selected: 0,
        kind: zz_protocol::ChooseTreeKind::Windows,
        filter_no_matches: false,
        prompt: String::new(),
        help: false,
    });
    let mut renderer = Renderer::new();
    renderer.paint_chooser(&model);
    let output = String::from_utf8(renderer.output).unwrap();

    assert_eq!(
        output,
        "\u{1b}[0;48;2;16;19;24m\u{1b}[2J\u{1b}[1;1H\u{1b}[0;38;2;16;19;24;48;2;77;163;235mChoose window                                               \u{1b}[0m\u{1b}[2;1H\u{1b}[0;38;2;216;222;233;48;2;16;19;24m> (0)   alpha  1 window                                     \u{1b}[0m\u{1b}[3;1H\u{1b}[0;38;2;216;222;233;48;2;16;19;24m  (M-a) beta  1 window                                      \u{1b}[0m\u{1b}[4;1H\u{1b}[0;38;2;216;222;233;48;2;16;19;24m        gamma  1 window                                     \u{1b}[0m\u{1b}[12;1H\u{1b}[0;38;2;16;19;24;48;2;216;222;233mEnter select  / search  Esc close                           \u{1b}[0m"
    );
}

#[test]
fn a_row_format_takes_over_the_row_the_chooser_would_have_composed() {
    let mut model = block_model(60, 12);
    model.choose_tree = Some(zz_protocol::ChooseTreeState {
        items: vec![zz_protocol::ChooseTreeItem {
            label: "alpha".to_owned(),
            detail: "1 window".to_owned(),
            target: zz_protocol::ChooseTreeTarget::Session(zz_protocol::SessionId(1)),
            depth: 0,
            flags: 0,
            pane_kind: None,
            key: "0".to_owned(),
            text: "ZZTREE<%3>".to_owned(),
        }],
        search: None,
        selected: 0,
        kind: zz_protocol::ChooseTreeKind::Windows,
        filter_no_matches: false,
        prompt: String::new(),
        help: false,
    });
    let mut renderer = Renderer::new();
    renderer.paint_chooser(&model);
    let output = String::from_utf8(renderer.output).unwrap();
    assert!(output.contains("(0) ZZTREE<%3>"), "{output}");
    assert!(!output.contains("alpha"), "{output}");
    assert!(!output.contains("1 window"), "{output}");

    let mut model = block_model(60, 12);
    model.choose_buffer = Some(zz_protocol::ChooseBufferState {
        items: vec![zz_protocol::ChooseBufferItem {
            name: "buffer0".to_owned(),
            preview: "hello".to_owned(),
            size_bytes: 5,
            created_unix_seconds: 0,
            key: "0".to_owned(),
            text: "ZZBUF<buffer0>".to_owned(),
            tagged: false,
        }],
        search: None,
        selected: 0,
        filter_no_matches: false,
        help: false,
        prompt: String::new(),
    });
    let mut renderer = Renderer::new();
    renderer.paint_chooser(&model);
    let output = String::from_utf8(renderer.output).unwrap();
    assert!(output.contains("(0) ZZBUF<buffer0>"), "{output}");
    assert!(!output.contains("5 bytes"), "{output}");
    assert!(!output.contains("hello"), "{output}");
}

#[test]
fn chooser_fallback_status_keeps_fully_keyless_rows_selectable_without_a_gutter() {
    let mut model = block_model(60, 12);
    model.choose_buffer = Some(zz_protocol::ChooseBufferState {
        items: vec![zz_protocol::ChooseBufferItem {
            name: "keyless".to_owned(),
            preview: "fallback row".to_owned(),
            size_bytes: 1,
            created_unix_seconds: 0,
            key: String::new(),
            text: String::new(),
            tagged: false,
        }],
        search: None,
        selected: 0,
        filter_no_matches: true,
        help: false,
        prompt: String::new(),
    });
    let mut renderer = Renderer::new();
    renderer.paint_chooser(&model);
    let output = String::from_utf8(renderer.output).unwrap();

    assert!(output.contains("filter: no matches"), "{output}");
    assert!(output.contains("> keyless  1 bytes"), "{output}");
    assert!(!output.contains(">      keyless"), "{output}");
}
