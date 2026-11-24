# Linux DMA-BUF import

`src/linux_dmabuf.rs` derives from the Linux importer in
[cef-rs revision 6a6ed14be68c5b8d90b1e5f5a6705a7f899b18b7](https://github.com/tauri-apps/cef-rs/blob/6a6ed14be68c5b8d90b1e5f5a6705a7f899b18b7/cef/src/osr_texture_import/dmabuf.rs),
shipped in `cef 154.0.0+154.0.23`. See [NOTICE.md](NOTICE.md) for its MIT license.

The upstream importer returns a Vulkan image without retaining its allocated
`VkDeviceMemory`. It wraps the image as `TextureMemory::External`, so wgpu frees
the image but cannot free the memory. The local importer retains image and memory
through failure paths, transfers memory to `TextureMemory::Dedicated` on success,
and keeps the duplicated file descriptor owned until Vulkan consumes it. It also
declares the external image handle type and intersects the image's memory types
with the imported descriptor's supported memory types.

wgpu-hal 30's `texture_from_dmabuf_fd` handles the same lifetime, but only for
single-plane modifier layouts. The local importer keeps cef-rs's support for
multiple modifier planes sharing one DMA-BUF allocation. It does not import
disjoint allocations for separate planes.

Remove the local importer when a compatible cef-rs release owns both resources
on success and cleans up image, memory, and duplicated descriptors on failure.
Validate that release with sustained accelerated paints and allocation/bind
failure injection before switching back. Keep the CEF binary milestone aligned
with the Rust bindings as described in `third_party/cef-dll-sys/UPSTREAM.md`.
