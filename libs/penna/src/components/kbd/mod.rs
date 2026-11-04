use crate::Space;
use ala::{
    Action, AnyElement, App, AsKeystroke as _, ElementId, FocusHandle, IntoElement, KeyContext,
    Keystroke, ParentElement as _, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled, Window, px,
};

use crate::{
    ActiveTheme as _, AuthoredElement as _, Ground, Icon, IconName, Layer, PaintState, Sizable,
    Size, StyledExt as _, Surface, SurfaceRole, TextStyle, Theme, h_flex,
};

const ICON_SCALE: f32 = 1.2;

#[cfg(not(target_family = "wasm"))]
fn apple() -> bool {
    cfg!(any(target_os = "macos", target_os = "ios"))
}

#[cfg(target_family = "wasm")]
fn apple() -> bool {
    static APPLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *APPLE.get_or_init(|| {
        web_sys::window()
            .and_then(|window| window.navigator().user_agent().ok())
            .is_some_and(|agent| agent.contains("Mac"))
    })
}

fn separator() -> &'static str {
    if apple() { "" } else { "+" }
}

fn modifiers() -> [&'static str; 4] {
    if apple() {
        ["⌃", "⌥", "⇧", "⌘"]
    } else {
        ["Ctrl", "Alt", "Shift", "Win"]
    }
}

#[derive(IntoElement, Clone, Debug)]
pub struct Kbd {
    style: StyleRefinement,
    stroke: Keystroke,
    lowercase: bool,
    on: Option<Ground>,
    size: Size,
    id: Option<ElementId>,
}

impl Kbd {
    #[must_use]
    pub fn new(stroke: Keystroke) -> Self {
        Self {
            style: StyleRefinement::default(),
            stroke,
            lowercase: false,
            on: None,
            size: Size::default(),
            id: None,
        }
    }

    #[must_use]
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }

    #[must_use]
    pub fn lowercase(mut self) -> Self {
        self.lowercase = true;
        self
    }

    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.on = Some(ground);
        self
    }

    fn cap(&self) -> Keycap {
        let glyphs = KeyGlyphs {
            stroke: self.stroke.clone(),
            lowercase: self.lowercase,
        };
        Keycap {
            label: glyphs.into_any_element(),
            name: self.text().into(),
            id: self.id.clone(),
            on: self.on,
            size: self.size,
            style: self.style.clone(),
        }
    }

    #[must_use]
    pub fn text(&self) -> String {
        Self::format_key(&self.stroke, self.lowercase)
    }

    #[must_use]
    pub fn binding_for_action(
        action: &dyn Action,
        context: Option<&str>,
        window: &Window,
    ) -> Option<Self> {
        let binding = match context.and_then(|context| KeyContext::parse(context).ok()) {
            Some(context) => {
                window.highest_precedence_binding_for_action_in_context(action, context)
            }
            None => window.highest_precedence_binding_for_action(action),
        }?;
        binding
            .keystrokes()
            .first()
            .map(|key| Self::new(key.as_keystroke().clone()))
    }

    #[must_use]
    pub fn binding_for_action_in(
        action: &dyn Action,
        focus_handle: &FocusHandle,
        window: &Window,
    ) -> Option<Self> {
        let binding = window.highest_precedence_binding_for_action_in(action, focus_handle)?;
        binding
            .keystrokes()
            .first()
            .map(|key| Self::new(key.as_keystroke().clone()))
    }

    #[must_use]
    pub fn format(key: &Keystroke) -> String {
        Self::format_key(key, false)
    }

    pub fn format_key(key: &Keystroke, lowercase: bool) -> String {
        let m = &key.modifiers;
        let [control, alt, shift, platform] = modifiers();
        let mut parts: Vec<&str> = [
            m.control.then_some(control),
            m.alt.then_some(alt),
            m.shift.then_some(shift),
            m.platform.then_some(platform),
        ]
        .into_iter()
        .flatten()
        .collect();

        let key = key_symbol(key.key.as_str());
        let key = if lowercase { key.to_lowercase() } else { key };
        parts.push(&key);
        parts.join(separator())
    }
}

fn key_symbol(key: &str) -> String {
    let apple = apple();
    let platform = |mac: &str, other: &str| if apple { mac } else { other }.to_string();
    match key {
        "ctrl" => platform("⌃", "Ctrl"),
        "alt" => platform("⌥", "Alt"),
        "shift" => platform("⇧", "Shift"),
        "cmd" => platform("⌘", "Win"),
        "backspace" => platform("⌫", "Backspace"),
        "delete" => platform("⌫", "Delete"),
        "escape" => platform("⎋", "Esc"),
        "enter" => platform("⏎", "Enter"),
        "left" => platform("←", "Left"),
        "right" => platform("→", "Right"),
        "up" => platform("↑", "Up"),
        "down" => platform("↓", "Down"),
        "space" => "Space".to_string(),
        "pagedown" => "Page Down".to_string(),
        "pageup" => "Page Up".to_string(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        }
    }
}

#[must_use]
pub fn key_icon(key: &str) -> Option<IconName> {
    Some(match key {
        "ctrl" if apple() => IconName::KeyControl,
        "ctrl" => IconName::KeyControlIso,
        "alt" if apple() => IconName::KeyOption,
        "alt" => IconName::KeyAltIso,
        "shift" => IconName::KeyShift,
        "cmd" if apple() => IconName::KeyCommand,
        "escape" => IconName::KeyEscape,
        "tab" if apple() => IconName::KeyTab,
        "tab" => IconName::KeyTabIso,
        "enter" => IconName::KeyReturn,
        "backspace" => IconName::KeyBackspace,
        "up" => IconName::ArrowUp,
        "down" => IconName::ArrowDown,
        "left" => IconName::ArrowLeft,
        "right" => IconName::ArrowRight,
        _ => return None,
    })
}

#[derive(IntoElement, Clone, Debug)]
pub struct KeyGlyphs {
    stroke: Keystroke,
    lowercase: bool,
}

impl KeyGlyphs {
    #[must_use]
    pub const fn new(stroke: Keystroke) -> Self {
        Self {
            stroke,
            lowercase: false,
        }
    }

    #[must_use]
    pub const fn lowercase(mut self) -> Self {
        self.lowercase = true;
        self
    }

    fn parts(&self) -> Vec<Result<IconName, String>> {
        let m = &self.stroke.modifiers;
        let [control, alt, shift, platform] = modifiers();
        let modifiers = [
            (m.control, "ctrl", control),
            (m.alt, "alt", alt),
            (m.shift, "shift", shift),
            (m.platform, "cmd", platform),
        ];
        let key = self.stroke.key.as_str();
        let mut parts: Vec<Result<IconName, String>> = modifiers
            .into_iter()
            .filter(|(held, ..)| *held)
            .map(|(_, name, text)| key_icon(name).ok_or_else(|| text.to_owned()))
            .collect();
        parts.push(key_icon(key).ok_or_else(|| {
            let name = key_symbol(key);
            if self.lowercase {
                name.to_lowercase()
            } else {
                name
            }
        }));
        let mut merged: Vec<Result<IconName, String>> = Vec::with_capacity(parts.len());
        for part in parts {
            match (merged.last_mut(), part) {
                (Some(Err(text)), Err(next)) => {
                    text.push_str(separator());
                    text.push_str(&next);
                }
                (_, part) => merged.push(part),
            }
        }
        merged
    }
}

impl RenderOnce for KeyGlyphs {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let text = window.text_style().font_size.to_pixels(window.rem_size());
        let icon = (text * ICON_SCALE).round();
        h_flex()
            .flex_none()
            .items_center()
            .gap(Space::S2)
            .children(self.parts().into_iter().map(|part| -> AnyElement {
                match part {
                    Ok(name) => Icon::new(name).size(icon).into_any_element(),
                    Err(text) => SharedString::from(text).into_any_element(),
                }
            }))
    }
}

impl Styled for Kbd {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Sizable for Kbd {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl RenderOnce for Kbd {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.cap()
    }
}

#[derive(IntoElement)]
pub struct Keycap {
    label: AnyElement,
    name: SharedString,
    id: Option<ElementId>,
    on: Option<Ground>,
    size: Size,
    style: StyleRefinement,
}

impl Keycap {
    #[must_use]
    pub fn new(label: impl Into<SharedString>) -> Self {
        let name = label.into();
        Self {
            label: name.clone().into_any_element(),
            name,
            id: None,
            on: None,
            size: Size::default(),
            style: StyleRefinement::default(),
        }
    }

    #[must_use]
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }

    fn surface_layer(&self, theme: &Theme) -> Layer {
        theme.layer(
            Surface::new(SurfaceRole::KEYCAP).on_maybe(self.on),
            PaintState::Rest,
        )
    }

    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.on = Some(ground);
        self
    }
}

impl Sizable for Keycap {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Styled for Keycap {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Keycap {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let (height, text) = match self.size {
            Size::Small => (16.0, TextStyle::Micro),
            Size::Medium | Size::Size(_) => (18.0, TextStyle::Caption),
        };
        let layer = self.surface_layer(cx.theme());
        let cap = h_flex()
            .flex_none()
            .justify_center()
            .h(px(height))
            .min_w(px(height))
            .px(Space::S4)
            .layer(&layer)
            .text_color(layer.muted_text())
            .type_style(text, cx)
            .refine_style(&self.style)
            .child(self.label);
        match self.id {
            Some(id) => cap
                .author_id(id)
                .role(Role::Label)
                .aria_label(self.name)
                .into_any_element(),
            None => cap.into_any_element(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Kbd, KeyGlyphs, Keycap};
    use crate::{Elevation, Theme, ThemeColor};
    use ala::Keystroke;

    #[test]
    fn kbd_and_keycap_draw_one_cap_on_the_same_ground() {
        let theme = Theme::from(&*ThemeColor::dark());
        let stroke = Keystroke::parse("cmd-k").unwrap();
        assert_eq!(
            Kbd::new(stroke.clone()).cap().surface_layer(&theme),
            Keycap::new("⌘K").surface_layer(&theme)
        );
        let panel = theme.surface(Elevation::Panel);
        let on_panel = Kbd::new(stroke).on(panel).cap().surface_layer(&theme);
        assert_eq!(on_panel, Keycap::new("⌘K").on(panel).surface_layer(&theme));
        assert_ne!(on_panel, Keycap::new("⌘K").surface_layer(&theme));
    }

    #[test]
    fn lowercase_touches_only_the_key_glyph() {
        let key = |s: &str| Keystroke::parse(s).unwrap();
        assert_eq!(Kbd::format(&key("t")), "T");
        assert_eq!(Kbd::format_key(&key("t"), true), "t");
        let modified = Kbd::format_key(&key("ctrl-a"), true);
        assert!(modified.ends_with('a'), "{modified}");
        assert_eq!(
            modified.trim_end_matches('a'),
            Kbd::format(&key("ctrl-a")).trim_end_matches('A')
        );
    }

    #[test]
    fn glyphs_draw_keys_with_icons_and_name_the_rest() {
        use crate::IconName;
        let parts = |s: &str| KeyGlyphs::new(Keystroke::parse(s).unwrap()).parts();
        let control = if cfg!(any(target_os = "macos", target_os = "ios")) {
            IconName::KeyControl
        } else {
            IconName::KeyControlIso
        };
        assert_eq!(parts("ctrl-c"), [Ok(control.clone()), Err("C".to_owned())]);
        assert_eq!(
            KeyGlyphs::new(Keystroke::parse("ctrl-b").unwrap())
                .lowercase()
                .parts(),
            [Ok(control), Err("b".to_owned())]
        );
        assert_eq!(parts("up"), [Ok(IconName::ArrowUp)]);
        assert_eq!(parts("enter"), [Ok(IconName::KeyReturn)]);
        assert_eq!(parts("1"), [Err("1".to_owned())]);
        if cfg!(any(target_os = "macos", target_os = "ios")) {
            assert_eq!(
                parts("cmd-k"),
                [Ok(IconName::KeyCommand), Err("K".to_owned())]
            );
        } else {
            assert_eq!(parts("cmd-k"), [Err("Win+K".to_owned())]);
        }
    }

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    #[test]
    fn formats_apple_glyphs() {
        let f = |s: &str| Kbd::format(&Keystroke::parse(s).unwrap());
        assert_eq!(f("cmd-a"), "⌘A");
        assert_eq!(f("cmd--"), "⌘-");
        assert_eq!(f("cmd-+"), "⌘+");
        assert_eq!(f("cmd-enter"), "⌘⏎");
        assert_eq!(f("secondary-f12"), "⌘F12");
        assert_eq!(f("shift-pagedown"), "⇧Page Down");
        assert_eq!(f("shift-pageup"), "⇧Page Up");
        assert_eq!(f("shift-space"), "⇧Space");
        assert_eq!(f("cmd-ctrl-a"), "⌃⌘A");
        assert_eq!(f("cmd-alt-backspace"), "⌥⌘⌫");
        assert_eq!(f("shift-delete"), "⇧⌫");
        assert_eq!(f("cmd-ctrl-shift-a"), "⌃⇧⌘A");
        assert_eq!(f("cmd-ctrl-shift-alt-a"), "⌃⌥⇧⌘A");
    }

    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    #[test]
    fn formats_named_keys() {
        let f = |s: &str| Kbd::format(&Keystroke::parse(s).unwrap());
        assert_eq!(f("a"), "A");
        assert_eq!(f("ctrl-a"), "Ctrl+A");
        assert_eq!(f("shift-space"), "Shift+Space");
        assert_eq!(f("ctrl-alt-a"), "Ctrl+Alt+A");
        assert_eq!(f("ctrl-alt-shift-a"), "Ctrl+Alt+Shift+A");
        assert_eq!(f("ctrl-alt-shift-win-a"), "Ctrl+Alt+Shift+Win+A");
        assert_eq!(f("ctrl-shift-backspace"), "Ctrl+Shift+Backspace");
        assert_eq!(f("alt-delete"), "Alt+Delete");
        assert_eq!(f("alt-tab"), "Alt+Tab");
    }
}
