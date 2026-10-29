pub(crate) mod controller;
pub(crate) mod element;
pub(crate) mod environment;
pub(crate) mod inline;
#[cfg(target_os = "macos")]
pub(crate) mod macos_surface;
pub(crate) mod recent_pages;
pub(crate) mod screenshot;
#[cfg(not(target_os = "windows"))]
pub(crate) mod tui;
#[cfg(target_os = "macos")]
pub(crate) mod underlay;
pub(crate) mod view;

use std::io;

use zz_browser::{BrowserKey, BrowserProfilePaths, KeyAction, KeyInput, Modifiers};
use zz_terminal::{KeyAction as Action, KeyCode as Key};

const DEFAULT_PROFILE_DIRECTORY: &str = "zz-default";

const _: () = assert!(zz_browser::MAX_ARTIFACT_PATH_BYTES == zz_protocol::MAX_ARTIFACT_PATH_BYTES);
const _: () =
    assert!(zz_browser::MAX_ARTIFACT_REPLY_BYTES == zz_protocol::MAX_ARTIFACT_REPLY_BYTES);
const _: () = assert!(
    zz_browser::MAX_BROWSER_PROFILE_NAME_BYTES == zz_protocol::MAX_BROWSER_PROFILE_NAME_BYTES
);
const _: () = assert!(zz_browser::MAX_FAVICON_BYTES == zz_protocol::MAX_FAVICON_BYTES);

pub(crate) fn profile_paths() -> io::Result<BrowserProfilePaths> {
    Ok(BrowserProfilePaths::under(
        &zz_protocol::app_identity::browser_data_dir()?,
        DEFAULT_PROFILE_DIRECTORY,
    ))
}

pub(crate) fn browser_key_input(input: &zz_terminal::KeyInput) -> KeyInput {
    KeyInput {
        action: if input.action == Action::Release {
            KeyAction::Release
        } else {
            KeyAction::Press
        },
        key: match input.key {
            Key::Character(value) => BrowserKey::Character(value),
            Key::Backspace => BrowserKey::Backspace,
            Key::Enter => BrowserKey::Enter,
            Key::Tab => BrowserKey::Tab,
            Key::Escape => BrowserKey::Escape,
            Key::Delete => BrowserKey::Delete,
            Key::Insert => BrowserKey::Insert,
            Key::Home => BrowserKey::Home,
            Key::End => BrowserKey::End,
            Key::PageUp => BrowserKey::PageUp,
            Key::PageDown => BrowserKey::PageDown,
            Key::ArrowUp => BrowserKey::ArrowUp,
            Key::ArrowDown => BrowserKey::ArrowDown,
            Key::ArrowLeft => BrowserKey::ArrowLeft,
            Key::ArrowRight => BrowserKey::ArrowRight,
            Key::Function(value) => BrowserKey::Function(value),
            Key::Unidentified | Key::User(_) => BrowserKey::Unidentified,
        },
        modifiers: Modifiers::new(
            input.modifiers.shift(),
            input.modifiers.control(),
            input.modifiers.alt(),
            input.modifiers.platform(),
        )
        .with_repeat(input.action == Action::Repeat),
    }
}
