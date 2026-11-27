use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
    time::Duration,
};

use ala::{Bounds, CustomShader, Pixels, Point, Size, Window};
use naga::valid::{Capabilities, ValidationFlags, Validator};
use web_time::Instant;
use zz_terminal::{Color, TerminalAppearance};

const PRELUDE: &str = include_str!("shaders/prelude.glsl");
const MAIN: &str = "\nvoid main() { mainImage(zz_FragColor, zz_FragCoord); }\n";
const MAX_COPY_RECTS: usize = 16;
const COPY_FLASH_SECONDS: f32 = 0.55;
const SELECTION_GRACE: Duration = Duration::from_millis(500);

static COPY_FLASH: LazyLock<Option<TerminalShader>> = LazyLock::new(|| {
    TerminalShader::compile(&format!(
        "#define FLASH_SECONDS {COPY_FLASH_SECONDS:?}\n{}",
        include_str!("shaders/copy_flash.glsl")
    ))
    .inspect_err(|error| log::error!("copy flash shader failed: {error}"))
    .ok()
});

#[derive(Clone)]
pub struct CopyFlash {
    epoch: Instant,
    started: Instant,
    rects: Arc<[Bounds<Pixels>]>,
}

pub struct CopyFlashTracker {
    epoch: Instant,
    painted: Vec<Bounds<Pixels>>,
    cleared_at: Option<Instant>,
    flash: Option<CopyFlash>,
}

impl Default for CopyFlashTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl CopyFlashTracker {
    #[must_use]
    pub fn new() -> Self {
        Self {
            epoch: Instant::now(),
            painted: Vec::new(),
            cleared_at: None,
            flash: None,
        }
    }

    pub fn record(&mut self, selection: &[Bounds<Pixels>]) -> Option<CopyFlash> {
        if selection.is_empty() {
            if !self.painted.is_empty() && self.cleared_at.is_none() {
                self.cleared_at = Some(Instant::now());
            }
        } else {
            self.cleared_at = None;
            if self.painted != selection {
                self.painted = selection.to_vec();
                if let Some(flash) = &mut self.flash {
                    flash.rects = merge_rows(selection).into();
                }
            }
        }
        self.flash.clone()
    }

    pub fn start(&mut self) -> bool {
        let now = Instant::now();
        let recent = self
            .cleared_at
            .is_none_or(|cleared| now.duration_since(cleared) < SELECTION_GRACE);
        if self.painted.is_empty() || !recent {
            return false;
        }
        self.flash = Some(CopyFlash {
            epoch: self.epoch,
            started: now,
            rects: merge_rows(&self.painted).into(),
        });
        true
    }

    pub fn live(&mut self) -> Option<CopyFlash> {
        let flash = self.flash.as_ref()?;
        if flash.started.elapsed().as_secs_f32() > COPY_FLASH_SECONDS {
            self.flash = None;
        }
        self.flash.clone()
    }
}

pub fn paint_copy_flash<R>(
    flash: Option<(&CopyFlash, &TerminalAppearance)>,
    bounds: Bounds<Pixels>,
    cell: Size<Pixels>,
    window: &mut Window,
    content: impl FnOnce(&mut Window) -> R,
) -> R {
    let layer = flash.and_then(|(flash, appearance)| {
        let shader = COPY_FLASH.as_ref()?;
        let scale = window.scale_factor();
        let mut uniforms = shader.uniforms();
        uniforms
            .float(
                "iTime",
                Instant::now().duration_since(flash.epoch).as_secs_f32(),
            )
            .float(
                "iTimeCopy",
                flash.started.duration_since(flash.epoch).as_secs_f32(),
            )
            .vector(
                "iCellSize",
                &[
                    f32::from(cell.width) * scale,
                    f32::from(cell.height) * scale,
                ],
            )
            .appearance(appearance)
            .copy_rects(&flash.rects, bounds.origin, scale);
        Some((shader, uniforms.finish()))
    });
    let Some((shader, uniforms)) = layer else {
        return content(window);
    };
    let painted = window.paint_shader_layer(bounds, &shader.shader, uniforms, content);
    window.request_animation_frame();
    painted
}

struct TerminalShader {
    shader: CustomShader,
    layout: Arc<UniformLayout>,
}

struct UniformLayout {
    size: usize,
    offsets: HashMap<String, usize>,
}

impl TerminalShader {
    fn compile(body: &str) -> Result<Self, String> {
        let source = format!("{PRELUDE}\n{body}{MAIN}");
        let module = naga::front::glsl::Frontend::default()
            .parse(
                &naga::front::glsl::Options::from(naga::ShaderStage::Fragment),
                &source,
            )
            .map_err(|error| error.emit_to_string(&source))?;
        let info = Validator::new(ValidationFlags::all(), Capabilities::all())
            .validate(&module)
            .map_err(|error| error.emit_to_string(&source))?;
        let wgsl =
            naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty())
                .map_err(|error| error.to_string())?;
        let module =
            naga::front::wgsl::parse_str(&wgsl).map_err(|error| error.emit_to_string(&wgsl))?;
        Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .map_err(|error| error.emit_to_string(&wgsl))?;
        Ok(Self {
            layout: Arc::new(UniformLayout::of(&module)?),
            shader: CustomShader::new(wgsl),
        })
    }

    fn uniforms(&self) -> Uniforms {
        Uniforms {
            layout: Arc::clone(&self.layout),
            bytes: vec![0; self.layout.size],
        }
    }
}

impl UniformLayout {
    fn of(module: &naga::Module) -> Result<Self, String> {
        let mut layouter = naga::proc::Layouter::default();
        layouter
            .update(module.to_ctx())
            .map_err(|error| error.to_string())?;
        let globals = module
            .global_variables
            .iter()
            .find(|(_, global)| global.space == naga::AddressSpace::Uniform)
            .map(|(_, global)| global.ty)
            .ok_or("the shader has no uniform block")?;
        let naga::TypeInner::Struct { members, .. } = &module.types[globals].inner else {
            return Err("the uniform block is not a struct".to_owned());
        };
        Ok(Self {
            size: layouter[globals].size as usize,
            offsets: members
                .iter()
                .filter_map(|member| Some((member.name.clone()?, member.offset as usize)))
                .collect(),
        })
    }
}

struct Uniforms {
    layout: Arc<UniformLayout>,
    bytes: Vec<u8>,
}

impl Uniforms {
    fn write(&mut self, name: &str, index: usize, values: &[f32]) {
        let Some(offset) = self
            .layout
            .offsets
            .get(name)
            .map(|offset| offset + index * 16)
        else {
            return;
        };
        for (slot, value) in values.iter().enumerate() {
            let at = offset + slot * 4;
            if let Some(bytes) = self.bytes.get_mut(at..at + 4) {
                bytes.copy_from_slice(&value.to_ne_bytes());
            }
        }
    }

    fn float(&mut self, name: &str, value: f32) -> &mut Self {
        self.write(name, 0, &[value]);
        self
    }

    fn int(&mut self, name: &str, value: i32) -> &mut Self {
        if let Some(&offset) = self.layout.offsets.get(name)
            && let Some(bytes) = self.bytes.get_mut(offset..offset + 4)
        {
            bytes.copy_from_slice(&value.to_ne_bytes());
        }
        self
    }

    fn vector(&mut self, name: &str, values: &[f32]) -> &mut Self {
        self.write(name, 0, values);
        self
    }

    fn element(&mut self, name: &str, index: usize, values: &[f32]) -> &mut Self {
        self.write(name, index, values);
        self
    }

    fn appearance(&mut self, appearance: &TerminalAppearance) -> &mut Self {
        let selection = appearance.selection_background;
        self.vector("iBackgroundColor", &rgb(appearance.background))
            .vector("iForegroundColor", &rgb(appearance.foreground))
            .vector("iCursorColor", &rgb(appearance.cursor_color))
            .vector(
                "iSelectionForegroundColor",
                &rgb(appearance.selection_foreground),
            )
            .vector(
                "iSelectionBackgroundColor",
                &rgb(Color::rgb(selection.r, selection.g, selection.b)),
            );
        for (index, color) in appearance.palette.as_array().iter().enumerate() {
            self.element("iPalette", index, &rgb(*color));
        }
        self
    }

    fn copy_rects(
        &mut self,
        rects: &[Bounds<Pixels>],
        origin: Point<Pixels>,
        scale: f32,
    ) -> &mut Self {
        let count = rects.len().min(MAX_COPY_RECTS);
        for (index, rect) in rects.iter().take(count).enumerate() {
            let left = (rect.origin.x - origin.x).as_f32() * scale;
            let top = (rect.origin.y - origin.y).as_f32() * scale;
            let right = (rect.right() - origin.x).as_f32() * scale;
            let bottom = (rect.bottom() - origin.y).as_f32() * scale;
            self.element("iCopyRects", index, &[left, top, right, bottom]);
        }
        self.int("iCopyRectCount", count as i32)
    }

    fn finish(self) -> Arc<[u8]> {
        self.bytes.into()
    }
}

fn rgb(color: Color) -> [f32; 3] {
    [
        f32::from(color.r) / 255.0,
        f32::from(color.g) / 255.0,
        f32::from(color.b) / 255.0,
    ]
}

fn merge_rows(rows: &[Bounds<Pixels>]) -> Vec<Bounds<Pixels>> {
    let mut merged: Vec<Bounds<Pixels>> = Vec::with_capacity(rows.len().min(MAX_COPY_RECTS));
    for row in rows {
        match merged.last_mut() {
            Some(last)
                if last.left() == row.left()
                    && last.right() == row.right()
                    && last.bottom() == row.top() =>
            {
                last.size.height += row.size.height;
            }
            _ => merged.push(*row),
        }
    }
    if merged.len() > MAX_COPY_RECTS {
        let tail = merged.split_off(MAX_COPY_RECTS - 1);
        let bounds = tail
            .iter()
            .skip(1)
            .fold(tail[0], |bounds, rect| bounds.union(rect));
        merged.push(bounds);
    }
    merged
}

#[cfg(test)]
mod tests {
    use ala::{point, px, size};

    use super::*;

    fn row(left: f32, top: f32, width: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(left), px(top)), size(px(width), px(10.0)))
    }

    #[test]
    fn copy_flash_compiles_and_exposes_its_uniforms() {
        let shader = COPY_FLASH.as_ref().expect("copy flash compiles");
        for name in [
            "iTime",
            "iTimeCopy",
            "iCopyRects",
            "iCopyRectCount",
            "iCellSize",
        ] {
            assert!(shader.layout.offsets.contains_key(name), "{name} missing");
        }
        assert_eq!(shader.uniforms().finish().len(), shader.layout.size);
    }

    #[test]
    fn ghostty_shaders_translate() {
        let shader = TerminalShader::compile(
            "void mainImage(out vec4 fragColor, in vec2 fragCoord) {\n\
                 vec2 uv = fragCoord / iResolution.xy;\n\
                 vec4 base = texture(iChannel0, uv);\n\
                 float glow = smoothstep(0.0, 1.0, iCurrentCursor.z) * iChannelTime[0];\n\
                 fragColor = vec4(base.rgb + iPalette[1] * glow, base.a);\n\
             }",
        );
        assert!(shader.is_ok(), "{:?}", shader.err());
        assert_eq!(
            TerminalShader::compile("void mainImage(out vec4 c, in vec2 p) { c = nope; }")
                .err()
                .as_deref()
                .and_then(|error| error.lines().next()),
            Some("error: Unknown variable: nope")
        );
    }

    #[test]
    fn uniforms_land_at_the_translated_offsets() {
        let shader = COPY_FLASH.as_ref().expect("copy flash compiles");
        let mut uniforms = shader.uniforms();
        uniforms.float("iTimeCopy", 1.5).copy_rects(
            &[row(10.0, 20.0, 30.0)],
            point(px(10.0), px(10.0)),
            2.0,
        );
        let bytes = uniforms.finish();
        let read = |name: &str, index: usize| {
            let offset = shader.layout.offsets[name] + index * 4;
            f32::from_ne_bytes(bytes[offset..offset + 4].try_into().unwrap())
        };
        assert_eq!(read("iTimeCopy", 0), 1.5);
        assert_eq!(
            [0, 1, 2, 3].map(|index| read("iCopyRects", index)),
            [0.0, 20.0, 60.0, 40.0]
        );
        let count = shader.layout.offsets["iCopyRectCount"];
        assert_eq!(
            i32::from_ne_bytes(bytes[count..count + 4].try_into().unwrap()),
            1
        );
    }

    #[test]
    fn stacked_rows_with_the_same_span_merge() {
        let rows = [
            row(30.0, 0.0, 70.0),
            row(0.0, 10.0, 100.0),
            row(0.0, 20.0, 100.0),
            row(0.0, 30.0, 40.0),
        ];
        assert_eq!(
            merge_rows(&rows),
            vec![
                rows[0],
                Bounds::new(point(px(0.0), px(10.0)), size(px(100.0), px(20.0))),
                rows[3],
            ]
        );
        let ragged: Vec<_> = (0..40)
            .map(|index| row(0.0, index as f32 * 10.0, 10.0 + index as f32))
            .collect();
        let merged = merge_rows(&ragged);
        assert_eq!(merged.len(), MAX_COPY_RECTS);
        assert_eq!(merged.last().unwrap().bottom(), px(400.0));
    }
}
