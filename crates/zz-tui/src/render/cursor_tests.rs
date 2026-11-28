use std::sync::Arc;

use super::test_support::{
    CLIPPED, attach_one_pane, block_model, block_status, menu_state, tiled_under_float,
    waiting_pane_model,
};
use super::*;
use crate::state::ClientMessage;
use zz_terminal::{Cursor, CursorStyle, SessionStatus};

#[test]
fn a_paint_that_only_puts_the_cursor_back_writes_nothing() {
    let model = waiting_pane_model();
    let (send, receive) = std::sync::mpsc::channel();
    let mut renderer = Renderer::with_sink(Box::new(move |bytes| {
        send.send(bytes.to_vec()).unwrap();
        Ok(())
    }));
    let wait = std::time::Duration::from_secs(5);
    renderer.paint(&model, true).unwrap();
    let first = receive.recv_timeout(wait).unwrap();
    renderer.paint(&model, false).unwrap();
    assert!(
        receive
            .recv_timeout(std::time::Duration::from_millis(100))
            .is_err(),
        "the cursor is already where {first:?} left it"
    );
    renderer.paint(&model, true).unwrap();
    assert!(
        receive.recv_timeout(wait).is_ok(),
        "a forced paint is always written"
    );
    renderer.invalidate();
    renderer.paint(&model, false).unwrap();
    assert!(
        receive.recv_timeout(wait).is_ok(),
        "an invalidated screen repaints"
    );
}

#[test]
fn sidebar_refocus_hides_the_cursor_without_changing_visibility_or_geometry() {
    let mut model = block_model(120, 30);
    let pane = PaneId(1);
    attach_one_pane(&mut model, pane);
    let mut viewport = TerminalViewport::blank(120, 29, SessionStatus::Running);
    viewport.cursor = Some(Cursor::new(
        0,
        0,
        true,
        false,
        false,
        CursorStyle::Bar,
        viewport.foreground,
    ));
    model.viewports.insert(pane, viewport);
    let mut renderer = Renderer::new();

    assert!(model.focus_sidebar());
    assert_eq!(model.status_area(), (29, 91));
    renderer.place_active_cursor(&model);
    assert_eq!(renderer.output, b"\x1b[?25l");

    model.focus_active_pane();
    assert!(model.sidebar_visible());
    renderer.output.clear();
    renderer.place_active_cursor(&model);
    assert!(renderer.output.ends_with(b"\x1b[?25h"));

    assert!(!model.focus_sidebar());
    assert!(model.sidebar_focused());
    assert!(model.sidebar_visible());
    assert_eq!(model.status_area(), (29, 91));
    renderer.output.clear();
    renderer.place_active_cursor(&model);
    assert_eq!(renderer.output, b"\x1b[?25l");

    assert!(model.hide_sidebar());
    assert!(!model.sidebar_focused());
    assert!(!model.sidebar_visible());
    assert_eq!(model.status_area(), (0, 120));
    renderer.output.clear();
    renderer.place_active_cursor(&model);
    assert!(renderer.output.ends_with(b"\x1b[?25h"));
}

#[test]
fn menu_hides_the_workspace_cursor() {
    let mut model = block_model(40, 12);
    model.menu = Some(menu_state());
    let mut renderer = Renderer::new();
    renderer.place_active_cursor(&model);
    assert_eq!(renderer.output, b"\x1b[?25l");
}

#[test]
fn a_status_row_message_takes_the_cursor_and_a_prompt_keeps_it() {
    let mut model = block_model(40, 10);
    model.set_status(block_status(vec!["ROW"], false));
    assert!(!client_message_hides_the_cursor(&model));
    model.client_message = Some(ClientMessage::local("a message"));
    assert!(client_message_hides_the_cursor(&model));

    let mut sidebar_model = block_model(120, 10);
    sidebar_model.focus_sidebar();
    assert!(sidebar_model.sidebar_visible());
    sidebar_model.set_status(block_status(vec!["ROW"], false));
    sidebar_model.client_message = Some(ClientMessage::local("a sidebar message"));
    assert!(!client_message_hides_the_cursor(&sidebar_model));

    model.command_prompt = Some(zz_protocol::CommandPromptState {
        prompt: ":".to_owned(),
        input: "list".to_owned(),
        cursor: 4,
        kind: zz_protocol::CommandPromptKind::Command,
        history: Vec::new(),
        prompt_type: zz_protocol::CommandPromptType::Command,
        mode: zz_protocol::CommandPromptMode::Text,
        no_freeze: false,
        pane: None,
        command_mode: false,
        prompt_cursor: zz_protocol::PromptCursor::default(),
    });
    let mut renderer = Renderer::new();
    renderer.place_active_cursor(&model);
    let output = String::from_utf8(renderer.output).unwrap();
    assert!(output.ends_with("\x1b[?25h"), "{output:?}");
}

#[test]
fn a_client_prompt_sends_its_cursor_and_takes_it_back_when_it_closes() {
    let mut model = block_model(40, 10);
    model.set_status(block_status(vec!["ROW"], false));
    let prompt = zz_protocol::CommandPromptState {
        prompt: ":".to_owned(),
        input: "abc".to_owned(),
        cursor: 3,
        kind: zz_protocol::CommandPromptKind::Command,
        history: Vec::new(),
        prompt_type: zz_protocol::CommandPromptType::Command,
        mode: zz_protocol::CommandPromptMode::Text,
        no_freeze: false,
        pane: None,
        command_mode: false,
        prompt_cursor: zz_protocol::PromptCursor {
            style: 6,
            colour: Some(TmuxColour::Basic(1)),
        },
    };
    let mut renderer = Renderer::new();
    let written = |renderer: &mut Renderer, model: &Model| {
        renderer.write_terminal_cursor(model, true, true);
        String::from_utf8(std::mem::take(&mut renderer.output)).unwrap()
    };
    assert_eq!(written(&mut renderer, &model), "");

    model.command_prompt = Some(prompt.clone());
    assert_eq!(
        written(&mut renderer, &model),
        "\x1b]12;rgb:80/00/00\x07\x1b[6 q"
    );
    assert_eq!(written(&mut renderer, &model), "");

    model.command_prompt = Some(zz_protocol::CommandPromptState {
        command_mode: true,
        prompt_cursor: zz_protocol::PromptCursor {
            style: 3,
            colour: Some(TmuxColour::Theme(6)),
        },
        ..prompt.clone()
    });
    let theme = model.status.theme.0[6];
    let TmuxColour::Rgb(rgb) = theme else {
        panic!("the default theme slot is rgb: {theme:?}");
    };
    assert_eq!(
        written(&mut renderer, &model),
        format!(
            "\x1b]12;rgb:{:02x}/{:02x}/{:02x}\x07\x1b[3 q",
            rgb >> 16 & 0xff,
            rgb >> 8 & 0xff,
            rgb & 0xff
        )
    );

    model.command_prompt = Some(zz_protocol::CommandPromptState {
        pane: Some(PaneId(1)),
        ..prompt.clone()
    });
    assert_eq!(written(&mut renderer, &model), "\x1b]112\x07\x1b[2 q");

    model.command_prompt = None;
    model.confirm = Some(zz_protocol::ConfirmState {
        prompt: "sure? ".to_owned(),
        confirm_key: b'y',
        default_yes: false,
        prompt_cursor: zz_protocol::PromptCursor {
            style: 1,
            colour: None,
        },
    });
    assert_eq!(written(&mut renderer, &model), "\x1b[1 q");
    model.confirm = None;
    assert_eq!(written(&mut renderer, &model), "\x1b[2 q");

    model.command_prompt = Some(prompt);
    renderer.write_terminal_cursor(&model, false, false);
    assert!(renderer.output.is_empty());
    assert_eq!(
        written(&mut renderer, &model),
        "\x1b]12;rgb:80/00/00\x07\x1b[6 q",
        "a cursor the terminal could not take yet is sent once it can"
    );
}

#[test]
fn a_cursor_style_is_restored_at_teardown_even_when_its_reset_was_only_built() {
    let mut model = block_model(40, 10);
    model.set_status(block_status(vec!["ROW"], false));
    model.command_prompt = Some(zz_protocol::CommandPromptState {
        prompt: ":".to_owned(),
        input: String::new(),
        cursor: 0,
        kind: zz_protocol::CommandPromptKind::Command,
        history: Vec::new(),
        prompt_type: zz_protocol::CommandPromptType::Command,
        mode: zz_protocol::CommandPromptMode::Text,
        no_freeze: false,
        pane: None,
        command_mode: false,
        prompt_cursor: zz_protocol::PromptCursor {
            style: 6,
            colour: None,
        },
    });
    let mut renderer = Renderer::new();
    renderer.write_terminal_cursor(&model, true, true);
    model.command_prompt = None;
    renderer.write_terminal_cursor(&model, true, true);
    assert!(String::from_utf8_lossy(&renderer.output).ends_with("\x1b[2 q"));
    assert!(
        crate::tty::cursor_style_needs_restore(),
        "a reset that may never reach the terminal leaves the teardown restore armed"
    );
}

#[test]
fn a_dropped_paint_sends_the_prompt_cursor_again() {
    let mut model = block_model(40, 10);
    model.set_status(block_status(vec!["ROW"], false));
    model.command_prompt = Some(zz_protocol::CommandPromptState {
        prompt: ":".to_owned(),
        input: String::new(),
        cursor: 0,
        kind: zz_protocol::CommandPromptKind::Command,
        history: Vec::new(),
        prompt_type: zz_protocol::CommandPromptType::Command,
        mode: zz_protocol::CommandPromptMode::Text,
        no_freeze: false,
        pane: None,
        command_mode: false,
        prompt_cursor: zz_protocol::PromptCursor {
            style: 6,
            colour: Some(TmuxColour::Basic(1)),
        },
    });
    let sent = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut renderer = Renderer::with_sink(Box::new({
        let sent = Arc::clone(&sent);
        move |bytes| {
            sent.lock().unwrap().extend_from_slice(bytes);
            Ok(())
        }
    }));
    let paint = |renderer: &mut Renderer, model: &Model| {
        renderer.output.extend_from_slice(PAINT_BEGIN);
        renderer.write_terminal_cursor(model, true, true);
        renderer.output.extend_from_slice(PAINT_END);
        renderer.flush_output().unwrap();
        String::from_utf8(std::mem::take(&mut *sent.lock().unwrap())).unwrap()
    };

    renderer.writer.borrow_mut().pause(true);
    assert_eq!(paint(&mut renderer, &model), "");
    renderer.writer.borrow_mut().pause(false);
    let opened = paint(&mut renderer, &model);
    assert!(opened.contains("\x1b[6 q"), "{opened:?}");
    assert!(opened.contains("\x1b]12;rgb:80/00/00\x07"), "{opened:?}");

    model.command_prompt = None;
    renderer.writer.borrow_mut().pause(true);
    assert_eq!(paint(&mut renderer, &model), "");
    renderer.writer.borrow_mut().pause(false);
    let closed = paint(&mut renderer, &model);
    assert!(closed.contains("\x1b[2 q"), "{closed:?}");
    assert!(closed.contains("\x1b]112\x07"), "{closed:?}");
}

#[test]
fn a_pane_mode_cursor_under_a_float_is_hidden() {
    let mode = zz_protocol::PaneMode::Switch {
        rows: vec!["cli".to_owned()],
        selected: 0,
        offset: 0,
        selection_style: "noattr,bg=themeyellow,fg=themeblack".to_owned(),
        prompt: "(search) ".to_owned(),
        prompt_style: "bg=themeyellow,fg=themeblack".to_owned(),
        prompt_cursor: 9,
        matches: Vec::new(),
        match_style: String::new(),
        prompt_shape: zz_protocol::PromptCursor::default(),
    };
    let open = zz_protocol::FloatingPaneSnapshot {
        xoff: 30,
        yoff: 8,
        sx: 4,
        sy: 1,
        ..CLIPPED
    };
    let model = tiled_under_float(open, Some(mode.clone()));
    let content = model.pane_rect(PaneId(7)).unwrap().content();
    let cursor = pane_mode::surface(&mode, content, &model.status.theme).cursor;
    let mut renderer = Renderer::new();
    renderer.place_pane_cursor(&model);
    assert_eq!(
        String::from_utf8_lossy(&renderer.output),
        "\u{1b}[12;10H\u{1b}[?25h"
    );

    let covering = zz_protocol::FloatingPaneSnapshot {
        xoff: i32::from(cursor.0) - 1,
        yoff: i32::from(cursor.1),
        sx: 4,
        sy: 1,
        ..CLIPPED
    };
    let model = tiled_under_float(covering, Some(mode));
    let mut renderer = Renderer::new();
    renderer.place_pane_cursor(&model);
    let output = String::from_utf8_lossy(&renderer.output).into_owned();
    assert_eq!(output, "\u{1b}[12;10H\u{1b}[?25l");
}

#[test]
fn a_command_output_on_another_pane_owns_neither_the_prompt_nor_the_cursor() {
    let mut model = block_model(40, 8);
    attach_one_pane(&mut model, PaneId(1));
    let viewport = TerminalViewport::blank(40, 6, SessionStatus::Running);
    model.command_output = Some((PaneId(2), viewport.clone()));
    model.command_output_search = Some(SearchQuery::literal("elsewhere"));
    assert_eq!(model.command_output_focus(), None);
    assert!(status_overlay(&model, 40).is_none());
    model.command_output = Some((PaneId(1), viewport));
    assert_eq!(model.command_output_focus(), Some(PaneId(1)));
    assert!(matches!(
        status_overlay(&model, 40),
        Some(StatusOverlay { .. })
    ));
}
