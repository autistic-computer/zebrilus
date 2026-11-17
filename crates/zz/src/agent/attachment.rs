use std::sync::Arc;

use ala::{Image, ImageFormat};

pub(crate) fn wire_images(images: &[Arc<Image>]) -> Vec<zz_protocol::AgentImage> {
    images
        .iter()
        .map(|image| zz_protocol::AgentImage {
            format: image.format.mime_type().to_owned(),
            data: image.bytes.clone(),
        })
        .collect()
}

pub(crate) fn inbound_image(image: &zz_daemon::AgentStreamImage) -> Option<Arc<Image>> {
    let format = ImageFormat::from_mime_type(&image.format)?;
    Some(Arc::new(Image::from_bytes(format, image.data.clone())))
}
