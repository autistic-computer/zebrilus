use ala::{
    App, Context, FocusHandle, InteractiveElement as _, IntoElement, OwnedMenu, OwnedMenuItem,
    RenderOnce, Window, prelude::FluentBuilder as _,
};
use penna::{
    Ground, IconName,
    blocks::sidebar::chrome_button,
    components::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem},
};

#[derive(IntoElement)]
pub(crate) struct AppMenuButton(pub(crate) Ground);

impl RenderOnce for AppMenuButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let target = window.use_keyed_state("app-menu-target", cx, |_, _| None::<FocusHandle>);
        let mouse_target = target.clone();
        let key_target = target.clone();
        chrome_button("workspace-app-menu", IconName::Menu, "App menu")
            .on(self.0)
            .capture_any_mouse_down(move |_, window, cx| {
                mouse_target.update(cx, |target, cx| *target = window.focused(cx));
            })
            .capture_key_down(move |_, window, cx| {
                key_target.update(cx, |target, cx| *target = window.focused(cx));
            })
            .dropdown_menu(move |mut popup, window, cx| {
                let focus = target.read(cx).clone();
                for group in cx.get_menus().unwrap_or_default() {
                    popup = submenu(popup, group, focus.as_ref(), window, cx);
                }
                popup
            })
    }
}

fn submenu(
    popup: PopupMenu,
    group: OwnedMenu,
    focus: Option<&FocusHandle>,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    if group.disabled || group.items.is_empty() {
        return popup.item(PopupMenuItem::new(group.name).disabled(true));
    }
    let focus = focus.cloned();
    popup.submenu_with_icon(None, group.name, window, cx, move |menu, window, cx| {
        let mut menu = menu.when_some(focus.clone(), PopupMenu::action_context);
        for item in &group.items {
            menu = match item {
                OwnedMenuItem::Separator => menu.separator(),
                OwnedMenuItem::Submenu(group) => {
                    submenu(menu, group.clone(), focus.as_ref(), window, cx)
                }
                OwnedMenuItem::Action {
                    name,
                    action,
                    checked,
                    disabled,
                    ..
                } => {
                    let available = focus.as_ref().is_some_and(|handle| {
                        window.is_action_available_in(action.as_ref(), handle)
                    }) || cx.is_action_available(action.as_ref());
                    menu.item(
                        PopupMenuItem::new(name.clone())
                            .checked(*checked)
                            .disabled(*disabled || !available)
                            .action(action.boxed_clone()),
                    )
                }
                OwnedMenuItem::SystemMenu(_) => menu,
            };
        }
        menu
    })
}
