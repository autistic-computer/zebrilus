use std::rc::Rc;

use ala::{AnyElement, App, ElementId, IntoElement, SharedString, Window, div, prelude::*, px};

use crate::{
    ActiveTheme as _, Colorize as _, Disableable as _, Icon, IconName, Sizable as _,
    StyledExt as _,
    button::{Button, ButtonLook},
    h_flex,
    menu::{DropdownMenu as _, PopupMenu, PopupMenuItem},
    tooltip::Tooltip,
};
use penna::TextStyle;
use zz_client::agent_settings::{self, LensSettings};

use penna::blocks::chat::{self, COMPOSER_FOOTER_HEIGHT};

pub use zz_client::agent_timeline::{
    ComposerAction, composer_action, context_usage_fraction, context_usage_tooltip,
    git_file_count_label,
};

pub fn composer_action_button(
    id: impl Into<ElementId>,
    action: ComposerAction,
    enabled: bool,
    cx: &App,
) -> Button {
    let action = match action {
        ComposerAction::Send => chat::ComposerAction::Send,
        ComposerAction::Queue => chat::ComposerAction::Queue,
        ComposerAction::Stop => chat::ComposerAction::Stop,
    };
    chat::composer_action_button(id, action, enabled, cx)
}

pub fn agent_directory_chip(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    enabled: bool,
    cx: &App,
) -> Button {
    chat::chrome_button(id)
        .min_w_0()
        .flex_shrink_1()
        .max_w(px(220.0))
        .overflow_hidden()
        .icon(IconName::Folder)
        .text_color(cx.theme().foreground.muted())
        .tooltip("Move this chat to another folder")
        .disabled(!enabled)
        .child(div().min_w_0().text_ellipsis().child(label.into()))
}

pub fn agent_settings_menu(
    id: impl Into<ElementId>,
    settings: &LensSettings,
    enabled: bool,
    on_pick: impl Fn(&'static str, String, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let summary = settings.summary();
    let choices = !settings.models.is_empty() || !settings.efforts.is_empty();
    let button = chat::chrome_button(id)
        .text_color(cx.theme().foreground.muted())
        .disabled(!enabled || !choices)
        .child(if summary.is_empty() {
            "Model".to_owned()
        } else {
            summary
        })
        .child(
            Icon::new(IconName::ChevronDown)
                .small()
                .text_color(cx.theme().foreground.muted()),
        );
    let settings = settings.clone();
    let on_pick = Rc::new(on_pick);
    button
        .tooltip("Choose the model and effort")
        .dropdown_menu(move |menu, _, _| {
            let menu = choice_items(menu, "Model", agent_settings::MODEL, &settings, &on_pick);
            let menu = if settings.models.is_empty() || settings.efforts.is_empty() {
                menu
            } else {
                menu.separator()
            };
            choice_items(menu, "Effort", agent_settings::EFFORT, &settings, &on_pick)
        })
        .into_any_element()
}

fn choice_items<F>(
    menu: PopupMenu,
    title: &'static str,
    option: &'static str,
    settings: &LensSettings,
    on_pick: &Rc<F>,
) -> PopupMenu
where
    F: Fn(&'static str, String, &mut Window, &mut App) + 'static,
{
    let Some((choices, current)) = settings
        .choices(option)
        .filter(|(choices, _)| !choices.is_empty())
    else {
        return menu;
    };
    choices
        .iter()
        .fold(menu.item(PopupMenuItem::label(title)), |menu, choice| {
            let pick = on_pick.clone();
            let value = choice.value.clone();
            menu.item(
                PopupMenuItem::new(choice.name.clone())
                    .checked(choice.value == current)
                    .on_click(move |_, window, cx| pick(option, value.clone(), window, cx)),
            )
        })
}

pub fn agent_side_chat_button(
    id: impl Into<ElementId>,
    enabled: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> Button {
    chat::chrome_button(id)
        .look(ButtonLook::Quiet)
        .icon(IconName::ChatPlus)
        .tooltip("Side chat: a floating copy of this chat for a quick question")
        .disabled(!enabled)
        .on_click(move |_, window, cx| on_click(window, cx))
}

pub fn git_summary_footer(
    id: impl Into<ElementId>,
    branch: Option<SharedString>,
    changed_files: u32,
    additions: u32,
    deletions: u32,
    cx: &App,
) -> AnyElement {
    let files = git_file_count_label(changed_files);
    let tooltip = zz_client::agent_timeline::git_summary_tooltip(
        branch.as_deref(),
        changed_files,
        additions,
        deletions,
    );
    h_flex()
        .id(id)
        .min_w_0()
        .h(px(COMPOSER_FOOTER_HEIGHT))
        .items_center()
        .gap_2()
        .type_style(TextStyle::Caption, cx)
        .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
        .child(
            Icon::new(IconName::GitBranch)
                .small()
                .flex_none()
                .text_color(cx.theme().foreground.muted())
                .relative()
                .top(crate::icon::cap_center_drop(px(11.0), px(12.0), cx)),
        )
        .when_some(branch, |this, branch| {
            this.child(
                div()
                    .min_w_0()
                    .max_w(px(220.0))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_color(cx.theme().foreground.muted())
                    .child(branch),
            )
        })
        .child(
            div()
                .flex_none()
                .text_color(cx.theme().foreground.muted())
                .child(files),
        )
        .child(
            div()
                .flex_none()
                .text_color(cx.theme().success)
                .child(format!("+{additions}")),
        )
        .child(
            div()
                .flex_none()
                .text_color(cx.theme().danger)
                .child(format!("-{deletions}")),
        )
        .into_any_element()
}

#[allow(clippy::cast_possible_truncation)]
pub fn context_usage_meter(id: impl Into<ElementId>, used: u64, size: u64, cx: &App) -> AnyElement {
    chat::context_meter(
        id,
        context_usage_fraction(used, size) as f32,
        context_usage_tooltip(used, size),
        cx,
    )
}
