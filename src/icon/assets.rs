use std::borrow::Cow;

use ala::{AssetSource, Result, SharedString};

use super::glyphs;

#[derive(rust_embed::RustEmbed)]
#[folder = "assets"]
#[include = "icons/**/*.svg"]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }
        if let Some(svg) = glyphs::load(path) {
            return Ok(Some(Cow::Owned(svg.into_bytes())));
        }

        Ok(Self::get(path).map(|asset| asset.data))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(Self::iter()
            .filter(|asset| asset.starts_with(path))
            .map(Into::into)
            .collect())
    }
}
