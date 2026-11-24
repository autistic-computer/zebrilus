use std::rc::Rc;

use ala::{App, ParentElement as _, Role, SharedString, Styled as _, Window, div, prelude::*, px};
use penna::{AuthoredElement as _, Label, Size, Space, TextStyle};
use zz_protocol::HostPlatform;

use super::{SettingEntry, SettingsStack, settings_card_ground, settings_group_header};
use crate::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex, muted_on,
    program::platform_icon,
    tooltip::Tooltip,
    v_flex,
};

const HOST_COLUMN: f32 = 150.0;
const ZZ_COLUMN: f32 = 176.0;
const ACTIONS_COLUMN: f32 = 132.0;
const COLUMN_GAP: f32 = 10.0;
const ACTIONS_GAP: f32 = 8.0;
const HEADER_TRIM: f32 = 4.0;
const ROW_CONTROL: f32 = 24.0;
const NOTE_RISE: f32 = 2.0;

type HostAction = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostZz {
    Offline(SharedString),
    Unknown,
    Current(SharedString),
    Outdated {
        daemon: Option<SharedString>,
        target: SharedString,
        failed: Option<SharedString>,
    },
    RestartPending {
        installed: SharedString,
        failed: Option<SharedString>,
    },
    Updating {
        progress: Option<SharedString>,
    },
    CannotUpdate {
        daemon: Option<SharedString>,
        advice: SharedString,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostRow {
    pub name: SharedString,
    pub platform: HostPlatform,
    pub address: SharedString,
    pub remove: SharedString,
    pub zz: HostZz,
}

#[must_use]
pub fn hosts_heading(
    update_all: Option<impl Fn(&mut Window, &mut App) + 'static>,
    cx: &App,
) -> ala::Div {
    h_flex()
        .gap(px(COLUMN_GAP))
        .child(settings_group_header("Machines".into(), cx).flex_1())
        .children(update_all.map(|update_all| {
            Button::new("settings-hosts-update-all")
                .primary()
                .small()
                .label("Update all")
                .on_click(move |_, window, cx| update_all(window, cx))
        }))
}

#[must_use]
pub fn hosts_table(
    rows: &[HostRow],
    on_update: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
    on_remove: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
    cx: &App,
) -> SettingsStack {
    if rows.is_empty() {
        return SettingsStack::new().child(
            SettingEntry::new("No hosts yet").description("Hosts you add show up in the sidebar"),
        );
    }
    let on_update: HostAction = Rc::new(on_update);
    let on_remove: HostAction = Rc::new(on_remove);
    SettingsStack::new().child(header_row(cx)).children(
        rows.iter()
            .map(|row| host_row(row, &on_update, &on_remove, cx)),
    )
}

fn muted(style: TextStyle, text: impl Into<SharedString>, cx: &App) -> Label {
    Label::new(style, text).muted().on(settings_card_ground(cx))
}

fn header_row(cx: &App) -> SettingEntry {
    let label = |column: &'static str| {
        div()
            .author_id(SharedString::from(format!("hosts-head-{column}")))
            .role(Role::ColumnHeader)
            .child(muted(TextStyle::Caption, column, cx))
    };
    SettingEntry::bare().child(
        h_flex()
            .w_full()
            .my(px(-HEADER_TRIM))
            .gap(px(COLUMN_GAP))
            .child(label("Host").flex_none().w(px(HOST_COLUMN)))
            .child(label("Address").flex_1().min_w_0())
            .child(label("zz").flex_none().w(px(ZZ_COLUMN)))
            .child(div().flex_none().w(px(ACTIONS_COLUMN))),
    )
}

fn zz_cell(name: &SharedString, zz: &HostZz, cx: &App) -> ala::Stateful<ala::Div> {
    let (text, tip): (SharedString, Option<SharedString>) = match zz {
        HostZz::Offline(state) => (state.clone(), None),
        HostZz::Unknown => (SharedString::default(), None),
        HostZz::Current(version) => (version.clone(), None),
        HostZz::Outdated { daemon, target, .. } => (
            daemon.as_ref().map_or_else(
                || format!("older → {target}").into(),
                |daemon| format!("{daemon} → {target}").into(),
            ),
            None,
        ),
        HostZz::RestartPending { installed, .. } => (format!("{installed} installed").into(), None),
        HostZz::Updating { progress } => (progress.clone().unwrap_or_default(), None),
        HostZz::CannotUpdate { daemon, advice } => (
            daemon.as_ref().map_or_else(
                || "update it there".into(),
                |daemon| format!("{daemon}, update it there").into(),
            ),
            Some(advice.clone()),
        ),
    };
    h_flex()
        .id(SharedString::from(format!("hosts-zz-{name}")))
        .flex_none()
        .w(px(ZZ_COLUMN))
        .gap(px(5.0))
        .when(matches!(zz, HostZz::Current(_)), |this| {
            this.child(
                Icon::new(IconName::Check)
                    .small()
                    .text_color(cx.theme().success),
            )
        })
        .child(
            Label::new(TextStyle::Caption, text.clone())
                .mono()
                .muted()
                .min_w_0()
                .truncate()
                .on(settings_card_ground(cx)),
        )
        .tooltip(move |window, cx| {
            Tooltip::new(tip.clone().unwrap_or_else(|| text.clone())).build(window, cx)
        })
}

fn action(name: &SharedString, zz: &HostZz, on_update: &HostAction, cx: &App) -> Option<Button> {
    let ground = settings_card_ground(cx);
    let id = SharedString::from(format!("hosts-update-{name}"));
    let run = |button: Button| {
        let on_update = Rc::clone(on_update);
        let name = name.clone();
        button.on_click(move |_, window, cx| on_update(&name, window, cx))
    };
    Some(match zz {
        HostZz::Outdated {
            failed: Some(_), ..
        }
        | HostZz::RestartPending {
            failed: Some(_), ..
        } => run(Button::new(id).small().label("Try again").on(ground)),
        HostZz::Outdated { .. } => run(Button::new(id).primary().small().label("Update")),
        HostZz::RestartPending { .. } => run(Button::new(id).primary().small().label("Restart")),
        HostZz::Updating { .. } => Button::new(id)
            .small()
            .loading(true)
            .disabled(true)
            .label("Updating")
            .on(ground),
        HostZz::Offline(_) | HostZz::Unknown | HostZz::Current(_) | HostZz::CannotUpdate { .. } => {
            return None;
        }
    })
}

fn host_row(
    row: &HostRow,
    on_update: &HostAction,
    on_remove: &HostAction,
    cx: &App,
) -> SettingEntry {
    let ground = settings_card_ground(cx);
    let offline = matches!(row.zz, HostZz::Offline(_));
    let mark = platform_icon(row.platform).unwrap_or(IconName::Globe);
    let name = h_flex()
        .flex_none()
        .w(px(HOST_COLUMN))
        .gap(Space::S8)
        .child(
            Icon::new(mark)
                .with_size(Size::Medium)
                .text_color(muted_on(ground, cx)),
        )
        .child(
            Label::new(TextStyle::Body, row.name.clone())
                .min_w_0()
                .truncate()
                .when(offline, |label| label.muted().on(ground)),
        );
    let failed = match &row.zz {
        HostZz::Outdated { failed, .. } | HostZz::RestartPending { failed, .. } => failed.clone(),
        _ => None,
    };
    let remove_name = row.name.clone();
    let on_remove = Rc::clone(on_remove);
    let line = h_flex()
        .w_full()
        .min_h(px(ROW_CONTROL))
        .gap(px(COLUMN_GAP))
        .child(name)
        .child(
            muted(TextStyle::Small, row.address.clone(), cx)
                .flex_1()
                .min_w_0()
                .truncate(),
        )
        .child(zz_cell(&row.name, &row.zz, cx))
        .child(
            h_flex()
                .flex_none()
                .w(px(ACTIONS_COLUMN))
                .justify_end()
                .gap(px(ACTIONS_GAP))
                .children(action(&row.name, &row.zz, on_update, cx))
                .child(
                    Button::compact_icon(
                        SharedString::from(format!("settings-remove-host-{}", row.name)),
                        Icon::new(IconName::Xmark).text_color(muted_on(ground, cx)),
                    )
                    .tooltip(row.remove.clone())
                    .on(ground)
                    .on_click(move |_, window, cx| on_remove(&remove_name, window, cx)),
                ),
        );
    SettingEntry::bare().child(v_flex().w_full().child(line).children(failed.map(|reason| {
        Label::new(TextStyle::Caption, reason)
            .mono()
            .min_w_0()
            .text_color(cx.theme().warning)
            .mt(px(NOTE_RISE))
            .pl(px(HOST_COLUMN + COLUMN_GAP))
    })))
}
