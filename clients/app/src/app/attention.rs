use std::collections::BTreeMap;

use ala::{App, Context, SystemNotification, Window};
use web_time::Instant;
use zz_client::AgentAttentionEdge;
use zz_client::attention::{
    self, AgentAttentionTracker, AgentPaneStatus, AttentionBanners, AttentionEdge, chime_due,
    loudest,
};
use zz_protocol::PaneId;

use super::{AppShell, sidebar};

#[derive(Default)]
pub(super) struct Attention {
    tracker: AgentAttentionTracker<PaneId>,
    banners: AttentionBanners<PaneId>,
    last_chime: Option<Instant>,
    shown: Option<(usize, bool)>,
}

impl Attention {
    pub(super) fn forget_host(&mut self, cx: &App) {
        for pane in self.banners.retire(&BTreeMap::new(), None) {
            cx.dismiss_system_notification(&attention::tag_for(pane));
        }
        self.tracker = AgentAttentionTracker::default();
    }
}

impl AppShell {
    pub(super) fn reconcile_attention(
        &mut self,
        window: &mut Window,
        reported: Option<(PaneId, AgentAttentionEdge)>,
        cx: &mut Context<Self>,
    ) {
        let watched = window
            .is_window_active()
            .then(|| self.active_window(cx).map(|active| active.active_pane))
            .flatten();
        let core = &self.connection.read(cx).core;
        let statuses = attention::pane_statuses(core);
        let edges = self.attention.tracker.observe_reported(
            &statuses,
            watched,
            |pane| core.agent_state(pane).is_some(),
            reported.map(|(pane, edge)| (pane, edge.into())),
        );
        if let Some(chime) = loudest(&edges)
            && chime_due(&mut self.attention.last_chime, Instant::now())
        {
            crate::chime::play(chime);
        }
        self.post_banners(&statuses, watched, &edges, window, cx);
        let waiting = self.attention.tracker.waiting(&statuses);
        let notify = self.preferences.notifications;
        if self.attention.shown.replace((waiting, notify)) != Some((waiting, notify)) {
            cx.set_app_badge(if notify { waiting } else { 0 });
            #[cfg(target_family = "wasm")]
            window.set_window_title(&tab_title(waiting));
        }
    }

    fn post_banners(
        &mut self,
        statuses: &BTreeMap<PaneId, AgentPaneStatus>,
        watched: Option<PaneId>,
        edges: &[AttentionEdge<PaneId>],
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        for pane in self.attention.banners.retire(statuses, watched) {
            cx.dismiss_system_notification(&attention::tag_for(pane));
        }
        if edges.is_empty() || window.is_window_active() || !self.preferences.notifications {
            return;
        }
        let snapshot = self.connection.read(cx).core.snapshot();
        let now = Instant::now();
        let banners = edges
            .iter()
            .filter_map(|edge| {
                let (title, body) = attention::banner(snapshot, edge.pane, edge.status)?;
                self.attention
                    .banners
                    .admit(edge.pane, now)
                    .then(|| SystemNotification {
                        tag: attention::tag_for(edge.pane).into(),
                        title: title.into(),
                        body: body.into(),
                        actions: Vec::new(),
                    })
            })
            .collect::<Vec<_>>();
        if banners.is_empty() {
            return;
        }
        if self.attention.banners.listen() {
            let view = cx.weak_entity();
            let handle = window.window_handle();
            cx.on_system_notification_response(move |response, cx| {
                handle
                    .update(cx, |_, window, cx| {
                        view.update(cx, |this, cx| {
                            this.open_attention(&response.tag, window, cx);
                        })
                    })
                    .ok();
            });
        }
        for banner in banners {
            cx.show_system_notification(banner);
        }
    }

    fn open_attention(&mut self, tag: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pane) = self.attention.banners.pane(tag) else {
            return;
        };
        self.settings = None;
        sidebar::activate(&self.connection, sidebar::Target::Pane(pane), cx);
        if Self::narrow(window) {
            self.close_overview(cx);
        } else {
            self.focus_pane(pane, window, cx);
        }
        cx.notify();
    }
}

#[cfg(target_family = "wasm")]
fn tab_title(waiting: usize) -> String {
    let name = zz_protocol::app_identity::DISPLAY_NAME;
    if waiting == 0 {
        name.to_owned()
    } else {
        format!("({waiting}) {name}")
    }
}
