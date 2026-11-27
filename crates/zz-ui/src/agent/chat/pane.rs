use super::*;

impl ChatPane {
    pub fn markdown(&mut self, refs: ChatRefs) -> Rc<ChatMarkdown> {
        self.markdown_showing(refs, None, None)
    }

    pub(super) fn markdown_showing(
        &mut self,
        refs: ChatRefs,
        html: Option<crate::agent::HtmlBlocks>,
        ui: Option<UiBlocks>,
    ) -> Rc<ChatMarkdown> {
        match &self.markdown {
            Some(markdown) if markdown.reads(&refs, html, ui) => Rc::clone(markdown),
            _ => {
                let markdown = Rc::new(ChatMarkdown::showing(refs, html, ui));
                self.markdown = Some(Rc::clone(&markdown));
                markdown
            }
        }
    }

    pub fn click(&mut self, click: &ChatClick) {
        match click {
            ChatClick::Turn(turn) => toggle(&mut self.open.turns, *turn),
            ChatClick::Run(item) => toggle(&mut self.open.runs, item.clone()),
            ChatClick::Agent(item) => {
                self.open.agent = (self.open.agent.as_ref() != Some(item)).then(|| item.clone());
                return;
            }
            ChatClick::CloseAgent => {
                self.open.agent = None;
                return;
            }
            ChatClick::Fork(_) | ChatClick::Hold(_) | ChatClick::Child(_) => return,
            ChatClick::Unfold(item) => {
                if !self.open.unfolded.remove(item) {
                    self.open.unfolded.insert(item.clone());
                }
            }
            ChatClick::Tall(item, tall) => {
                if *tall {
                    self.open.tall.insert(item.clone());
                } else {
                    self.open.tall.remove(item);
                }
                return;
            }
            ChatClick::Expand(item) => {
                self.open.expanded.insert(item.clone());
            }
            ChatClick::Collapse(item, shrink) => {
                self.open.expanded.remove(item);
                self.list.pause_following_tail();
                self.list.scroll_by(-*shrink);
                return;
            }
        }
        if self.list.is_following_tail() {
            self.list.hold_here();
            self.hold = Hold::Pinned;
        }
    }

    pub fn sync(&mut self, view: &ChatView) {
        let session = match &view.session {
            SessionView::Ready { session } => Some(session.clone()),
            _ => view.last_session.clone(),
        };
        if session != self.session {
            let open = std::mem::take(&mut self.open);
            self.retired
                .extend(open.images.into_values().flat_map(|images| images.to_vec()));
            self.session = session;
            self.rows.clear();
            self.keys.clear();
            self.list.reset(0);
            self.hold = Hold::Idle;
            self.list.set_follow_mode(FollowMode::Tail);
        }
        self.last_turn = view.turns.last().map(|turn| turn.id);
        self.open
            .turns
            .retain(|turn| view.turns.iter().any(|held| held.id == *turn));
        self.open.runs.retain(|id| view.item(id).is_some());
        if self
            .open
            .agent
            .as_ref()
            .is_some_and(|id| view.item(id).is_none())
        {
            self.open.agent = None;
        }
        self.retired.extend(
            self.open
                .images
                .extract_if(|id, _| view.item(id).is_none_or(|item| item.body.images.is_empty()))
                .flat_map(|(_, images)| images.to_vec()),
        );
        for item in view.items.values() {
            if !item.body.images.is_empty() && !self.open.images.contains_key(&item.id) {
                self.open
                    .images
                    .insert(item.id.clone(), decode_images(&item.body.images));
            }
        }
        self.sync_texts(view);
        self.sync_ui(view);
        self.sync_rows(view);
    }

    fn sync_texts(&mut self, view: &ChatView) {
        let texts = &mut self.open.texts;
        let previews = &mut self.open.previews;
        let expanded = &mut self.open.expanded;
        let ui = &mut self.open.ui;
        texts.retain(|id, _| view.item(id).is_some());
        previews.retain(|id, _| texts.contains_key(id));
        ui.retain(|id, _| texts.contains_key(id));
        self.open.sent.retain(|id, _| texts.contains_key(id));
        for item in view.items.values() {
            if !matches!(item.kind, ItemKind::User | ItemKind::Agent) {
                continue;
            }
            let fresh = texts.get(&item.id).is_some_and(|held| {
                held.len() == item.body.text.len()
                    && (!item.status.is_open() || held.as_ref() == item.body.text)
            });
            if fresh {
                continue;
            }
            texts.insert(item.id.clone(), item.body.text.clone().into());
            if item.kind == ItemKind::Agent
                && item.parent.is_none()
                && crate::agent::ui::has_block(&item.body.text)
            {
                ui.entry(item.id.clone()).or_default();
            } else {
                ui.remove(&item.id);
            }
            match preview(&item.body.text).filter(|_| item.kind == ItemKind::Agent) {
                Some(cut) => {
                    previews.insert(item.id.clone(), cut.into());
                }
                None => {
                    previews.remove(&item.id);
                }
            }
        }
        expanded.retain(|id| previews.contains_key(id));
    }

    fn sync_ui(&mut self, view: &ChatView) {
        let open = &mut self.open;
        if open.ui.is_empty() {
            return;
        }
        let mut order = Vec::new();
        let mut reply = None;
        for item in view
            .turns
            .iter()
            .flat_map(|turn| &turn.items)
            .filter_map(|id| view.item(id))
        {
            if item.kind == ItemKind::User {
                if !open.sent.contains_key(&item.id)
                    && let Some(sent) = crate::agent::ui::sent(&item.body.text)
                {
                    open.sent.insert(item.id.clone(), UiSent::new(&sent, reply));
                }
                order.push(item);
            } else if open.ui.contains_key(&item.id) {
                reply = Some(item.body.text.as_str());
                order.push(item);
            }
        }
        let queued = view.queue.first().map(|prompt| prompt.text.as_str());
        let mut after = queued.map(crate::agent::ui::pressed);
        for item in order.into_iter().rev() {
            if item.kind == ItemKind::User {
                after = Some(open.sent.get(&item.id).and_then(|sent| sent.event.clone()));
                continue;
            }
            let look = UiLook {
                held: after.is_some(),
                pressed: after.clone().flatten(),
                open: item.status.is_open() && crate::agent::ui::tail_open(&item.body.text),
            };
            if open.ui.get(&item.id) != Some(&look) {
                open.ui.insert(item.id.clone(), look);
            }
        }
    }

    fn sync_rows(&mut self, view: &ChatView) {
        self.fold = chat_rows::child_fold(view);
        let rows = thread_rows(view, &self.fold);
        let keys = rows.iter().map(|row| row.key(view)).collect::<Vec<_>>();
        let kept = self
            .keys
            .iter()
            .zip(&keys)
            .take_while(|(held, key)| held == key)
            .count();
        if kept < self.keys.len() || kept < keys.len() {
            self.list.splice(kept..self.keys.len(), keys.len() - kept);
        }
        self.rows = rows;
        self.keys = keys;
        self.background_live = chat_rows::background_live(view);
    }

    pub fn release(&mut self, window: &mut Window, cx: &mut App) {
        *self.shown.borrow_mut() = self
            .open
            .images
            .values()
            .flat_map(|images| images.iter().map(|image| image.id))
            .collect();
        if !self.listed {
            self.listed = true;
            cx.default_global::<ShownImages>()
                .0
                .push(Rc::downgrade(&self.shown));
        }
        self.free(Some(window), cx);
    }

    pub fn close(&mut self, window: Option<&mut Window>, cx: &mut App) {
        let open = std::mem::take(&mut self.open.images);
        self.retired
            .extend(open.into_values().flat_map(|images| images.to_vec()));
        self.shown.borrow_mut().clear();
        self.free(window, cx);
    }

    pub fn close_on_release<T: 'static>(
        window: &Window,
        cx: &mut Context<T>,
        chat: fn(&mut T) -> &mut ChatPane,
    ) -> Subscription {
        let handle = window.window_handle();
        cx.on_release(move |owner, cx| {
            let chat = chat(owner);
            if handle
                .update(cx, |_, window, cx| chat.close(Some(window), cx))
                .is_err()
            {
                chat.close(None, cx);
            }
        })
    }

    fn free(&mut self, mut window: Option<&mut Window>, cx: &mut App) {
        if self.retired.is_empty() {
            return;
        }
        let panes = &mut cx.default_global::<ShownImages>().0;
        panes.retain(|pane| pane.strong_count() > 0);
        let mut shown = HashSet::new();
        for pane in panes.iter().filter_map(Weak::upgrade) {
            shown.extend(pane.borrow().iter().copied());
        }
        for image in self.retired.drain(..) {
            if shown.contains(&image.id) {
                continue;
            }
            if let Some(window) = window.as_deref_mut()
                && let Some(render) = Arc::clone(&image).get_render_image(window, cx)
            {
                cx.drop_image(render, Some(window));
            }
            image.remove_asset(cx);
        }
    }

    pub fn follow(&mut self) {
        self.hold = Hold::Await(self.last_turn);
        self.press = None;
        self.list.set_frozen(false);
        self.list.glide_to_end();
    }

    pub(super) fn update_hold(&mut self, view: &ChatView) {
        let last = self
            .rows
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, row)| match row {
                Row::Turn(turn) => view.turns.get(*turn).map(|turn| (index, turn)),
                Row::Background(_) => None,
            });
        let tracking = self.list.is_following_tail() || self.list.is_gliding();
        self.hold = match self.hold {
            Hold::Await(after) => match last {
                Some((_, turn)) if Some(turn.id) != after && turn.is_open() => {
                    if tracking {
                        Hold::Measure(turn.id)
                    } else {
                        Hold::Idle
                    }
                }
                _ => self.hold,
            },
            Hold::Measure(id) | Hold::Set(id) => {
                let released =
                    matches!(self.hold, Hold::Set(_)) && self.list.follow_hold().is_none();
                match last.filter(|(_, turn)| turn.id == id) {
                    Some((index, _)) if !released && tracking => {
                        match self.marks.reply.get().filter(|(mark, _)| *mark == id) {
                            Some((_, offset)) => {
                                self.list.set_follow_hold(Some(ListOffset {
                                    item_ix: index,
                                    offset_in_item: offset,
                                }));
                                Hold::Set(id)
                            }
                            None => self.hold,
                        }
                    }
                    _ => {
                        if !released {
                            self.list.set_follow_hold(None);
                        }
                        Hold::Idle
                    }
                }
            }
            Hold::Pinned if self.list.follow_hold().is_none() => Hold::Idle,
            hold => hold,
        };
    }

    pub(super) fn press(&mut self, at: Point<Pixels>) {
        self.press = Some(at);
        self.list.set_frozen(true);
    }

    pub(super) fn let_go(&mut self, at: Point<Pixels>) {
        let Some(start) = self.press.take() else {
            return;
        };
        self.list.set_frozen(false);
        let moved = at - start;
        if f32::from(moved.x).hypot(f32::from(moved.y)) > DRAG_SLOP {
            self.list.pause_following_tail();
        }
    }

    pub fn scroll_to_top(&mut self) {
        self.list.scroll_to(ala::ListOffset::default());
    }

    pub fn reveal(&mut self, view: &ChatView, item: &ItemId) {
        let mut root = item.clone();
        while let Some(parent) = view.item(&root).and_then(|found| found.parent.clone()) {
            root = parent;
        }
        let Some(found) = view.item(&root) else {
            return;
        };
        let Some(index) = self.rows.iter().position(|row| match row {
            Row::Turn(index) => view
                .turns
                .get(*index)
                .is_some_and(|turn| Some(turn.id) == found.turn),
            Row::Background(unit) => holds(unit, &root),
        }) else {
            return;
        };
        if let Some(Row::Turn(index)) = self.rows.get(index)
            && let Some(turn) = view.turns.get(*index)
        {
            let rows = chat_rows::turn_rows(view, turn, chat_rows::now_ms(), &self.fold);
            if rows.folded.iter().any(|unit| holds(unit, &root))
                && !self.open.turns.contains(&turn.id)
            {
                self.open.turns.push(turn.id);
            }
            let run = rows
                .shown
                .iter()
                .chain(&rows.folded)
                .find_map(|unit| match unit {
                    Unit::Run(run) if run.items.contains(&root) => run.items.first().cloned(),
                    _ => None,
                });
            if let Some(run) = run
                && !self.open.runs.contains(&run)
            {
                self.open.runs.push(run);
            }
        }
        if root != *item && found.kind == ItemKind::Subagent {
            self.open.agent = Some(root);
        }
        self.list.pause_following_tail();
        self.list.scroll_to_reveal_item(index);
    }

    #[must_use]
    pub fn away(&self) -> bool {
        !self.list.is_following_tail()
    }

    #[must_use]
    pub fn shows_jump(&self) -> bool {
        (self.away() || self.list.follow_hold().is_some())
            && !self.list.is_gliding()
            && self.list.is_scrolled_to_end() == Some(false)
    }

    fn draw<'a, 'h>(
        &'a self,
        view: &'a ChatView,
        hooks: &'a ChatHooks<'h>,
        cx: &'a App,
    ) -> Draw<'a, 'h> {
        Draw {
            view,
            now: self.frame.now,
            open: &self.open,
            phase: self.frame.phase,
            hooks,
            fold: &self.fold,
            marks: &self.marks,
            cx,
        }
    }

    pub(super) fn row(
        &self,
        index: usize,
        view: &ChatView,
        hooks: &ChatHooks<'_>,
        cx: &App,
    ) -> AnyElement {
        let draw = self.draw(view, hooks, cx);
        let body = match self.rows.get(index) {
            Some(Row::Turn(turn)) => view.turns.get(*turn).map(|turn| draw.turn(turn)),
            Some(Row::Background(unit)) => Some(draw.unit(unit, self.background_live)),
            None => None,
        };
        let gutter = if hooks.touch {
            px(TOUCH_GUTTER)
        } else {
            px(12.0)
        };
        div()
            .w_full()
            .px(gutter)
            .pb(gutter)
            .child(
                div()
                    .w_full()
                    .max_w(px(CHAT_CONTENT_MAX_WIDTH))
                    .mx_auto()
                    .children(body),
            )
            .into_any_element()
    }
}
