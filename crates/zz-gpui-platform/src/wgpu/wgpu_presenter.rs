use std::sync::Arc;

use anyhow::{Context as _, Result};

pub struct WgpuPresenter {
    surface: wgpu::Surface<'static>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    capabilities: wgpu::SurfaceCapabilities,
    config: Option<wgpu::SurfaceConfiguration>,
    blitter: Option<wgpu::util::TextureBlitter>,
}

impl WgpuPresenter {
    pub(crate) fn new(
        surface: wgpu::Surface<'static>,
        adapter: &wgpu::Adapter,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
    ) -> Self {
        let capabilities = surface.get_capabilities(adapter);
        Self {
            surface,
            device,
            queue,
            capabilities,
            config: None,
            blitter: None,
        }
    }

    fn configure(&mut self, texture: &wgpu::Texture) -> Result<()> {
        let width = texture.width();
        let height = texture.height();
        let format = texture.format();
        let source_copies = texture.usage().contains(wgpu::TextureUsages::COPY_SRC);
        if self.config.as_ref().is_some_and(|config| {
            config.width == width
                && config.height == height
                && (self.blitter.is_some() || (config.format == format && source_copies))
        }) {
            return Ok(());
        }
        let capabilities = &self.capabilities;
        let target_format = if capabilities.formats.contains(&format) {
            format
        } else {
            capabilities
                .formats
                .iter()
                .copied()
                .find(|format| !format.is_srgb())
                .or_else(|| capabilities.formats.first().copied())
                .context("the presenter surface reports no formats")?
        };
        let copies = source_copies
            && target_format == format
            && capabilities.usages.contains(wgpu::TextureUsages::COPY_DST);
        let present_mode = [wgpu::PresentMode::Mailbox, wgpu::PresentMode::Immediate]
            .into_iter()
            .find(|mode| capabilities.present_modes.contains(mode))
            .unwrap_or(wgpu::PresentMode::Fifo);
        let alpha_mode = if capabilities
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::Opaque)
        {
            wgpu::CompositeAlphaMode::Opaque
        } else {
            capabilities
                .alpha_modes
                .first()
                .copied()
                .unwrap_or(wgpu::CompositeAlphaMode::Auto)
        };
        let config = wgpu::SurfaceConfiguration {
            usage: if copies {
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST
            } else {
                wgpu::TextureUsages::RENDER_ATTACHMENT
            },
            format: target_format,
            width,
            height,
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: Vec::new(),
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        self.surface.configure(&self.device, &config);
        self.blitter =
            (!copies).then(|| wgpu::util::TextureBlitter::new(&self.device, target_format));
        self.config = Some(config);
        Ok(())
    }

    pub fn present(&mut self, texture: &wgpu::Texture) -> Result<bool> {
        self.configure(texture)?;
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                drop(frame);
                self.config = None;
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                self.config = None;
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                anyhow::bail!("the presenter surface texture failed validation")
            }
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("underlay_present"),
            });
        let width = texture.width().min(frame.texture.width());
        let height = texture.height().min(frame.texture.height());
        if let Some(blitter) = &self.blitter {
            let source = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let target = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            blitter.copy(&self.device, &mut encoder, &source, &target);
        } else {
            encoder.copy_texture_to_texture(
                texture.as_image_copy(),
                frame.texture.as_image_copy(),
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        Ok(true)
    }
}
