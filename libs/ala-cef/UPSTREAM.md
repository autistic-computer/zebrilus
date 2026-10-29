# Linux DMA-BUF import

`src/linux_dmabuf.rs` derives from the Linux importer in
[cef-rs revision 6a6ed14be68c5b8d90b1e5f5a6705a7f899b18b7](https://github.com/tauri-apps/cef-rs/blob/6a6ed14be68c5b8d90b1e5f5a6705a7f899b18b7/cef/src/osr_texture_import/dmabuf.rs),
shipped in `cef 154.0.0+154.0.23`. We use that code under its MIT license below.

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

## MIT License

Copyright (c) 2023 Wu Yuwei

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
