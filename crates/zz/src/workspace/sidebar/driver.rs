use zz_gpui::{
    AnyElement, AppContext as _, Context, Entity, IntoElement as _, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, div, px,
};
use zz_ui::{
    agent::chat::{
        ChatBubble, ChatTyping, chat_asleep, chat_header, chat_latest, chat_unreachable,
    },
    navigation::sidebar::{TreeRowMenuItem, tree_row_menu},
};

use super::{SidebarOpenDriver, SidebarShowDriverTerminal, WorkspaceSidebar};
use crate::{agent::driver::DriverChat, mux::client::MuxClient};

/// How many lines of the driver's last answer the sidebar shows.
const LATEST_LINES: usize = 4;
const ATTENTION_DOT: f32 = 7.0;

/// The driver's last message at the bottom of the sidebar. The chat itself is
/// the command palette's Driver mode, which a click on it opens.
pub(super) struct DriverSection {
    chat: Entity<DriverChat>,
}

impl DriverSection {
    pub(super) fn new(
        mux: Entity<MuxClient>,
        local_name: SharedString,
        cx: &mut Context<WorkspaceSidebar>,
    ) -> Self {
        let chat = cx.new(|cx| DriverChat::new(mux, local_name, cx));
        cx.observe(&chat, |_, _, cx| cx.notify()).detach();
        Self { chat }
    }
}

impl WorkspaceSidebar {
    pub(crate) fn driver_chat(&self) -> Entity<DriverChat> {
        self.driver.chat.clone()
    }

    /// `navigation` over the driver's last message while there is a driver and it
    /// has said something or is at work; `navigation` alone otherwise.
    pub(super) fn with_driver_latest(
        &self,
        navigation: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chat = self.driver.chat.read(cx);
        let (Some(sender), Some(driver)) = (chat.driver_sender(), chat.driver()) else {
            let ground = self.content_ground(cx);
            let entity = self.driver.chat.clone();
            let idle = if let Some((answered, hosts)) = chat.unreachable() {
                chat_unreachable(
                    "workspace-driver-unreachable",
                    answered,
                    hosts,
                    ground,
                    move |_, _, cx| entity.update(cx, DriverChat::start_here),
                    cx,
                )
            } else if chat.asleep(cx) {
                chat_asleep(
                    "workspace-driver-asleep",
                    chat.waking(),
                    ground,
                    move |_, _, cx| entity.update(cx, DriverChat::wake),
                    cx,
                )
            } else {
                return navigation;
            };
            return div()
                .flex()
                .flex_col()
                .size_full()
                .min_h_0()
                .child(div().flex().flex_col().flex_1().min_h_0().child(navigation))
                .child(idle)
                .into_any_element();
        };
        let ground = self.content_ground(cx);
        let message = if let Some(steps) = chat.steps(cx) {
            ChatTyping::new("workspace-driver-typing")
                .steps(steps)
                .on(ground)
                .into_any_element()
        } else if let Some(text) = chat.last_answer(cx) {
            ChatBubble::theirs("workspace-driver-latest-message", text, "")
                .lines(LATEST_LINES)
                .on(ground)
                .into_any_element()
        } else {
            return navigation;
        };
        let attention = chat.attention(cx).map(|color| {
            div()
                .size(px(ATTENTION_DOT))
                .rounded_full()
                .bg(color)
                .into_any_element()
        });
        let header = chat_header(&sender, driver.host_name.clone(), attention, cx);
        let sidebar = cx.entity().downgrade();
        let show_terminal = TreeRowMenuItem::new("Show terminal", move |_, cx| {
            let _ = sidebar.update(cx, |_, cx| cx.emit(SidebarShowDriverTerminal));
        });
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .child(div().flex().flex_col().flex_1().min_h_0().child(navigation))
            .child(tree_row_menu(
                chat_latest("workspace-driver-latest", header, message, cx)
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(SidebarOpenDriver))),
                vec![show_terminal],
            ))
            .into_any_element()
    }
}
