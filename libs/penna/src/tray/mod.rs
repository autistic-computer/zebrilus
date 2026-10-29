#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

use std::sync::Arc;

use async_channel::Sender;
use parking_lot::Mutex;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TrayEvent {
    Activate,
    Action(String),
    Available(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub enum MenuEntry {
    Separator,
    Item {
        label: String,
        action: Option<String>,
        checked: Option<bool>,
    },
}

impl MenuEntry {
    pub fn item(label: impl Into<String>, action: impl Into<String>) -> Self {
        Self::Item {
            label: label.into(),
            action: Some(action.into()),
            checked: None,
        }
    }

    pub fn label(label: impl Into<String>) -> Self {
        Self::Item {
            label: label.into(),
            action: None,
            checked: None,
        }
    }

    pub fn check(label: impl Into<String>, action: impl Into<String>, checked: bool) -> Self {
        Self::Item {
            label: label.into(),
            action: Some(action.into()),
            checked: Some(checked),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Menu(Arc<Mutex<Vec<MenuEntry>>>);

impl Menu {
    pub fn entries(&self) -> Vec<MenuEntry> {
        self.0.lock().clone()
    }

    pub fn set(&self, entries: Vec<MenuEntry>) -> bool {
        let mut current = self.0.lock();
        if *current == entries {
            return false;
        }
        *current = entries;
        true
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayIcon {
    pub png: Vec<u8>,
    pub template: bool,
}

#[derive(Clone, Debug)]
pub struct TrayOptions {
    pub id: String,
    pub tooltip: String,
    pub icon: TrayIcon,
    pub icon_name: Option<String>,
    pub menu_on_click: bool,
}

pub struct Tray {
    #[cfg(target_os = "linux")]
    backend: linux::Service,
    #[cfg(target_os = "macos")]
    backend: macos::StatusItem,
    #[cfg(target_os = "windows")]
    backend: windows::NotifyIcon,
}

impl Tray {
    pub fn spawn(options: TrayOptions, menu: Menu, events: Sender<TrayEvent>) -> Option<Self> {
        #[cfg(target_os = "linux")]
        let backend = linux::spawn(options, menu, events)?;
        #[cfg(target_os = "macos")]
        let backend = macos::spawn(options, menu, events)?;
        #[cfg(target_os = "windows")]
        let backend = windows::spawn(options, menu, events)?;
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            let _ = (options, menu, events);
            return None;
        }
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        Some(Self { backend })
    }

    pub fn menu_changed(&self) {
        #[cfg(target_os = "linux")]
        self.backend.menu_changed();
    }

    pub fn set_badge(&self, badge: &str) {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        self.backend.set_badge(badge);
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let _ = badge;
    }

    pub fn set_look(&self, tooltip: &str, icon: &TrayIcon) {
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        self.backend.set_look(tooltip, icon);
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        let _ = (tooltip, icon);
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn decode(icon: &TrayIcon) -> Option<image::RgbaImage> {
    match image::load_from_memory_with_format(&icon.png, image::ImageFormat::Png) {
        Ok(image) => Some(image.into_rgba8()),
        Err(error) => {
            log::warn!(target: "penna::tray", "could not decode the tray icon: {error}");
            None
        }
    }
}

fn send(events: &Sender<TrayEvent>, event: TrayEvent) {
    if let Err(error) = events.try_send(event) {
        log::warn!(target: "penna::tray", "dropped a tray event: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_reports_only_real_changes() {
        let menu = Menu::default();
        assert!(menu.set(vec![MenuEntry::item("Open", "open")]));
        assert!(!menu.set(vec![MenuEntry::item("Open", "open")]));
        assert!(menu.set(vec![MenuEntry::check("Sharing", "sharing", true)]));
        assert_eq!(
            menu.entries(),
            vec![MenuEntry::Item {
                label: "Sharing".into(),
                action: Some("sharing".into()),
                checked: Some(true),
            }]
        );
    }
}
