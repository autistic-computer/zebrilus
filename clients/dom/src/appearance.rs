use serde_json::json;
use wasm_bindgen::prelude::*;
use zz_terminal::{AppearanceConfigKey, AppearanceSource};

use crate::{Client, terminal::hex};

#[wasm_bindgen]
impl Client {
    pub fn appearance(&self) -> String {
        self.core.appearance().map_or_else(
            || "null".to_owned(),
            |appearance| {
                let defaulted = self
                    .core
                    .appearance_provenance()
                    .source(AppearanceConfigKey::FontFamily)
                    == AppearanceSource::Default;
                json!({
                    "fonts": if defaulted { Vec::new() } else { appearance.font_families.clone() },
                    "size": appearance.font_size_points,
                    "weight": appearance.font_weight,
                    "padding": [
                        appearance.padding_top,
                        appearance.padding_right,
                        appearance.padding_bottom,
                        appearance.padding_left,
                    ],
                    "foreground": hex(appearance.foreground),
                    "background": hex(appearance.background),
                    "cursor": hex(appearance.cursor_color),
                    "opacity": appearance.background_opacity,
                    "selectionBackground": format!("{:?}", appearance.selection_background),
                    "linkColor": hex(appearance.link_color),
                    "cursorBlinkMs": appearance.cursor_blink_interval_ms,
                    "palette": (0..16).map(|index| hex(appearance.palette[index])).collect::<Vec<_>>(),
                    "cursorStyle": format!("{:?}", appearance.cursor_style),
                })
                .to_string()
            },
        )
    }

    pub fn providers(&self) -> String {
        serde_json::to_string(&self.core.providers()).unwrap_or_else(|_| "null".to_owned())
    }
}
