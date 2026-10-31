use std::{cell::RefCell, rc::Rc, sync::Arc};

use ala::{
    AnyElement, App, Context, Entity, IntoElement, Subscription, TextInputMode, Window, canvas,
    div, prelude::*, px,
};
use penna::{
    ActiveTheme as _, Colorize as _, IconName, Theme,
    input::{InputEvent, InputState, NumberInput},
    menu::PopupMenuItem,
    select::{Select, SelectEvent, SelectState},
    settings::{
        about::{about_build_stack, about_copy_button, about_hero, about_project_stack},
        appearance::{
            ADVANCED_DISCLOSURE, AppearanceControls, appearance_page, kit_row, percent_slider,
        },
    },
    switch::Switch,
};
use zz_ui::{
    interface_theme::{self, AppearanceChange},
    pane_look::{PaneLook, pane_color},
    settings::{
        SettingEntry, SettingsFold, SettingsScrollColumn, SettingsSection, SettingsSelectItem,
        SettingsStack, StackPosition,
        about::{ABOUT_LOGO_SIZE, ZZ_ABOUT},
        appearance::{AppearancePageItem, ZzRow, appearance_page_items},
        settings_card_ground, settings_list_disclosure_header, settings_page_heading,
        settings_preview, settings_scroll_column, settings_title_bar,
    },
};

use super::AppShell;
use crate::{
    connection::Connection,
    terminal::{TerminalDisplayPreferences, localized_font_appearance},
};

pub(super) use crate::preferences::Preferences;

const PALETTE_LAYOUTS: [(&str, &str); 2] = [("grouped", "Tree"), ("flat", "Flat")];

fn palette_layout_value(preferences: &Preferences) -> &'static str {
    if preferences.palette_grouped {
        "grouped"
    } else {
        "flat"
    }
}

pub(super) const SECTIONS: [SettingsSection; 8] = [
    SettingsSection::Appearance,
    SettingsSection::StatusBar,
    SettingsSection::Panes,
    SettingsSection::Terminal,
    SettingsSection::Hosts,
    SettingsSection::Agents,
    SettingsSection::Advanced,
    SettingsSection::About,
];

const PHONE_MAX_ZOOM: f32 = 1.35;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Setting {
    Section(SettingsSection),
    SystemTextSize,
    CommandPalette,
}

pub(super) fn shown(setting: Setting, phone: bool) -> bool {
    !phone
        || !matches!(
            setting,
            Setting::Section(SettingsSection::StatusBar | SettingsSection::Panes)
                | Setting::SystemTextSize
                | Setting::CommandPalette
        )
}

pub(crate) fn phone() -> bool {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    return ala_platform::mobile::phone();
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    false
}

pub(super) fn sections() -> Vec<SettingsSection> {
    let phone = phone();
    SECTIONS
        .into_iter()
        .filter(|section| shown(Setting::Section(*section), phone))
        .collect()
}

fn interface_zoom(preferences: &Preferences, system_scale: f32, phone: bool) -> f32 {
    let zoom = preferences.appearance.zoom;
    if phone {
        zoom * system_scale.min(PHONE_MAX_ZOOM)
    } else if preferences.system_text_size {
        zoom * system_scale
    } else {
        zoom
    }
}

struct PlatformReduceMotion(bool);

impl ala::Global for PlatformReduceMotion {}

impl Preferences {
    pub(super) fn apply(&self, connection: &Entity<Connection>, window: &mut Window, cx: &mut App) {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        let system = ala_platform::mobile::accessibility();
        #[cfg(any(target_os = "ios", target_os = "android"))]
        cx.set_global(PlatformReduceMotion(system.reduce_motion));
        if !cx.has_global::<PlatformReduceMotion>() {
            cx.set_global(PlatformReduceMotion(cx.reduce_motion()));
        }
        cx.set_global(zz_ui::program::ProgramColors(self.program_colors));
        #[cfg(any(target_os = "ios", target_os = "android"))]
        let (contrast, text_scale) = (
            self.appearance.contrast * if system.increase_contrast { 1.25 } else { 1.0 },
            system.text_scale,
        );
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let (contrast, text_scale) = (self.appearance.contrast, 1.0);
        let pinned = interface_theme::pinned_theme_mode(self.theme_mode());
        #[cfg(any(target_os = "ios", target_os = "android"))]
        let mode = {
            cx.set_window_appearance(pinned.map(|mode| {
                if mode.is_dark() {
                    ala::WindowAppearance::Dark
                } else {
                    ala::WindowAppearance::Light
                }
            }));
            Some(pinned.unwrap_or_else(|| penna::ThemeMode::from(cx.window_appearance())))
        };
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let mode = pinned;
        let mut shown = self.appearance.clone();
        shown.set_theme_mode(mode);
        shown.contrast = contrast;
        shown.zoom = interface_zoom(self, text_scale, phone());
        shown.animations &= !cx.global::<PlatformReduceMotion>().0;
        shown.apply(self.saved(), Some(window), cx);
        window.set_default_corner_smoothing(penna::CORNER_SMOOTHING);
        cx.set_global(zz_ui::pip::PipOpacity(self.pip_background_opacity));
        cx.set_global(PaneLook {
            frames: self.gaps,
            glow_strength: self.pane_glow_strength,
        });
        Theme::global_mut(cx).pane_margin = self.margin();
        let available_fonts = cx.text_system().all_font_names();
        let wanted_font = self.shown_ui_font();
        let ui_font = if available_fonts
            .iter()
            .any(|family| family.eq_ignore_ascii_case(wanted_font))
        {
            wanted_font.to_owned()
        } else if cfg!(any(
            target_family = "wasm",
            target_os = "ios",
            target_os = "android"
        )) {
            "Inter Variable".to_owned()
        } else {
            ".SystemUIFont".to_owned()
        };
        Theme::global_mut(cx).font_family = ui_font.into();
        cx.set_global(TerminalDisplayPreferences {
            font_family: self.terminal_font_family.clone(),
            font_scale: self.terminal_font_scale,
        });
        let terminal =
            localized_font_appearance(&connection.read(cx).core, &available_fonts, "Lilex", cx);
        Theme::global_mut(cx).mono_font_family = terminal.font_families[0].clone().into();
        connection.update(cx, Connection::set_color_scheme);
        cx.refresh_windows();
    }

    pub(super) fn change_zoom(
        &mut self,
        step: f32,
        connection: &Entity<Connection>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let zoom = &mut self.appearance.zoom;
        *zoom = ((*zoom + step) * 10.0).round().clamp(5.0, 30.0) / 10.0;
        self.save();
        self.apply(connection, window, cx);
    }

    pub(super) fn reset_zoom(
        &mut self,
        connection: &Entity<Connection>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.appearance.zoom = 1.0;
        self.save();
        self.apply(connection, window, cx);
    }
}

pub(super) struct Controls {
    appearance: AppearanceControls,
    panes: [Entity<InputState>; 4],
    terminal_font: Entity<SelectState<Vec<SettingsSelectItem>>>,
    palette_layout: Entity<SelectState<Vec<SettingsSelectItem>>>,
    advanced: bool,
    _subscriptions: Vec<Subscription>,
}

impl Controls {
    pub(super) fn new(
        preferences: &Preferences,
        window: &mut Window,
        cx: &mut Context<AppShell>,
    ) -> Self {
        let mut subscriptions = Vec::new();
        let changed = cx.entity().downgrade();
        let menu = cx.entity().downgrade();
        let appearance = AppearanceControls::new(
            &preferences.appearance,
            move |change, window, cx| {
                let _ = changed.update(cx, |this, cx| this.appearance_changed(change, window, cx));
            },
            window,
            cx,
        )
        .theme_menu(move |popup, _, cx| {
            let saved = menu.upgrade().is_some_and(|view| {
                let preferences = &view.read(cx).preferences;
                preferences.saved_theme(preferences.theme_id()).is_some()
            });
            let import = menu.clone();
            let delete = menu.clone();
            popup
                .item(
                    PopupMenuItem::new("Import from clipboard").on_click(move |_, window, cx| {
                        let _ = import.update(cx, |this, cx| this.import_theme(window, cx));
                    }),
                )
                .when(saved, |popup| {
                    popup
                        .separator()
                        .item(
                            PopupMenuItem::new("Delete theme").on_click(move |_, window, cx| {
                                let _ = delete.update(cx, |this, cx| {
                                    let id = this.preferences.theme_id().to_owned();
                                    this.preferences.delete_theme(&id);
                                    this.theme_changed(window, cx);
                                });
                            }),
                        )
                })
        });
        let pane_values = [
            preferences.pip_background_opacity,
            preferences.pane_inactive_opacity,
            preferences.pane_glow_strength,
            preferences.pane_margin,
        ];
        let panes = PaneControl::ALL.map(|control| {
            let (min, max, step) = control.limits();
            let scale = if matches!(control, PaneControl::PipOpacity | PaneControl::Glow) {
                100.0
            } else {
                1.0
            };
            let (min, max, step) = (min * scale, max * scale, step * f64::from(scale));
            let value = pane_values[control as usize] * scale;
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .input_mode(TextInputMode::Decimal)
                    .default_value(number_text(value))
                    .min(f64::from(min))
                    .max(f64::from(max))
                    .step(step)
            });
            subscriptions.push(cx.subscribe_in(
                &input,
                window,
                move |this, input, event, window, cx| {
                    number_field_event(event, window, cx);
                    let commit = matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. });
                    if !commit && !matches!(event, InputEvent::Change) {
                        return;
                    }
                    let parsed = input
                        .read(cx)
                        .value()
                        .parse::<f32>()
                        .ok()
                        .filter(|value| value.is_finite());
                    let previous = *control.value(&mut this.preferences) * scale;
                    let value = match parsed {
                        Some(value) if commit => value.clamp(min, max),
                        Some(value) if (min..=max).contains(&value) => value,
                        _ if commit => previous,
                        _ => return,
                    };
                    *control.value(&mut this.preferences) = value / scale;
                    if commit {
                        input.update(cx, |input, cx| {
                            input.set_value(number_text(value), window, cx);
                        });
                    }
                    this.preferences.save();
                    cx.set_global(zz_ui::pip::PipOpacity(
                        this.preferences.pip_background_opacity,
                    ));
                    cx.set_global(PaneLook {
                        frames: this.preferences.gaps,
                        glow_strength: this.preferences.pane_glow_strength,
                    });
                    Theme::global_mut(cx).pane_margin = this.preferences.margin();
                    cx.refresh_windows();
                    cx.notify();
                },
            ));
            input
        });
        let mut families = cx.text_system().all_font_names();
        families.retain(|family| !family.starts_with('.') && !family.trim().is_empty());
        families.sort_by_cached_key(|family| family.to_lowercase());
        families.dedup();
        let items = std::iter::once(SettingsSelectItem::new("Follow host", ""))
            .chain(
                families
                    .into_iter()
                    .map(|family| SettingsSelectItem::new(family.clone(), family).preview_font()),
            )
            .collect::<Vec<_>>();
        let terminal_font = cx.new(|cx| SelectState::new(items, None, window, cx));
        terminal_font.update(cx, |select, cx| {
            select.set_selected_value(
                &preferences.terminal_font_family.clone().unwrap_or_default(),
                window,
                cx,
            );
        });
        subscriptions.push(cx.subscribe_in(
            &terminal_font,
            window,
            |this, _, event: &SelectEvent<Vec<SettingsSelectItem>>, window, cx| {
                let SelectEvent::Confirm(Some(value)) = event else {
                    return;
                };
                this.preferences.terminal_font_family = (!value.is_empty()).then(|| value.clone());
                this.preferences.save();
                this.preferences.apply(&this.connection, window, cx);
            },
        ));
        let choice =
            |choices: &[(&str, &str)], current: &str, window: &mut Window, cx: &mut App| {
                let items = choices
                    .iter()
                    .map(|(value, title)| SettingsSelectItem::new(*title, *value))
                    .collect::<Vec<_>>();
                let index = choices.iter().position(|(value, _)| *value == current);
                cx.new(|cx| SelectState::new(items, index.map(penna::IndexPath::new), window, cx))
            };
        let palette_layout = choice(
            &PALETTE_LAYOUTS,
            palette_layout_value(preferences),
            window,
            cx,
        );
        subscriptions.push(cx.subscribe_in(
            &palette_layout,
            window,
            |this, _, event: &SelectEvent<Vec<SettingsSelectItem>>, _, cx| {
                let SelectEvent::Confirm(Some(value)) = event else {
                    return;
                };
                this.preferences.palette_grouped = value == "grouped";
                this.preferences.save();
                cx.notify();
            },
        ));
        Self {
            appearance,
            panes,
            terminal_font,
            palette_layout,
            advanced: false,
            _subscriptions: subscriptions,
        }
    }
}

impl AppShell {
    pub(super) fn theme_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let preferences = &self.preferences;
        self.settings_controls.appearance.sync(
            &preferences.appearance,
            preferences.saved(),
            window,
            cx,
        );
        self.preferences.save();
        self.preferences.apply(&self.connection, window, cx);
    }

    fn appearance_changed(
        &mut self,
        change: AppearanceChange,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preferences.change_appearance(change);
        self.preferences.save();
        self.preferences.apply(&self.connection, window, cx);
    }

    fn theme_saved(
        &mut self,
        saved: Result<String, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use penna::{WindowExt as _, notification::Notification};
        let notification = match saved {
            Ok(id) => {
                self.theme_changed(window, cx);
                Notification::success(format!("Saved the theme as {id}"))
            }
            Err(error) => Notification::error(error),
        };
        window.push_notification(notification, cx);
    }

    fn import_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let clipboard = cx.read_from_clipboard_async();
        cx.spawn_in(window, async move |this, cx| {
            let clipboard = clipboard.await;
            this.update_in(cx, |this, window, cx| {
                let saved = match clipboard {
                    Ok(item) => this
                        .preferences
                        .import_theme(&item.and_then(|item| item.text()).unwrap_or_default()),
                    Err(error) => Err(format!("Could not read the clipboard: {error}.")),
                };
                this.theme_saved(saved, window, cx);
            })
        })
        .detach();
    }

    pub(super) fn render_settings(
        &self,
        section: SettingsSection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let narrow = AppShell::narrow(window);
        let sections = sections();
        let section = if sections.contains(&section) {
            section
        } else {
            SettingsSection::Appearance
        };
        if !narrow {
            return self.settings_page(section, None, cx);
        }
        let view = cx.entity().downgrade();
        let shell = cx.entity().downgrade();
        let picked = self.settings_picked;
        let attached = self.active_window(cx).is_some();
        #[cfg(any(target_os = "ios", target_os = "android"))]
        let sheet = self.host_sheets(window, cx);
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let sheet: Option<AnyElement> = None;
        let route = penna::touch::swipe_back(
            "settings-swipe-back",
            self.settings_route(section, picked, cx),
            move |window, cx| {
                let _ = view.update(cx, |this, cx| this.settings_back(window, cx));
            },
        )
        .enters(attached)
        .when(picked, |swipe| {
            swipe.under(self.settings_route(section, false, cx))
        })
        .when(!picked && attached, |swipe| {
            swipe.under_with(move |window, cx| {
                let top = window.fully_visible_bounds().top();
                let height = window.viewport_size().height;
                let backdrop = shell
                    .update(cx, |this, cx| {
                        this.compact_shell(Vec::new(), Vec::new(), window, cx)
                            .into_any_element()
                    })
                    .unwrap_or_else(|_| div().into_any_element());
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(-top)
                    .h(height)
                    .child(backdrop)
                    .into_any_element()
            })
        })
        .pushed(picked)
        .leaving(self.settings_leaving);
        div()
            .size_full()
            .child(route)
            .children(sheet)
            .into_any_element()
    }

    pub(super) fn settings_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings_leaving = false;
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            self.hosts.sheet = None;
        }
        if self.settings_picked {
            self.settings_picked = false;
        } else {
            self.settings = None;
            self.focused_pane = None;
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn settings_route(
        &self,
        section: SettingsSection,
        picked: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let fold = if picked {
            &self.settings_fold
        } else {
            &self.settings_index_fold
        };
        let content = if picked {
            self.settings_page(section, Some(fold), cx)
        } else {
            #[cfg(any(target_os = "ios", target_os = "android"))]
            let host = self
                .connection
                .read(cx)
                .endpoint()
                .map(super::hosts::host_title);
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            let host: Option<String> = None;
            zz_ui::settings::settings_section_index(
                &sections(),
                move |section| match section {
                    SettingsSection::Hosts => host.clone().map(Into::into),
                    SettingsSection::About => Some(env!("CARGO_PKG_VERSION").into()),
                    _ => None,
                },
                {
                    let view = cx.entity().downgrade();
                    move |choice, _, cx| {
                        let _ = view.update(cx, |this, cx| {
                            this.settings = Some(choice);
                            this.settings_picked = true;
                            cx.notify();
                        });
                    }
                },
                cx,
            )
            .large_title("Settings", fold)
            .into_any_element()
        };
        let title = if picked { section.title() } else { "Settings" };
        let back = penna::compact::compact_bar_button("settings-back", IconName::ArrowLeft)
            .on_click(cx.listener(|this, _, _, cx| {
                this.settings_leaving = true;
                cx.notify();
            }));
        div()
            .flex()
            .flex_col_reverse()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(
                div().flex_1().min_h_0().relative().child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .child(penna::touch::touch_scale(content, cx)),
                ),
            )
            .child(settings_title_bar(title, fold, back))
            .into_any_element()
    }

    fn settings_page(
        &self,
        section: SettingsSection,
        fold: Option<&SettingsFold>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let narrow = fold.is_some();
        let large = |page: SettingsScrollColumn| match fold {
            Some(fold) => page.large_title(section.title(), fold),
            None => page,
        };
        let phone = phone();
        match section {
            SettingsSection::Appearance => {
                let items = appearance_page_items(false, false, self.settings_controls.advanced)
                    .into_iter()
                    .filter(|item| {
                        !matches!(item, AppearancePageItem::App(row) if *row != ZzRow::ProgramColors)
                    })
                    .filter(|item| !narrow || !matches!(item, AppearancePageItem::Title))
                    .collect::<Vec<_>>();
                let zoom = self.settings_controls.appearance.zoom_focus();
                let row_focus = items
                    .iter()
                    .map(|item| matches!(item, AppearancePageItem::UiZoom).then(|| zoom.clone()))
                    .collect::<Vec<_>>();
                let view = cx.entity();
                return appearance_page(
                    section.title(),
                    items,
                    move |item, position, window, cx| {
                        view.update(cx, |this, cx| match item {
                            AppearancePageItem::Advanced { expanded } => {
                                settings_list_disclosure_header(
                                    ADVANCED_DISCLOSURE,
                                    "Advanced",
                                    expanded,
                                )
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.settings_controls.advanced =
                                        !this.settings_controls.advanced;
                                    cx.notify();
                                }))
                                .into_any_element()
                            }
                            item => this.appearance_item(item, position, narrow, window, cx),
                        })
                    },
                )
                .row_focus(row_focus)
                .when_some(fold, |page, fold| page.large_title(section.title(), fold))
                .into_any_element();
            }
            SettingsSection::Panes => {
                let [background, inactive, glow, margin] =
                    PaneControl::ALL.map(|control| self.pane_setting(control, cx));
                let gaps = SettingEntry::new("Pane gaps").control(
                    Switch::new("settings-pane-gaps")
                        .checked(self.preferences.gaps)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.preferences.gaps = !this.preferences.gaps;
                            this.preferences.save();
                            this.preferences.apply(&this.connection, window, cx);
                            cx.notify();
                        })),
                );
                return zz_ui::settings::panes_page(
                    zz_ui::settings::panes_preview::PanesPreview {
                        gaps: self.preferences.gaps,
                        margin: self.preferences.pane_margin,
                        inactive_opacity: self.preferences.pane_inactive_opacity,
                    },
                    [gaps],
                    [background],
                    [inactive, glow],
                    [margin],
                    cx,
                )
                .into_any_element();
            }
            SettingsSection::StatusBar => {
                let rows = [
                    SettingEntry::new("Sidebar").control(
                        Switch::new("settings-sidebar")
                            .checked(self.sidebar)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.sidebar = !this.sidebar;
                                this.preferences.sidebar = this.sidebar;
                                this.preferences.save();
                                cx.notify();
                            })),
                    ),
                    SettingEntry::new("Window badges")
                        .description("Bell and activity markers on window items")
                        .control(
                            Switch::new("settings-status-badges")
                                .checked(self.preferences.status_badges)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.preferences.status_badges =
                                        !this.preferences.status_badges;
                                    this.preferences.save();
                                    cx.notify();
                                })),
                        ),
                ];
                return zz_ui::settings::status_bar_preview::status_bar_page(
                    self.preferences.status_bar_settings(),
                    self.preferences.gaps,
                    SettingsStack::new().children(rows),
                    cx,
                )
                .into_any_element();
            }
            SettingsSection::Terminal => {
                return large(self.terminal_settings(narrow, cx)).into_any_element();
            }
            SettingsSection::Advanced => {
                let text_size = shown(Setting::SystemTextSize, phone).then(|| {
                    SettingEntry::new("Match system text size")
                        .description("Follows the iPadOS text size")
                        .control(
                            Switch::new("settings-system-text-size")
                                .checked(self.preferences.system_text_size)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.preferences.system_text_size =
                                        !this.preferences.system_text_size;
                                    this.preferences.save();
                                    this.preferences.apply(&this.connection, window, cx);
                                    cx.notify();
                                })),
                        )
                });
                return large(settings_scroll_column("settings-page"))
                    .children((!narrow).then(|| settings_page_heading(section.title(), cx)))
                    .when(shown(Setting::CommandPalette, phone), |page| {
                        page.child(
                            SettingsStack::titled("Command palette")
                                .children(self.palette_settings(cx)),
                        )
                    })
                    .when(
                        cfg!(any(target_os = "ios", target_os = "android")),
                        |page| {
                            page.child(
                                SettingsStack::titled("Display")
                                    .child(
                                        SettingEntry::new("Draw under the home indicator").control(
                                            Switch::new("settings-bottom-safe-area")
                                                .checked(self.preferences.extend_bottom_safe_area)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.preferences.extend_bottom_safe_area =
                                                        !this.preferences.extend_bottom_safe_area;
                                                    this.preferences.save();
                                                    cx.notify();
                                                })),
                                        ),
                                    )
                                    .children(text_size)
                                    .child(
                                        SettingEntry::new("Keep the screen awake")
                                            .description("While connected to a host")
                                            .control(
                                                Switch::new("settings-keep-awake")
                                                    .checked(self.preferences.keep_screen_awake)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.preferences.keep_screen_awake =
                                                            !this.preferences.keep_screen_awake;
                                                        this.preferences.save();
                                                        this.sync_idle_guard(cx);
                                                        cx.notify();
                                                    })),
                                            ),
                                    ),
                            )
                        },
                    )
                    .into_any_element();
            }
            SettingsSection::About => return large(about_page(cx)).into_any_element(),
            SettingsSection::Agents => {
                return large(self.providers_page((!narrow).then_some(section.title()), cx))
                    .into_any_element();
            }
            _ => {}
        }
        large(self.hosts_page(
            "settings-page",
            (!narrow).then_some(section.title()),
            narrow,
            cx,
        ))
        .into_any_element()
    }

    fn appearance_item(
        &self,
        item: AppearancePageItem,
        position: StackPosition,
        narrow: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entry = match item {
            AppearancePageItem::App(row @ ZzRow::ProgramColors) => Some(
                SettingEntry::new(row.title())
                    .description("Fill sidebar tiles with the running program's colors")
                    .control(
                        Switch::new("settings-program-colors")
                            .checked(self.preferences.program_colors)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.preferences.program_colors = !this.preferences.program_colors;
                                this.preferences.save();
                                this.preferences.apply(&this.connection, window, cx);
                            })),
                    ),
            ),
            AppearancePageItem::App(_) => None,
            item => kit_row(
                item,
                &self.preferences.appearance,
                self.preferences.saved(),
                &self.settings_controls.appearance,
                narrow,
                window,
                cx,
            ),
        };
        entry.map_or_else(
            || div().into_any_element(),
            |entry| entry.position(position).into_any_element(),
        )
    }

    fn pane_setting(&self, control: PaneControl, cx: &mut Context<Self>) -> SettingEntry {
        let needs_gaps = !matches!(control, PaneControl::PipOpacity | PaneControl::Opacity);
        let off = needs_gaps && !self.preferences.gaps;
        let description = if off {
            Some("Needs pane gaps")
        } else if control == PaneControl::Opacity {
            Some("1 turns dimming off")
        } else {
            None
        };
        let entry = SettingEntry::new(control.title())
            .when_some(description, SettingEntry::description)
            .disabled(off);
        entry.control(number_control(
            &self.settings_controls.panes[control as usize],
            cx,
        ))
    }

    fn terminal_settings(&self, narrow: bool, cx: &mut Context<Self>) -> SettingsScrollColumn {
        let available = cx.text_system().all_font_names();
        let appearance =
            localized_font_appearance(&self.connection.read(cx).core, &available, "Lilex", cx);
        let font = SettingEntry::new("Font").control(font_select(
            &self.settings_controls.terminal_font,
            "Terminal font",
            "Follow host",
            cx,
        ));
        let scale = SettingEntry::new("Font scale");
        let view = cx.entity().downgrade();
        let scale = with_control(
            scale,
            percent_slider(
                "settings-terminal-scale",
                "Font scale",
                self.preferences.terminal_font_scale,
                move |scale, window, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.preferences.terminal_font_scale = scale;
                        this.preferences.save();
                        this.preferences.apply(&this.connection, window, cx);
                    });
                },
                cx,
            ),
            narrow,
        );
        let host_title = if phone() {
            "Cursor shape and spacing"
        } else {
            "Colors, cursor, and spacing"
        };
        settings_scroll_column("settings-terminal")
            .children((!narrow).then(|| settings_page_heading("Terminal", cx)))
            .child(zz_ui::settings::settings_list_group_header("Preview", cx))
            .child(terminal_preview(appearance, cx))
            .child(SettingsStack::titled("Display").child(font).child(scale))
            .child(
                SettingsStack::titled("Host configuration").child(
                    SettingEntry::new(host_title)
                        .description("Set in the host's Ghostty configuration")
                        .control(
                            div()
                                .text_size(penna::rems_from_px(13.0))
                                .text_color(cx.theme().foreground.muted())
                                .child("Shared"),
                        ),
                ),
            )
    }
}

impl AppShell {
    fn palette_settings(&self, cx: &Context<Self>) -> Vec<SettingEntry> {
        vec![
            SettingEntry::new("Navigation layout").control(choice_select(
                &self.settings_controls.palette_layout,
                "Navigation layout",
                cx,
            )),
            SettingEntry::new("Command shortcuts").control(
                Switch::new("settings-palette-show-keys")
                    .checked(self.preferences.palette_show_keys)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.preferences.palette_show_keys = !this.preferences.palette_show_keys;
                        this.preferences.save();
                        cx.notify();
                    })),
            ),
            SettingEntry::new("Vim mode")
                .description("Esc for normal mode: j k move, l into, h up")
                .control(
                    Switch::new("settings-palette-vim-mode")
                        .checked(self.preferences.palette_vim_mode)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.preferences.palette_vim_mode = !this.preferences.palette_vim_mode;
                            this.preferences.save();
                            cx.notify();
                        })),
                ),
        ]
    }
}

fn about_page(cx: &mut Context<AppShell>) -> SettingsScrollColumn {
    static LOGOS: std::sync::LazyLock<[Arc<ala::Image>; 2]> = std::sync::LazyLock::new(|| {
        let [light, dark]: [&'static [u8]; 2] = if zz_protocol::app_identity::DEVELOPMENT {
            [
                include_bytes!("../../../../assets/zz-dev-light-512.png"),
                include_bytes!("../../../../assets/zz-dev-dark-512.png"),
            ]
        } else {
            [
                include_bytes!("../../../../assets/zz-light-512.png"),
                include_bytes!("../../../../assets/zz-dark-512.png"),
            ]
        };
        [light, dark].map(|bytes| {
            Arc::new(ala::Image::from_bytes(
                ala::ImageFormat::Png,
                bytes.to_vec(),
            ))
        })
    });
    let platform = if cfg!(target_family = "wasm") {
        "browser · wasm32".to_owned()
    } else {
        format!("{} · {}", std::env::consts::OS, std::env::consts::ARCH)
    };
    let copied = platform.clone();
    let logo = Arc::clone(&LOGOS[usize::from(cx.theme().mode.is_dark())]);
    settings_scroll_column("settings-about")
        .child(about_hero(
            &ZZ_ABOUT,
            ala::img(logo).size(px(ABOUT_LOGO_SIZE)),
            cx,
        ))
        .child(about_build_stack(
            &ZZ_ABOUT,
            platform,
            about_copy_button("settings-about-copy-build-info")
                .on(settings_card_ground(cx))
                .on_click(move |_, window, cx| {
                    use penna::WindowExt as _;
                    cx.write_to_clipboard(ala::ClipboardItem::new_string(
                        zz_ui::settings::about::build_info(&copied),
                    ));
                    window.push_notification(
                        penna::notification::Notification::success("Copied build information"),
                        cx,
                    );
                }),
            cx,
        ))
        .child(about_project_stack(&ZZ_ABOUT, cx))
}

pub(super) fn with_control(
    entry: SettingEntry,
    control: impl IntoElement,
    narrow: bool,
) -> SettingEntry {
    if narrow {
        entry.child(control)
    } else {
        entry.control(control)
    }
}

fn number_field_event(event: &InputEvent, window: &mut Window, cx: &mut App) {
    if matches!(event, InputEvent::PressEnter { .. }) {
        finish_field(window, cx);
    }
}

pub(super) fn finish_field(window: &mut Window, cx: &mut App) {
    if super::compact::keyboard_visible(window) {
        window.blur(cx);
    }
}

fn number_text(value: f32) -> String {
    let text = format!("{value:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" { "0" } else { text }.to_owned()
}

fn font_select(
    select: &Entity<SelectState<Vec<SettingsSelectItem>>>,
    title: &'static str,
    placeholder: &'static str,
    cx: &App,
) -> ala::Div {
    div().w(px(200.0)).max_w_full().flex_none().child(
        Select::new(select)
            .title(title)
            .placeholder(placeholder)
            .on(settings_card_ground(cx)),
    )
}

fn choice_select(
    select: &Entity<SelectState<Vec<SettingsSelectItem>>>,
    title: &'static str,
    cx: &App,
) -> Select<Vec<SettingsSelectItem>> {
    Select::new(select)
        .title(title)
        .on(settings_card_ground(cx))
}

fn number_control(input: &Entity<InputState>, cx: &App) -> ala::Div {
    let width = if penna::touch::CoarsePointer::get(cx) {
        110.0
    } else {
        120.0
    };
    div()
        .w(penna::rems_from_px(width))
        .max_w_full()
        .flex_none()
        .child(NumberInput::new(input).on(settings_card_ground(cx)))
}

fn terminal_preview(appearance: zz_terminal::TerminalAppearance, _: &App) -> AnyElement {
    TerminalPreview { appearance }.into_any_element()
}

#[derive(IntoElement)]
struct TerminalPreview {
    appearance: zz_terminal::TerminalAppearance,
}

impl ala::RenderOnce for TerminalPreview {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        use zz_ui::terminal::{
            RowRenderCache, TerminalRenderInput, terminal_background, terminal_font_for_style,
        };
        let mut appearance = self.appearance;
        let sample = sample_terminal_viewport(&appearance);
        appearance.local_colors = crate::terminal::phone_colors(&sample, &appearance.palette, cx);
        let viewport = Arc::new(match appearance.local_colors {
            Some(local) => local.viewport(&sample),
            None => sample,
        });
        let appearance = Arc::new(appearance);
        let cache = Rc::new(RefCell::new(RowRenderCache::default()));
        let paint_cache = Rc::clone(&cache);
        let background = if phone() {
            cx.theme()
                .layer(zz_ui::roles::SETTINGS_CARD, penna::PaintState::Rest)
                .fill
        } else {
            pane_color(
                cx.theme(),
                terminal_background(appearance.background, appearance.background_opacity),
            )
        };
        let font = terminal_font_for_style(&appearance, &cx.theme().mono_font_family, false, false);
        let font_size = px(appearance.font_size_points);
        let probe = window.text_system().shape_line(
            "m".into(),
            font_size,
            &[ala::TextRun {
                len: 1,
                font: font.clone(),
                color: cx.theme().foreground,
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        );
        let font_id = probe.runs.first().map_or_else(
            || window.text_system().resolve_font(&font),
            |run| run.font_id,
        );
        let natural_height =
            probe.ascent + probe.descent + window.text_system().line_gap(font_id, font_size);
        let scale = window.scale_factor();
        let cell_height = (appearance
            .cell_height_adjustment
            .apply(f32::from(natural_height))
            .max(1.0)
            * scale)
            .round()
            / scale;
        let height = cell_height * 8.0 + appearance.padding_top + appearance.padding_bottom + 2.0;
        settings_preview(background, cx)
            .w_full()
            .h(px(height.max(190.0)))
            .flex_none()
            .child(
                div()
                    .size_full()
                    .pl(px(appearance.padding_left))
                    .pr(px(appearance.padding_right))
                    .pt(px(appearance.padding_top))
                    .pb(px(appearance.padding_bottom))
                    .font(font)
                    .text_size(font_size)
                    .line_height(px(appearance
                        .cell_height_adjustment
                        .apply(f32::from(font_size))
                        .max(1.0)))
                    .child(
                        canvas(
                            move |bounds, window, cx| {
                                cache.borrow_mut().prepaint(
                                    TerminalRenderInput {
                                        viewport: &viewport,
                                        row_revisions: &[1, 2, 3, 4, 5, 6, 7, 8],
                                        revision_epoch: 0,
                                        history: None,
                                        images: None,
                                        local_scroll_target: None,
                                        scroll_pixel_offset: px(0.0),
                                        overscroll: px(0.0),
                                        extra_height: px(0.0),
                                        command_output: true,
                                        appearance: &appearance,
                                        appearance_hash: appearance.stable_hash(),
                                        text_opacity: 1.0,
                                        focused: true,
                                        cursor_blink_visible: true,
                                        marked_text: None,
                                        rows_above: None,
                                        rows_below: None,
                                    },
                                    bounds,
                                    window,
                                    cx,
                                )
                            },
                            move |bounds, mut paint, window, cx| {
                                paint_cache
                                    .borrow_mut()
                                    .paint(&mut paint, bounds, window, cx);
                            },
                        )
                        .size_full(),
                    ),
            )
    }
}

fn sample_terminal_viewport(
    appearance: &zz_terminal::TerminalAppearance,
) -> zz_terminal::TerminalViewport {
    use zz_terminal::{
        ATTR_BOLD, ATTR_ITALIC, CellWidth, Cursor, PackedCell, PackedStyle, SessionStatus,
        TerminalViewport, UnderlineStyle,
    };
    const COLUMNS: u16 = 72;
    const ROWS: u16 = 8;
    let mut viewport =
        TerminalViewport::blank_with_appearance(COLUMNS, ROWS, SessionStatus::Running, appearance);
    let mut styles = vec![PackedStyle::new(
        appearance.foreground,
        appearance.background,
        None,
        0,
        UnderlineStyle::None,
    )];
    for index in 0..16 {
        styles.push(PackedStyle::new(
            appearance.palette[index],
            appearance.background,
            None,
            0,
            UnderlineStyle::None,
        ));
    }
    styles.push(PackedStyle::new(
        appearance.foreground,
        appearance.background,
        None,
        ATTR_BOLD,
        UnderlineStyle::None,
    ));
    styles.push(PackedStyle::new(
        appearance.foreground,
        appearance.background,
        None,
        ATTR_ITALIC,
        UnderlineStyle::None,
    ));
    let cells = Arc::make_mut(&mut viewport.cells);
    let mut write = |row: usize, column: usize, text: &str, style: u16| {
        for (offset, character) in text.chars().take(usize::from(COLUMNS) - column).enumerate() {
            cells[row * usize::from(COLUMNS) + column + offset] =
                PackedCell::new(character as u32, style, CellWidth::Narrow);
        }
    };
    write(0, 0, "~/projects/zz", 5);
    write(0, 15, "main", 3);
    write(1, 0, ">", 3);
    write(1, 2, "printf 'hello, terminal\\n'", 0);
    write(2, 0, "hello, terminal", 17);
    write(3, 0, "Regular", 0);
    write(3, 10, "Bold", 17);
    write(3, 17, "Italic", 18);
    for index in 0..8 {
        write(5, index * 4, "██", u16::try_from(index + 1).unwrap());
        write(6, index * 4, "██", u16::try_from(index + 9).unwrap());
    }
    write(7, 0, ">", 3);
    Arc::make_mut(&mut viewport.dictionary).styles = styles.into();
    viewport.cursor = Some(Cursor::new(
        2,
        7,
        true,
        true,
        false,
        appearance.cursor_style,
        appearance.cursor_color,
    ));
    viewport
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PaneControl {
    PipOpacity,
    Opacity,
    Glow,
    Margin,
}

impl PaneControl {
    const ALL: [Self; 4] = [Self::PipOpacity, Self::Opacity, Self::Glow, Self::Margin];

    fn value(self, preferences: &mut Preferences) -> &mut f32 {
        match self {
            Self::PipOpacity => &mut preferences.pip_background_opacity,
            Self::Opacity => &mut preferences.pane_inactive_opacity,
            Self::Glow => &mut preferences.pane_glow_strength,
            Self::Margin => &mut preferences.pane_margin,
        }
    }

    fn limits(self) -> (f32, f32, f64) {
        match self {
            Self::PipOpacity | Self::Opacity => (0.0, 1.0, 0.05),
            Self::Glow => (0.0, 2.0, 0.05),
            Self::Margin => (0.0, 32.0, 0.5),
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::PipOpacity => "PiP background opacity",
            Self::Opacity => "Inactive pane opacity",
            Self::Glow => "Selected pane contrast",
            Self::Margin => "Pane margin",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Preferences, SECTIONS, Setting, interface_zoom, number_text, sample_terminal_viewport,
        shown,
    };
    use zz_terminal::{ATTR_BOLD, ATTR_ITALIC, CursorStyle, TerminalAppearance};
    use zz_ui::settings::SettingsSection;

    #[test]
    fn phone_hides_pane_status_bar_and_system_text_settings_only() {
        let visible = |phone| {
            SECTIONS
                .into_iter()
                .filter(|section| shown(Setting::Section(*section), phone))
                .collect::<Vec<_>>()
        };
        assert_eq!(visible(false), SECTIONS);
        assert_eq!(
            visible(true),
            [
                SettingsSection::Appearance,
                SettingsSection::Terminal,
                SettingsSection::Hosts,
                SettingsSection::Agents,
                SettingsSection::Advanced,
                SettingsSection::About,
            ]
        );
        for setting in [Setting::SystemTextSize, Setting::CommandPalette] {
            assert!(shown(setting, false));
            assert!(!shown(setting, true));
        }
    }

    #[test]
    fn phone_scale_is_ui_zoom_over_the_capped_system_text_size() {
        let mut zoomed = Preferences {
            system_text_size: false,
            ..Preferences::default()
        };
        zoomed.appearance.zoom = 1.5;
        assert_eq!(interface_zoom(&zoomed, 1.12, false), 1.5);
        assert!((interface_zoom(&zoomed, 1.12, true) - 1.68).abs() < 1e-6);
        assert!((interface_zoom(&zoomed, 0.82, true) - 1.23).abs() < 1e-6);
        assert!((interface_zoom(&zoomed, 1.9, true) - 2.025).abs() < 1e-6);
        let mut following = Preferences::default();
        following.appearance.zoom = 1.5;
        assert!((interface_zoom(&following, 1.2, false) - 1.8).abs() < 1e-6);
    }

    #[test]
    fn number_fields_hide_float_noise() {
        assert_eq!(number_text(1.05_f32 * 100.0), "105");
        assert_eq!(number_text(0.85_f32 * 100.0), "85");
        assert_eq!(number_text(13.5), "13.5");
        assert_eq!(number_text(0.7), "0.7");
        assert_eq!(number_text(0.05), "0.05");
        assert_eq!(number_text(0.0), "0");
        assert_eq!(number_text(-0.001), "0");
    }

    #[test]
    fn terminal_preview_uses_host_palette_styles_and_cursor() {
        let mut appearance = TerminalAppearance::default();
        appearance.palette[1] = appearance.palette[6];
        appearance.cursor_style = CursorStyle::Underline;
        appearance.cursor_color = appearance.palette[3];
        let viewport = sample_terminal_viewport(&appearance);
        let swatch = viewport.cell(5, 4).unwrap();
        assert_eq!(
            viewport.style(swatch).unwrap().foreground(),
            appearance.palette[1]
        );
        for (column, style) in [(10, ATTR_BOLD), (17, ATTR_ITALIC)] {
            let cell = viewport.cell(3, column).unwrap();
            assert_eq!(viewport.style(cell).unwrap().attributes() & style, style);
        }
        let cursor = viewport.cursor.unwrap();
        assert_eq!(cursor.style(), appearance.cursor_style);
        assert_eq!(cursor.color(), appearance.cursor_color);
        assert!(cursor.column() < viewport.columns && cursor.row() < viewport.rows);
    }

    #[cfg(not(target_family = "wasm"))]
    #[ala::test]
    fn apply_draws_a_saved_theme_in_its_colors_materials_and_roles(cx: &mut ala::TestAppContext) {
        use ala::{AppContext as _, px};
        use penna::{ActiveTheme as _, ThemeMode};

        cx.update(zz_ui::init);
        let mut preferences = Preferences::default();
        preferences.appearance.mode = Some("dark".into());
        preferences
            .import_theme(
                r##"{"name":"Neon","materials":{"radius":3,"outline":0.4,"float":{"strength":0.5}},"roles":{"button":{"shape":"pill"}},"dark":{"accent":"#ff00ff"}}"##,
            )
            .unwrap();
        let expected = preferences.interface_theme().colors(ThemeMode::Dark).accent;
        let window = cx.add_empty_window();
        let connection = window.update(|_, cx| cx.new(crate::connection::Connection::new));
        window.update(|window, cx| {
            preferences.apply(&connection, window, cx);
            let theme = cx.theme();
            assert_eq!(theme.mode, ThemeMode::Dark);
            assert_eq!(theme.accent, expected);
            assert_eq!(theme.radius, px(3.0));
            assert_eq!(theme.float_shadow.strength, 0.5);
            assert_eq!(theme.outline, px(0.4));
            assert_eq!(
                theme.style(penna::Kind::Button.role()).shape,
                penna::Shape::Pill
            );
        });
        preferences.appearance.overrides.radius = Some(5.0);
        preferences.appearance.overrides.accent = Some("green".into());
        window.update(|window, cx| {
            preferences.apply(&connection, window, cx);
            let theme = cx.theme();
            assert_eq!(
                Some(theme.accent),
                zz_ui::interface_theme::accent_color("green", ThemeMode::Dark)
            );
            assert_eq!(theme.radius, px(5.0));
            assert_eq!(theme.outline, px(0.4));
        });
    }
}
