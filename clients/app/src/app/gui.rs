use ala::{Context, Window};
use zz_client::gui_actions::{ACCENT, GLASS, GuiOffer, PANE_GAPS, SIDEBAR_LAYOUT, TITLEBAR_MODE};
use zz_ui::{
    command::GuiEvent,
    interface_theme::{AppearanceChange, Glass},
};

use super::AppShell;

impl AppShell {
    pub(super) fn gui_offers(&self, window: &Window) -> Vec<GuiOffer> {
        let preferences = &self.preferences;
        [
            (!Self::narrow(window))
                .then(|| {
                    GuiOffer::fixed(
                        TITLEBAR_MODE,
                        if self.sidebar { "sidebar" } else { "titlebar" },
                    )
                })
                .flatten(),
            GuiOffer::toggle(PANE_GAPS, preferences.gaps),
            GuiOffer::fixed(
                GLASS,
                if preferences.appearance.glass() == Glass::None {
                    "none"
                } else {
                    "liquid"
                },
            ),
            GuiOffer::fixed(ACCENT, &preferences.appearance.accent()),
            GuiOffer::toggle(SIDEBAR_LAYOUT, preferences.flat_sidebar),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    pub(super) fn on_gui_event(
        &mut self,
        event: &GuiEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            GuiEvent::Preview { action, value } if action.id == ACCENT => {
                self.gui_previewing = true;
                let mut preview = self.preferences.clone();
                preview.change_appearance(AppearanceChange::Accent(Some(value.clone())));
                preview.apply(&self.connection, window, cx);
            }
            GuiEvent::Preview { .. } => {}
            GuiEvent::Revert => self.revert_gui_preview(window, cx),
            GuiEvent::Apply { action, value } => {
                self.gui_previewing = false;
                let on = value == "true";
                match action.id {
                    TITLEBAR_MODE => {
                        if self.sidebar != (value == "sidebar") {
                            self.toggle_sidebar(window, cx);
                        }
                    }
                    PANE_GAPS => {
                        self.preferences.gaps = on;
                        self.preferences.save();
                        self.preferences.apply(&self.connection, window, cx);
                    }
                    GLASS => {
                        let glass = if value == "none" {
                            Glass::None
                        } else {
                            Glass::Liquid
                        };
                        self.preferences
                            .change_appearance(AppearanceChange::Glass(glass));
                        self.theme_changed(window, cx);
                    }
                    ACCENT => {
                        self.preferences
                            .change_appearance(AppearanceChange::Accent(Some(value.clone())));
                        self.theme_changed(window, cx);
                    }
                    SIDEBAR_LAYOUT => super::sidebar::set_chat_view(self, on, cx),
                    _ => {}
                }
                cx.notify();
            }
        }
    }

    pub(super) fn revert_gui_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.gui_previewing) {
            self.preferences.apply(&self.connection, window, cx);
        }
    }
}
