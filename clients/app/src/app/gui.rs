use ala::{Context, Window};
use zz_client::gui_actions::{GuiOffer, PANE_GAPS, SIDEBAR_LAYOUT, THEME, TITLEBAR_MODE};
use zz_ui::command::{GuiEvent, theme_offer};

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
            theme_offer(&super::settings::theme_picks(preferences)),
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
            GuiEvent::Preview { action, value } if action.id == THEME => {
                self.gui_previewing = true;
                let mut preview = self.preferences.clone();
                preview.set_interface_theme(value);
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
                    THEME => {
                        self.preferences.set_interface_theme(value);
                        self.theme_changed(window, cx);
                    }
                    SIDEBAR_LAYOUT => {
                        self.preferences.flat_sidebar = on;
                        self.preferences.save();
                    }
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
