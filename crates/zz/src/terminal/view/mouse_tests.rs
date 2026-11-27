use super::*;
use zz_client::scrollback::HistoryRow;

#[test]
fn primary_paste_yields_to_application_mouse_and_rejects_output_views() {
    assert!(should_paste_primary(false, false, false));
    assert!(!should_paste_primary(false, true, false));
    assert!(should_paste_primary(false, true, true));
    assert!(!should_paste_primary(true, false, false));
    assert!(!should_paste_primary(true, true, true));
}

#[test]
fn hovered_uri_presentation_is_bounded_on_character_boundaries() {
    let uri = format!("https://example.com/{}", "界".repeat(300));
    let presented = presented_uri(&uri);
    assert_eq!(presented.chars().count(), 241);
    assert!(presented.ends_with('…'));
    assert!(presented.is_char_boundary(presented.len()));
}

#[test]
fn forwarded_mouse_buttons_keep_matching_releases_on_the_clamped_path() {
    let buttons = mouse_button_bit(MouseButton::Left) | mouse_button_bit(MouseButton::Right);
    assert!(forwarded_mouse_press(buttons, MouseButton::Left));
    assert!(forwarded_mouse_press(buttons, MouseButton::Right));
    assert!(!forwarded_mouse_press(buttons, MouseButton::Middle));
}

#[test]
fn only_a_button_this_pane_pressed_carries_its_motion_and_release() {
    let mut buttons = 0;
    assert!(!owns_pressed_button(buttons, Some(MouseButton::Left)));
    assert!(!forwarded_mouse_press(buttons, MouseButton::Left));
    assert!(owns_pressed_button(buttons, None));

    buttons |= mouse_button_bit(MouseButton::Left);
    assert!(owns_pressed_button(buttons, Some(MouseButton::Left)));
    assert!(!owns_pressed_button(buttons, Some(MouseButton::Right)));
    assert!(owns_pressed_button(buttons, None));

    buttons &= !mouse_button_bit(MouseButton::Left);
    assert!(!owns_pressed_button(buttons, Some(MouseButton::Left)));
}

#[test]
fn visible_scrollbar_strip_is_not_part_of_the_terminal_hit_grid() {
    let surface_bounds = Bounds::new(point(px(10.0), px(20.0)), ala::size(px(650.0), px(460.0)));
    let grid = HitGrid {
        bounds: Bounds::new(surface_bounds.origin, ala::size(px(640.0), px(456.0))),
        surface_bounds,
        columns: 80,
        rows: 24,
        cell_width: px(8.0),
        line_height: px(19.0),
        content_offset: px(0.0),
        scrollbar_gutter: px(16.0),
    };
    let scrollable = ScrollbarState {
        total: 100,
        offset: 0,
        len: 24,
    };
    assert!(scrollbar_strip_hit(
        grid,
        scrollable,
        point(surface_bounds.right() - px(1.0), px(100.0))
    ));
    assert!(!scrollbar_strip_hit(
        grid,
        scrollable,
        point(surface_bounds.right() - px(17.0), px(100.0))
    ));
    assert!(!scrollbar_strip_hit(
        grid,
        ScrollbarState {
            total: 24,
            offset: 0,
            len: 24,
        },
        point(surface_bounds.right() - px(1.0), px(100.0))
    ));
}

#[ala::test]
fn trackpad_scrollback_stays_local_and_syncs_the_daemon_before_a_press(
    cx: &mut ala::TestAppContext,
) {
    cx.update(zz_ui::init);
    let mux_slot = Rc::new(RefCell::new(None));
    let captured_mux = Rc::clone(&mux_slot);
    let (terminal, cx) = cx.add_window_view(move |window, cx| {
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(zz_daemon_client::DaemonError::Thread(
                    "test client".to_owned(),
                )),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        captured_mux.replace(Some(mux.clone()));
        TerminalView::new(PaneId(0), mux, Rc::new(Cell::new(false)), window, cx)
    });
    let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");
    let sent = mux.update(cx, |mux, _| mux.record_input_for_test());
    let grid = Bounds::new(point(px(0.0), px(0.0)), ala::size(px(640.0), px(480.0)));
    let geometry = |view: &mut TerminalView, content_offset, cx: &mut Context<TerminalView>| {
        view.update_geometry(
            GridSize {
                columns: 80,
                rows: 24,
                cell_width_px: 8,
                cell_height_px: 20,
            },
            grid,
            grid,
            px(8.0),
            px(20.0),
            content_offset,
            None,
            None,
            cx,
        );
    };
    let pixels = |y: f32| ScrollWheelEvent {
        position: point(px(20.0), px(40.0)),
        delta: ScrollDelta::Pixels(point(px(0.0), px(y))),
        ..Default::default()
    };
    let scroll_actions = |sent: &RefCell<Vec<InputMessage>>| {
        sent.borrow()
            .iter()
            .filter_map(|message| match message {
                InputMessage::TerminalView { action, .. } => match action {
                    TerminalViewAction::ScrollToOffset(offset) => Some(Some(*offset)),
                    TerminalViewAction::ScrollWheel { .. } => Some(None),
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    cx.update(|window, cx| {
        terminal.update(cx, |view, cx| {
            {
                let mut retained = view.retained.write();
                retained.viewport.scrollbar = ScrollbarState {
                    total: 1024,
                    offset: 1000,
                    len: 24,
                };
                retained.history.rows = (0..200)
                    .map(|revision| HistoryRow {
                        cells: Box::from([]),
                        dictionary: Arc::default(),
                        revision,
                    })
                    .collect();
            }
            geometry(view, px(0.0), cx);
            sent.borrow_mut().clear();

            view.on_scroll(&pixels(5.0), window, cx);
            assert_eq!(view.scroll_pixel_offset(), px(5.0));
            assert_eq!(view.local_scroll_target(), None);
            view.on_scroll(&pixels(20.0), window, cx);
            assert_eq!(view.scroll_pixel_offset(), px(5.0));
            assert_eq!(view.local_scroll_target(), Some(999));
            view.on_scroll(&pixels(-50.0), window, cx);
            assert_eq!(view.scroll_pixel_offset(), px(0.0));
            assert_eq!(view.local_scroll_target(), None);
            assert!(scroll_actions(&sent).is_empty());

            view.on_scroll(&pixels(605.0), window, cx);
            assert_eq!(view.local_scroll_target(), Some(970));
            assert_eq!(view.scroll_pixel_offset(), px(5.0));
            assert_eq!(scroll_actions(&sent), [Some(988)]);

            geometry(view, px(5.0), cx);
            view.on_mouse_down(
                &MouseDownEvent {
                    button: MouseButton::Left,
                    position: point(px(20.0), px(40.0)),
                    click_count: 1,
                    ..Default::default()
                },
                window,
                cx,
            );
            assert_eq!(scroll_actions(&sent), [Some(988), Some(970)]);
            let Some(InputMessage::TerminalView {
                action: TerminalViewAction::Mouse(press),
                ..
            }) = sent.borrow().last().cloned()
            else {
                panic!("the press follows the flushed offset");
            };
            assert_eq!(press.cell.row, 1);
            assert_eq!(view.local_scroll_target(), Some(970));

            sent.borrow_mut().clear();
            view.on_scroll(
                &ScrollWheelEvent {
                    position: point(px(20.0), px(40.0)),
                    delta: ScrollDelta::Lines(point(0.0, 1.0)),
                    ..Default::default()
                },
                window,
                cx,
            );
            assert_eq!(view.scroll_pixel_offset(), px(0.0));
            assert_eq!(scroll_actions(&sent), [Some(970), None]);

            sent.borrow_mut().clear();
            view.retained.write().viewport.mouse_tracking = true;
            view.on_scroll(&pixels(25.0), window, cx);
            assert_eq!(view.scroll_pixel_offset(), px(0.0));
            assert_eq!(scroll_actions(&sent), [None]);
        });
    });
}
