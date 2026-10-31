use ala::{App, ElementId, IntoElement, ParentElement as _, SharedString, Styled as _, div, px};

use super::{SettingEntry, SettingsStack, settings_card_ground, settings_provenance_badge};
use crate::{
    ActiveTheme as _, Colorize as _, IconName, Sizable as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
};

pub const ABOUT_LOGO_SIZE: f32 = 88.0;

#[derive(Clone, Copy, Debug)]
pub struct About {
    pub name: &'static str,
    pub tagline: &'static str,
    pub version: &'static str,
    pub license: &'static str,
    pub links: &'static [AboutLink],
}

#[derive(Clone, Copy, Debug)]
pub struct AboutLink {
    pub id: &'static str,
    pub title: &'static str,
    pub label: &'static str,
    pub url: &'static str,
}

#[must_use]
pub fn about_hero(about: &About, logo: impl IntoElement, cx: &App) -> ala::Div {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(12.0))
        .pt(px(20.0))
        .pb(px(10.0))
        .child(logo)
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(5.0))
                .child(
                    div()
                        .text_size(crate::rems_from_px(22.0))
                        .font_medium()
                        .child(about.name),
                )
                .child(
                    div()
                        .text_size(crate::rems_from_px(12.0))
                        .text_color(cx.theme().foreground.muted())
                        .child(about.tagline),
                )
                .child(settings_provenance_badge(format!("v{}", about.version))),
        )
}

#[must_use]
pub fn about_value(value: impl Into<SharedString>, cx: &App) -> ala::Div {
    div()
        .flex_none()
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(crate::rems_from_px(11.0))
        .text_color(cx.theme().foreground.muted())
        .child(value.into())
}

#[must_use]
pub fn about_link_button(
    id: impl Into<ElementId>,
    label: &'static str,
    url: &'static str,
    cx: &App,
) -> Button {
    Button::new(id)
        .icon(IconName::ExternalLink)
        .label(label)
        .on(settings_card_ground(cx))
        .on_click(move |_, _, cx| cx.open_url(url))
}

#[must_use]
pub fn about_copy_button(id: impl Into<ElementId>) -> Button {
    Button::new(id)
        .small()
        .compact()
        .ghost()
        .icon(IconName::Copy)
        .tooltip("Copy build information")
}

#[must_use]
pub fn build_info(about: &About, platform: &str) -> String {
    format!("{} {} ({platform})", about.name, about.version)
}

#[must_use]
pub fn about_build_stack(
    about: &About,
    platform: impl Into<SharedString>,
    copy: impl IntoElement,
    cx: &App,
) -> SettingsStack {
    SettingsStack::titled("Build")
        .child(
            SettingEntry::new("Version")
                .title_actions(copy)
                .control(about_value(about.version, cx)),
        )
        .child(SettingEntry::new("Platform").control(about_value(platform, cx)))
}

#[must_use]
pub fn about_project_stack(about: &About, cx: &App) -> SettingsStack {
    SettingsStack::titled("Project")
        .children(about.links.iter().map(|link| {
            SettingEntry::new(link.title)
                .control(about_link_button(link.id, link.label, link.url, cx))
        }))
        .child(SettingEntry::new("License").control(about_value(about.license, cx)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_info_names_the_app_its_version_and_the_platform() {
        let about = About {
            name: "notes",
            tagline: "",
            version: "1.2.3",
            license: "MIT",
            links: &[],
        };
        assert_eq!(
            build_info(&about, "macos · aarch64"),
            "notes 1.2.3 (macos · aarch64)"
        );
    }
}
