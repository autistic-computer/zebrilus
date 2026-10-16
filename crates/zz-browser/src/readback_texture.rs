use crate::frame::{FrameDamage, frame_byte_len};
use crate::{BrowserGpuContext, FrameError, OwnedBgraFrame, SessionId};

#[derive(Clone)]
pub struct ReadbackTexture {
    texture: wgpu::Texture,
    session: SessionId,
    generation: u64,
}

impl ReadbackTexture {
    pub fn upload(
        gpu: &BrowserGpuContext,
        frame: &OwnedBgraFrame,
        previous: Option<&Self>,
    ) -> Result<Self, FrameError> {
        let expected = frame_byte_len(frame.width as i32, frame.height as i32)?;
        if frame.bgra.len() != expected {
            return Err(FrameError::InvalidLength {
                expected,
                actual: frame.bgra.len(),
            });
        }
        let limit = gpu.device.limits().max_texture_dimension_2d;
        if frame.width > limit || frame.height > limit {
            return Err(FrameError::TextureTooLarge {
                width: frame.width,
                height: frame.height,
                limit,
            });
        }
        let previous = previous.filter(|previous| {
            previous.session == frame.session
                && previous.texture.width() == frame.width
                && previous.texture.height() == frame.height
        });
        let damage = upload_damage(frame, previous.map(|previous| previous.generation));
        let texture = previous.map_or_else(
            || {
                gpu.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("CEF readback"),
                    size: wgpu::Extent3d {
                        width: frame.width,
                        height: frame.height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Bgra8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_DST
                        | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                })
            },
            |previous| previous.texture.clone(),
        );
        let offset = (damage.y as usize * frame.width as usize + damage.x as usize) * 4;
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: damage.x,
                    y: damage.y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &frame.bgra[offset..],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(frame.width * 4),
                rows_per_image: Some(frame.height),
            },
            wgpu::Extent3d {
                width: damage.width,
                height: damage.height,
                depth_or_array_layers: 1,
            },
        );
        log::trace!(
            target: "zz_browser::diagnostics::readback_upload",
            "session={} generation={} width={} height={} upload_x={} upload_y={} upload_width={} upload_height={} bytes={} reused={}",
            frame.session.0,
            frame.generation,
            frame.width,
            frame.height,
            damage.x,
            damage.y,
            damage.width,
            damage.height,
            u64::from(damage.width) * u64::from(damage.height) * 4,
            previous.is_some(),
        );
        Ok(Self {
            texture,
            session: frame.session,
            generation: frame.generation,
        })
    }

    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }
}

fn upload_damage(frame: &OwnedBgraFrame, previous_generation: Option<u64>) -> FrameDamage {
    if previous_generation
        .is_some_and(|generation| generation.wrapping_add(1).max(1) == frame.generation)
        && let Some(damage) = frame.damage
    {
        let width = frame.width.saturating_sub(damage.x).min(damage.width);
        let height = frame.height.saturating_sub(damage.y).min(damage.height);
        if width > 0 && height > 0 {
            return FrameDamage {
                width,
                height,
                ..damage
            };
        }
    }
    FrameDamage {
        x: 0,
        y: 0,
        width: frame.width,
        height: frame.height,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn owned_frame(width: u32, height: u32, generation: u64) -> OwnedBgraFrame {
        OwnedBgraFrame {
            session: SessionId(1),
            generation,
            delivery_generation: 1,
            width,
            height,
            bgra: vec![37; width as usize * height as usize * 4],
            damage: Some(FrameDamage {
                x: 1,
                y: 1,
                width: 2,
                height: 2,
            }),
        }
    }

    #[test]
    fn missing_or_skipped_frames_require_full_upload() {
        let mut frame = owned_frame(8, 6, 3);
        let full = FrameDamage {
            x: 0,
            y: 0,
            width: 8,
            height: 6,
        };
        assert_eq!(upload_damage(&frame, None), full);
        assert_eq!(upload_damage(&frame, Some(1)), full);
        assert_eq!(Some(upload_damage(&frame, Some(2))), frame.damage);
        frame.damage = None;
        assert_eq!(upload_damage(&frame, Some(2)), full);
    }

    #[test]
    fn damage_is_clipped_and_empty_damage_falls_back_to_full_upload() {
        let mut frame = owned_frame(8, 6, 2);
        frame.damage = Some(FrameDamage {
            x: 7,
            y: 5,
            width: u32::MAX,
            height: u32::MAX,
        });
        assert_eq!(
            upload_damage(&frame, Some(1)),
            FrameDamage {
                x: 7,
                y: 5,
                width: 1,
                height: 1
            }
        );
        frame.damage = Some(FrameDamage {
            x: 8,
            y: 5,
            width: 1,
            height: 1,
        });
        assert_eq!(
            upload_damage(&frame, Some(1)),
            FrameDamage {
                x: 0,
                y: 0,
                width: 8,
                height: 6
            }
        );
    }

    fn gpu() -> BrowserGpuContext {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .unwrap();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        BrowserGpuContext::new(Arc::new(device), Arc::new(queue))
    }

    fn read_pixels(gpu: &BrowserGpuContext, texture: &wgpu::Texture) -> Vec<u8> {
        let row_bytes = texture.width() * 4;
        let padded_row_bytes = row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(padded_row_bytes) * u64::from(texture.height()),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row_bytes),
                    rows_per_image: None,
                },
            },
            texture.size(),
        );
        gpu.queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap();
            });
        gpu.device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        receiver.recv().unwrap().unwrap();
        let mapped = buffer.slice(..).get_mapped_range().unwrap();
        mapped
            .chunks_exact(padded_row_bytes as usize)
            .flat_map(|row| row[..row_bytes as usize].iter().copied())
            .collect()
    }

    #[test]
    #[ignore = "requires a Vulkan adapter"]
    fn gpu_readback_preserves_pixels_across_damage_gaps_resize_and_sessions() {
        let gpu = gpu();
        let mut frame = owned_frame(13, 7, 1);
        let first = ReadbackTexture::upload(&gpu, &frame, None).unwrap();
        assert_eq!(read_pixels(&gpu, first.texture()), frame.bgra);
        frame.generation += 1;
        for y in 1..3 {
            for x in 1..3 {
                let offset = (y * frame.width as usize + x) * 4;
                frame.bgra[offset..offset + 4].copy_from_slice(&[10, 20, 30, 40]);
            }
        }
        let second = ReadbackTexture::upload(&gpu, &frame, Some(&first)).unwrap();
        assert_eq!(first.texture, second.texture);
        assert_eq!(read_pixels(&gpu, second.texture()), frame.bgra);
        frame.generation += 2;
        frame.bgra.fill(83);
        let after_gap = ReadbackTexture::upload(&gpu, &frame, Some(&second)).unwrap();
        assert_eq!(read_pixels(&gpu, after_gap.texture()), frame.bgra);
        let mut resized = owned_frame(9, 5, 5);
        resized.bgra.fill(91);
        let after_resize = ReadbackTexture::upload(&gpu, &resized, Some(&after_gap)).unwrap();
        assert_ne!(after_gap.texture, after_resize.texture);
        assert_eq!(read_pixels(&gpu, after_resize.texture()), resized.bgra);
        resized.session = SessionId(2);
        resized.generation += 1;
        resized.bgra.fill(103);
        let after_session = ReadbackTexture::upload(&gpu, &resized, Some(&after_resize)).unwrap();
        assert_ne!(after_resize.texture, after_session.texture);
        assert_eq!(read_pixels(&gpu, after_session.texture()), resized.bgra);
        let oversized = owned_frame(gpu.device.limits().max_texture_dimension_2d + 1, 1, 1);
        assert!(matches!(
            ReadbackTexture::upload(&gpu, &oversized, None),
            Err(FrameError::TextureTooLarge { .. })
        ));
    }

    #[test]
    #[ignore = "requires a Vulkan adapter and an idle profiling host"]
    fn benchmark_readback_damage_uploads() {
        let gpu = gpu();
        let mut samples = Vec::new();
        let frames = 120;
        for round in 0..3 {
            for partial in if round % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let mut frame = owned_frame(1920, 1080, 1);
                frame.damage = partial.then_some(FrameDamage {
                    x: 32,
                    y: 48,
                    width: 256,
                    height: 128,
                });
                let mut texture = ReadbackTexture::upload(&gpu, &frame, None).unwrap();
                gpu.queue.submit([]);
                gpu.device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .unwrap();
                let started = std::time::Instant::now();
                for index in 0..frames {
                    frame.generation += 1;
                    frame.bgra[(48 * 1920 + 32) * 4] = index as u8;
                    texture = ReadbackTexture::upload(&gpu, &frame, Some(&texture)).unwrap();
                    gpu.queue.submit([]);
                    if index % 8 == 7 {
                        gpu.device
                            .poll(wgpu::PollType::wait_indefinitely())
                            .unwrap();
                    }
                }
                gpu.device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .unwrap();
                let elapsed_us = started.elapsed().as_micros();
                assert!(read_pixels(&gpu, texture.texture()).iter().eq(&frame.bgra));
                samples.push(serde_json::json!({
                    "round": round,
                    "partial": partial,
                    "frames": frames,
                    "elapsed_us": elapsed_us,
                    "pixel_bytes_uploaded": frames * if partial { 256_u64 * 128 * 4 } else { 1920_u64 * 1080 * 4 },
                }));
            }
        }
        println!(
            "{}",
            serde_json::json!({ "readback_upload_benchmark": samples })
        );
    }
}
