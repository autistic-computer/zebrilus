use super::*;

impl BrowserView {
    pub(super) fn open_context_menu(
        &mut self,
        request: &ContextMenuRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.element_pick_active {
            return;
        }
        let Some(bounds) = self.content_bounds else {
            return;
        };
        let position = bounds.origin + pane_offset(request.x, request.y);
        let browser = cx.entity().downgrade();
        let action_context = self.focus_handle.clone();
        let misspelled_word = request.misspelled_word.clone();
        let dictionary_suggestions = request.dictionary_suggestions.clone();
        let link = request.link_url.clone();
        let image_src = request.is_image.then(|| request.src_url.clone()).flatten();
        let has_selection = request.selection_text.is_some();
        let editable = request.editable;
        let flags = request.edit_flags;
        let can_go_back = self.can_go_back;
        let can_go_forward = self.can_go_forward;
        let (inspect_x, inspect_y) = (request.x, request.y);

        let menu = PopupMenu::build(window, cx, move |mut menu, _, _| {
            menu = menu.action_context(action_context);
            if let Some(misspelled) = misspelled_word {
                for suggestion in dictionary_suggestions.iter().take(5) {
                    let word = suggestion.clone();
                    menu = menu.item(browser_menu_item_owned(
                        suggestion.to_string(),
                        true,
                        &browser,
                        move |view, cx| view.replace_misspelling(&word, cx),
                    ));
                }
                let word = misspelled.clone();
                menu = menu
                    .item(browser_menu_item(
                        "Add to dictionary",
                        true,
                        &browser,
                        move |view, cx| view.add_word_to_dictionary(&word, cx),
                    ))
                    .separator();
            }
            if let Some(link) = link {
                let open_here = link.clone();
                let open_new = link.clone();
                let address = link;
                menu = menu
                    .item(browser_menu_item("Open link", true, &browser, {
                        move |view, cx| view.submit_address(&open_here, cx)
                    }))
                    .item(browser_window_menu_item(
                        "Open link in new tab",
                        true,
                        &browser,
                        move |view, window, cx| view.open_tab(Some(&open_new), true, window, cx),
                    ))
                    .item(
                        PopupMenuItem::new("Copy link address").on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(address.to_string()));
                        }),
                    )
                    .separator();
            }
            if let Some(src) = image_src {
                let open_image = src.clone();
                let copy_image = src;
                menu = menu
                    .item(browser_window_menu_item(
                        "Open image in new tab",
                        true,
                        &browser,
                        move |view, window, cx| view.open_tab(Some(&open_image), true, window, cx),
                    ))
                    .item(
                        PopupMenuItem::new("Copy image address").on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(
                                copy_image.to_string(),
                            ));
                        }),
                    )
                    .separator();
            }
            if editable {
                menu = menu
                    .item(browser_menu_item(
                        "Cut",
                        flags.can_cut,
                        &browser,
                        |view, cx| {
                            view.edit(EditCommand::Cut, cx);
                        },
                    ))
                    .item(browser_menu_item(
                        "Copy",
                        flags.can_copy,
                        &browser,
                        |view, cx| view.edit(EditCommand::Copy, cx),
                    ))
                    .item(browser_menu_item(
                        "Paste",
                        flags.can_paste,
                        &browser,
                        |view, cx| view.edit(EditCommand::Paste, cx),
                    ))
                    .item(browser_menu_item(
                        "Select all",
                        flags.can_select_all,
                        &browser,
                        |view, cx| view.edit(EditCommand::SelectAll, cx),
                    ))
                    .separator();
            } else if has_selection {
                menu = menu
                    .item(browser_menu_item(
                        "Copy",
                        flags.can_copy,
                        &browser,
                        |view, cx| view.edit(EditCommand::Copy, cx),
                    ))
                    .separator();
            }
            menu.item(browser_menu_item(
                "Back",
                can_go_back,
                &browser,
                BrowserView::on_back,
            ))
            .item(browser_menu_item(
                "Forward",
                can_go_forward,
                &browser,
                BrowserView::on_forward,
            ))
            .item(browser_menu_item(
                "Reload",
                true,
                &browser,
                BrowserView::on_reload,
            ))
            .item(browser_menu_item(
                "Hard reload",
                true,
                &browser,
                BrowserView::on_hard_reload,
            ))
            .separator()
            .item(browser_menu_item(
                "Inspect element",
                true,
                &browser,
                move |view, cx| view.inspect_element_at(inspect_x, inspect_y, cx),
            ))
        });

        let subscription = cx.subscribe(&menu, |view, _, _: &DismissEvent, cx| {
            view.context_menu = None;
            cx.notify();
        });
        menu.focus_handle(cx).focus(window, cx);
        self.context_menu = Some(PageContextMenu {
            menu,
            position,
            _subscription: subscription,
        });
    }

    pub(super) fn edit(&mut self, command: EditCommand, cx: &mut Context<Self>) {
        self.controller.update(cx, |controller, cx| {
            controller.edit(self.pane, self.active_tab, command, cx);
        });
    }

    fn inspect_element_at(&mut self, x: i32, y: i32, cx: &mut Context<Self>) {
        self.controller.update(cx, |controller, _| {
            controller.inspect_element_at(self.pane, self.active_tab, x, y);
        });
    }

    fn replace_misspelling(&mut self, word: &str, cx: &mut Context<Self>) {
        self.controller.update(cx, |controller, cx| {
            controller.replace_misspelling(self.pane, self.active_tab, word, cx);
        });
    }

    fn add_word_to_dictionary(&mut self, word: &str, cx: &mut Context<Self>) {
        self.controller.update(cx, |controller, cx| {
            controller.add_word_to_dictionary(self.pane, self.active_tab, word, cx);
        });
    }
}
