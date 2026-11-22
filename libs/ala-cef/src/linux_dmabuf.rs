use std::{
    fs::File,
    mem::ManuallyDrop,
    os::{
        fd::{AsRawFd, BorrowedFd, FromRawFd as _, IntoRawFd},
        unix::fs::MetadataExt as _,
    },
};

use ash::vk;
use cef::osr_texture_import::TextureImportError;
use cef::{AcceleratedPaintInfo, ColorType};
use wgpu::hal::api::Vulkan;

use crate::popup::PopupPlacement;

pub(crate) const EXTERNAL_TEXTURE_STATE: wgpu::TextureUses =
    wgpu::TextureUses::COPY_SRC.union(wgpu::TextureUses::RESOURCE);

pub(crate) struct ImportedDmaBuf {
    pub texture: wgpu::Texture,
    pub external_queue_family: u32,
}

struct ImportedImage<'a> {
    device: &'a ash::Device,
    image: vk::Image,
    memory: Option<vk::DeviceMemory>,
}

impl Drop for ImportedImage<'_> {
    fn drop(&mut self) {
        unsafe {
            if self.image != vk::Image::null() {
                self.device.destroy_image(self.image, None);
            }
            if let Some(memory) = self.memory {
                self.device.free_memory(memory, None);
            }
        }
    }
}

fn vulkan_error(operation: &str, result: vk::Result) -> TextureImportError {
    TextureImportError::VulkanError {
        operation: format!("{operation}: {result:?}"),
    }
}

pub(crate) fn import_texture(
    info: &AcceleratedPaintInfo,
    device: &wgpu::Device,
) -> Result<ImportedDmaBuf, TextureImportError> {
    let (format, vk_format) = match info.format {
        ColorType::BGRA_8888 => (wgpu::TextureFormat::Bgra8Unorm, vk::Format::B8G8R8A8_UNORM),
        ColorType::RGBA_8888 => (wgpu::TextureFormat::Rgba8Unorm, vk::Format::R8G8B8A8_UNORM),
        _ => {
            return Err(TextureImportError::UnsupportedFormat {
                format: *info.format.as_ref(),
            });
        }
    };
    let width = u32::try_from(info.extra.coded_size.width)
        .ok()
        .filter(|width| *width > 0);
    let height = u32::try_from(info.extra.coded_size.height)
        .ok()
        .filter(|height| *height > 0);
    let planes = usize::try_from(info.plane_count)
        .ok()
        .filter(|count| *count > 0)
        .and_then(|count| info.planes.get(..count));
    let (Some(width), Some(height), Some(planes)) = (width, height, planes) else {
        return Err(TextureImportError::InvalidHandle(
            "Invalid DMA-BUF dimensions or plane count".to_owned(),
        ));
    };
    let max_dimension = device.limits().max_texture_dimension_2d;
    if width > max_dimension || height > max_dimension {
        return Err(TextureImportError::InvalidHandle(format!(
            "DMA-BUF dimensions {width}x{height} exceed the device limit of {max_dimension}"
        )));
    }
    if planes.iter().any(|plane| plane.fd < 0 || plane.stride == 0) {
        return Err(TextureImportError::InvalidHandle(
            "Invalid DMA-BUF plane descriptor or stride".to_owned(),
        ));
    }
    if planes.len() > 1 {
        let mut allocation = None;
        for plane in planes {
            let file = ManuallyDrop::new(unsafe { File::from_raw_fd(plane.fd) });
            let metadata = file.metadata().map_err(|error| {
                TextureImportError::InvalidHandle(format!("Invalid DMA-BUF descriptor: {error}"))
            })?;
            let identity = (metadata.dev(), metadata.ino());
            if allocation.is_some_and(|existing| existing != identity) {
                return Err(TextureImportError::HardwareUnavailable {
                    reason:
                        "DMA-BUF planes backed by separate allocations require disjoint bindings"
                            .to_owned(),
                });
            }
            allocation = Some(identity);
        }
    }
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let hal_device = unsafe { device.as_hal::<Vulkan>() }.ok_or_else(|| {
        TextureImportError::HardwareUnavailable {
            reason: "Device is not using Vulkan".to_owned(),
        }
    })?;
    let enabled_extensions = hal_device.enabled_device_extensions();
    if [
        ash::khr::external_memory_fd::NAME,
        ash::ext::external_memory_dma_buf::NAME,
        ash::ext::image_drm_format_modifier::NAME,
    ]
    .iter()
    .any(|extension| !enabled_extensions.contains(extension))
    {
        return Err(TextureImportError::HardwareUnavailable {
            reason: "Vulkan DMA-BUF extensions are unavailable".to_owned(),
        });
    }
    let external_queue_family =
        if enabled_extensions.contains(&ash::ext::queue_family_foreign::NAME) {
            vk::QUEUE_FAMILY_FOREIGN_EXT
        } else {
            vk::QUEUE_FAMILY_EXTERNAL
        };
    let raw_device = hal_device.raw_device();
    let layouts: Vec<_> = planes
        .iter()
        .map(|plane| vk::SubresourceLayout {
            offset: plane.offset,
            row_pitch: u64::from(plane.stride),
            ..Default::default()
        })
        .collect();
    let mut modifier = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
        .drm_format_modifier(info.modifier)
        .plane_layouts(&layouts);
    let mut external_image = vk::ExternalMemoryImageCreateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let image_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(vk_format)
        .extent(vk::Extent3D {
            width,
            height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
        .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_SRC)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .push_next(&mut modifier)
        .push_next(&mut external_image);
    let mut imported = ImportedImage {
        device: raw_device,
        image: unsafe { raw_device.create_image(&image_info, None) }
            .map_err(|error| vulkan_error("create DMA-BUF image", error))?,
        memory: None,
    };
    let requirements = unsafe { raw_device.get_image_memory_requirements(imported.image) };
    let fd = unsafe { BorrowedFd::borrow_raw(planes[0].fd) }
        .try_clone_to_owned()
        .map_err(|error| TextureImportError::PlatformError {
            message: format!("Duplicate DMA-BUF descriptor: {error}"),
        })?;
    let external_memory = ash::khr::external_memory_fd::Device::new(
        hal_device.shared_instance().raw_instance(),
        raw_device,
    );
    let mut fd_properties = vk::MemoryFdPropertiesKHR::default();
    unsafe {
        external_memory.get_memory_fd_properties(
            vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
            fd.as_raw_fd(),
            &mut fd_properties,
        )
    }
    .map_err(|error| vulkan_error("query DMA-BUF memory types", error))?;
    let memory_types = requirements.memory_type_bits & fd_properties.memory_type_bits;
    if memory_types == 0 {
        return Err(TextureImportError::InvalidHandle(
            "DMA-BUF has no compatible memory type".to_owned(),
        ));
    }
    let mut import_fd = vk::ImportMemoryFdInfoKHR::default()
        .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
        .fd(fd.as_raw_fd());
    let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(imported.image);
    let allocation = vk::MemoryAllocateInfo::default()
        .allocation_size(requirements.size)
        .memory_type_index(memory_types.trailing_zeros())
        .push_next(&mut import_fd)
        .push_next(&mut dedicated);
    let memory = unsafe { raw_device.allocate_memory(&allocation, None) }
        .map_err(|error| vulkan_error("allocate DMA-BUF memory", error))?;
    let _ = fd.into_raw_fd();
    imported.memory = Some(memory);
    unsafe { raw_device.bind_image_memory(imported.image, memory, 0) }
        .map_err(|error| vulkan_error("bind DMA-BUF memory", error))?;
    let hal_texture = unsafe {
        hal_device.texture_from_raw(
            imported.image,
            &wgpu::hal::TextureDescriptor {
                label: Some("CEF DMA-BUF"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: EXTERNAL_TEXTURE_STATE,
                memory_flags: wgpu::hal::MemoryFlags::empty(),
                view_formats: vec![],
            },
            None,
            wgpu::hal::vulkan::TextureMemory::Dedicated(memory),
        )
    };
    imported.image = vk::Image::null();
    imported.memory = None;
    drop(imported);
    drop(hal_device);
    let texture = unsafe {
        device.create_texture_from_hal::<Vulkan>(
            hal_texture,
            &wgpu::TextureDescriptor {
                label: Some("CEF DMA-BUF"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            },
            EXTERNAL_TEXTURE_STATE,
        )
    };
    Ok(ImportedDmaBuf {
        texture,
        external_queue_family,
    })
}

pub(crate) fn copy_imported_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    imported: &ImportedDmaBuf,
    destination: &wgpu::Texture,
    popup: Option<(PopupPlacement, &wgpu::Texture)>,
) -> Result<(), TextureImportError> {
    let (raw_device, local_queue_family, image) = unsafe {
        let hal_device =
            device
                .as_hal::<Vulkan>()
                .ok_or_else(|| TextureImportError::HardwareUnavailable {
                    reason: "Device is not using Vulkan".to_owned(),
                })?;
        let hal_texture = imported.texture.as_hal::<Vulkan>().ok_or_else(|| {
            TextureImportError::HardwareUnavailable {
                reason: "Imported texture is not backed by Vulkan".to_owned(),
            }
        })?;
        (
            hal_device.raw_device().clone(),
            hal_device.queue_family_index(),
            hal_texture.raw_handle(),
        )
    };

    let acquire = ownership_transfer(
        device,
        &raw_device,
        image,
        imported.external_queue_family,
        local_queue_family,
        true,
    )?;

    let mut copy = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("browser accelerated OSR copy-on-receive"),
    });
    copy.copy_texture_to_texture(
        imported.texture.as_image_copy(),
        destination.as_image_copy(),
        imported.texture.size(),
    );
    if let Some((placement, popup_texture)) = popup {
        copy.copy_texture_to_texture(
            popup_texture.as_image_copy(),
            wgpu::TexelCopyTextureInfo {
                texture: destination,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: placement.x,
                    y: placement.y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: placement.width,
                height: placement.height,
                depth_or_array_layers: 1,
            },
        );
    }
    copy.transition_resources(
        std::iter::empty(),
        std::iter::once(wgpu::TextureTransition {
            texture: &imported.texture,
            selector: None,
            state: EXTERNAL_TEXTURE_STATE,
        }),
    );

    let release = ownership_transfer(
        device,
        &raw_device,
        image,
        local_queue_family,
        imported.external_queue_family,
        false,
    )?;

    queue.submit([acquire, copy.finish(), release]);
    Ok(())
}

fn ownership_transfer(
    device: &wgpu::Device,
    raw_device: &ash::Device,
    image: vk::Image,
    from_queue_family: u32,
    to_queue_family: u32,
    acquire: bool,
) -> Result<wgpu::CommandBuffer, TextureImportError> {
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some(if acquire {
            "CEF Vulkan ownership acquire"
        } else {
            "CEF Vulkan ownership release"
        }),
    });
    let barrier = vk::ImageMemoryBarrier::default()
        .image(image)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        })
        .src_queue_family_index(from_queue_family)
        .dst_queue_family_index(to_queue_family)
        .old_layout(vk::ImageLayout::GENERAL)
        .new_layout(vk::ImageLayout::GENERAL)
        .src_access_mask(if acquire {
            vk::AccessFlags::empty()
        } else {
            vk::AccessFlags::MEMORY_READ
        })
        .dst_access_mask(if acquire {
            vk::AccessFlags::MEMORY_READ
        } else {
            vk::AccessFlags::empty()
        });

    unsafe {
        encoder.as_hal_mut::<Vulkan, _, _>(|hal_encoder| {
            let hal_encoder = hal_encoder.ok_or_else(|| TextureImportError::PlatformError {
                message: "Failed to record Vulkan external ownership transfer".to_owned(),
            })?;
            raw_device.cmd_pipeline_barrier(
                hal_encoder.raw_handle(),
                if acquire {
                    vk::PipelineStageFlags::TOP_OF_PIPE
                } else {
                    vk::PipelineStageFlags::ALL_COMMANDS
                },
                if acquire {
                    vk::PipelineStageFlags::ALL_COMMANDS
                } else {
                    vk::PipelineStageFlags::BOTTOM_OF_PIPE
                },
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            );
            Ok::<_, TextureImportError>(())
        })?;
    }
    Ok(encoder.finish())
}
