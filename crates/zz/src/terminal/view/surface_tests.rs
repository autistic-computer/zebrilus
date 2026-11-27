use super::*;
use zz_terminal::AppearanceColor;

#[test]
fn terminal_surface_uses_the_live_viewport_background_as_a_tint() {
    let live = zz_terminal::Color::rgb(0x12, 0x34, 0x56);
    assert_eq!(
        terminal_background(live, 1.0),
        appearance_hsla(AppearanceColor::rgba(0x12, 0x34, 0x56, 255))
    );
    assert_eq!(
        terminal_background(live, 0.5),
        appearance_hsla(AppearanceColor::rgba(0x12, 0x34, 0x56, 128))
    );
}

#[ala::test]
fn terminal_geometry_retries_the_same_grid_after_a_new_layout_view(cx: &mut ala::TestAppContext) {
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
        let sent = mux.update(cx, |mux, _| mux.record_input_for_test());
        captured_mux.replace(Some((mux.clone(), sent)));
        TerminalView::new(PaneId(0), mux, Rc::new(Cell::new(false)), window, cx)
    });
    let (mux, sent) = mux_slot.borrow().clone().expect("captured mux");
    let reports = || {
        sent.borrow()
            .iter()
            .filter_map(|message| match message {
                InputMessage::ResizeTerminalV2 {
                    columns,
                    rows,
                    cell_width_px,
                    cell_height_px,
                    layout_generation,
                    ..
                } => Some((
                    (*columns, *rows, *cell_width_px, *cell_height_px),
                    *layout_generation,
                )),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(reports().len(), 1);
    mux.update(cx, |mux, cx| {
        mux.handle_message_for_test(
            zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                sequence: 1,
                payload: zz_protocol::EventPayload::ClientView(zz_protocol::ClientView {
                    layout_generation: 1,
                    ..Default::default()
                }),
            }),
            cx,
        );
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let resized = reports();
    assert_eq!(resized.len(), 2);
    assert_eq!(resized[0].0, resized[1].0);
    assert_eq!(resized[0].1, 0);
    assert_eq!(resized[1].1, 1);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(reports().len(), 2);
    drop(terminal);
}
