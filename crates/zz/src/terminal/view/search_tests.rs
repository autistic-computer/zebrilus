use super::*;

#[ala::test]
fn pane_search_owns_focus_and_updates_unicode_edits_without_submitting(
    cx: &mut ala::TestAppContext,
) {
    cx.update(zz_ui::init);
    let (terminal, cx) = cx.add_window_view(|window, cx| {
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(zz_daemon_client::DaemonError::Thread(
                    "test client".to_owned(),
                )),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        TerminalView::new(PaneId(0), mux, Rc::new(Cell::new(false)), window, cx)
    });
    cx.update(|window, cx| {
        terminal.update(cx, |view, cx| {
            view.begin_search(
                SearchDirection::Backward,
                SearchPromptBehavior::Navigate,
                window,
                cx,
            );
            assert!(view.focus().is_focused(window));
            assert!(!view.focus_handle.is_focused(window));
            view.search_input
                .update(cx, |input, cx| input.insert("界 needle", window, cx));
        });
    });
    cx.run_until_parked();
    terminal.update(cx, |view, _| {
        let query = view.search_query.as_ref().expect("search stays open");
        assert_eq!(query.text, "界 needle");
        assert_eq!(query.direction, SearchDirection::Backward);
    });
    cx.update(|window, cx| {
        terminal.update(cx, |view, cx| {
            view.close_search(window, cx);
            assert!(view.search_query.is_none());
            assert!(view.focus_handle.is_focused(window));
        });
    });
}
