use std::os::fd::{AsRawFd, BorrowedFd, IntoRawFd};

use ash::vk;
use cef::osr_texture_import::{TextureImportError, TextureImportResult};
use cef::{AcceleratedPaintInfo, ColorType};
use wgpu::hal::api::Vulkan;

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
) -> TextureImportResult {
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
    if planes.iter().any(|plane| plane.fd < 0 || plane.stride == 0) {
        return Err(TextureImportError::InvalidHandle(
            "Invalid DMA-BUF plane descriptor or stride".to_owned(),
        ));
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
    if [
        ash::khr::external_memory_fd::NAME,
        ash::ext::external_memory_dma_buf::NAME,
        ash::ext::image_drm_format_modifier::NAME,
    ]
    .iter()
    .any(|extension| !hal_device.enabled_device_extensions().contains(extension))
    {
        return Err(TextureImportError::HardwareUnavailable {
            reason: "Vulkan DMA-BUF extensions are unavailable".to_owned(),
        });
    }
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
        .usage(vk::ImageUsageFlags::SAMPLED)
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
                usage: wgpu::TextureUses::RESOURCE,
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
    Ok(unsafe {
        device.create_texture_from_hal::<Vulkan>(
            hal_texture,
            &wgpu::TextureDescriptor {
                label: Some("CEF DMA-BUF"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::TextureUses::RESOURCE,
        )
    })
}
