use std::{cell::RefCell, rc::Rc, sync::Arc};

use ala::{DevicePixels, Hsla, PlatformAtlas, PlatformHeadlessRenderer, Result, Scene, Size};
use image::RgbaImage;
use serde_json::{Value, json};

pub struct Capture {
    pub native: Box<dyn PlatformHeadlessRenderer>,
    pub scene: Rc<RefCell<Value>>,
    pub size: Size<DevicePixels>,
}

fn color(value: Hsla) -> [f32; 4] {
    [value.h, value.s, value.l, value.a]
}

fn geometry(scene: &Scene) -> Value {
    let quads: Vec<_> = scene
        .quads
        .iter()
        .map(|quad| {
            json!({
                "bounds": [quad.bounds.origin.x.0, quad.bounds.origin.y.0, quad.bounds.size.width.0, quad.bounds.size.height.0],
                "color": quad.background.as_solid().map(color),
                "edge": color(quad.border_color),
                "radius": [quad.corner_radii.top_left.0, quad.corner_radii.top_right.0, quad.corner_radii.bottom_right.0, quad.corner_radii.bottom_left.0],
                "order": quad.order,
            })
        })
        .collect();
    let glyphs: Vec<_> = scene
        .monochrome_sprites
        .iter()
        .map(|glyph| {
            json!({
                "bounds": [glyph.bounds.origin.x.0, glyph.bounds.origin.y.0, glyph.bounds.size.width.0, glyph.bounds.size.height.0],
                "color": color(glyph.color),
                "tile": [glyph.tile.texture_id.index, glyph.tile.tile_id.0],
                "order": glyph.order,
            })
        })
        .collect();
    json!({"quads": quads, "glyphs": glyphs})
}

impl PlatformHeadlessRenderer for Capture {
    fn render_scene_to_image(
        &mut self,
        scene: &Scene,
        _: Size<DevicePixels>,
    ) -> Result<RgbaImage> {
        *self.scene.borrow_mut() = geometry(scene);
        self.native.render_scene_to_image(scene, self.size)
    }

    fn render_scene(&mut self, scene: &Scene, _: Size<DevicePixels>) -> Result<()> {
        *self.scene.borrow_mut() = geometry(scene);
        self.native.render_scene(scene, self.size)
    }

    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        self.native.sprite_atlas()
    }
}
