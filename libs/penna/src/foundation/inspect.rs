use ala::App;
use serde::{Deserialize, Serialize};

use crate::{
    ActiveTheme as _, Density, ThemeMode,
    foundation::{
        motion::Motion,
        surface::{SurfaceRole, peek_drawn_roles},
    },
    to_hex,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DrawnRoleEntry {
    pub id: String,
    pub kind: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TokenSnapshot {
    pub mode: String,
    pub radius_px: f32,
    pub control_radius_px: f32,
    pub smoothing: f32,
    pub contrast: f32,
    pub density: String,
    pub glass: bool,
    pub bg: String,
    pub fg: String,
    pub accent: String,
    pub success: String,
    pub warning: String,
    pub danger: String,
    pub data_palette: [String; 8],
    pub drawn_roles: Vec<DrawnRoleEntry>,
}

impl TokenSnapshot {
    #[must_use]
    pub fn capture(cx: &App) -> Self {
        let theme = cx.theme();
        let palette = theme.data_palette().map(to_hex);
        let roles = peek_drawn_roles()
            .into_iter()
            .map(|id| {
                let kind = SurfaceRole::KIT
                    .iter()
                    .find(|role| role.id() == id)
                    .map_or("custom", |role| role.kind().name());
                DrawnRoleEntry {
                    id: id.to_owned(),
                    kind: kind.to_owned(),
                }
            })
            .collect();
        Self {
            mode: theme.mode.name().to_owned(),
            radius_px: f32::from(theme.radius),
            control_radius_px: f32::from(theme.control_radius()),
            smoothing: theme.smoothing.exponent(),
            contrast: theme.contrast(),
            density: Density::get().name().to_owned(),
            glass: theme.glass.is_some(),
            bg: to_hex(theme.background),
            fg: to_hex(theme.foreground),
            accent: to_hex(theme.accent),
            success: to_hex(theme.success),
            warning: to_hex(theme.warning),
            danger: to_hex(theme.danger),
            data_palette: palette,
            drawn_roles: roles,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum InspectRequest {
    Snapshot,
    Roles,
    SetMode { mode: ThemeMode },
    SetDensity { density: Density },
    SetMotion { motion: Motion },
    SetContrast { contrast: f32 },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum InspectResponse {
    Snapshot { snapshot: Box<TokenSnapshot> },
    Roles { roles: Vec<DrawnRoleEntry> },
    Ok,
    Error { message: String },
}

#[must_use]
pub fn handle_inspect_request(request: InspectRequest, cx: &mut App) -> InspectResponse {
    match request {
        InspectRequest::Snapshot => InspectResponse::Snapshot {
            snapshot: Box::new(TokenSnapshot::capture(cx)),
        },
        InspectRequest::Roles => InspectResponse::Roles {
            roles: TokenSnapshot::capture(cx).drawn_roles,
        },
        InspectRequest::SetMode { mode } => {
            crate::Theme::change(mode, None, cx);
            InspectResponse::Ok
        }
        InspectRequest::SetDensity { density } => {
            density.set();
            InspectResponse::Ok
        }
        InspectRequest::SetMotion { motion } => {
            cx.set_reduce_motion(motion.reduced());
            cx.set_animation_speed(motion.speed());
            InspectResponse::Ok
        }
        InspectRequest::SetContrast { contrast } => {
            crate::Theme::global_mut(cx).set_contrast(contrast);
            InspectResponse::Ok
        }
    }
}

#[must_use]
pub fn handle_inspect_line(line: &str, cx: &mut App) -> String {
    let response = match serde_json::from_str::<InspectRequest>(line) {
        Ok(request) => handle_inspect_request(request, cx),
        Err(err) => InspectResponse::Error {
            message: err.to_string(),
        },
    };
    serde_json::to_string(&response)
        .unwrap_or_else(|_| r#"{"kind":"error","message":"encode"}"#.to_owned())
}

#[cfg(test)]
mod tests {
    use ala::TestAppContext;

    use super::*;

    #[ala::test]
    fn inspect_protocol_round_trips_snapshot_and_density_changes(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let reply =
            cx.update(|cx| handle_inspect_line(r#"{"op":"set-density","density":"compact"}"#, cx));
        assert_eq!(reply, r#"{"kind":"ok"}"#);

        let snap_line = cx.update(|cx| handle_inspect_line(r#"{"op":"snapshot"}"#, cx));
        let parsed: InspectResponse = serde_json::from_str(&snap_line).unwrap();
        let InspectResponse::Snapshot { snapshot } = parsed else {
            panic!("expected snapshot response, got {parsed:?}");
        };
        assert_eq!(snapshot.density, "compact");
        assert_eq!(snapshot.data_palette.len(), 8);
        assert!(snapshot.bg.starts_with('#'));

        Density::Regular.set();
    }
}
