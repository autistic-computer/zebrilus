use std::{cell::RefCell, ffi::c_void};

use anyhow::Context as _;
use wayland_client::{
    Proxy, QueueHandle,
    protocol::{wl_compositor, wl_subcompositor, wl_subsurface, wl_surface},
};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};
use zz_gpui::{Bounds, Pixels, PlatformUnderlay};

use crate::linux::WaylandClientStatePtr;
use crate::wgpu::{WgpuPresenter, wgpu};

pub(crate) struct WaylandUnderlay(RefCell<UnderlayState>);

struct UnderlayState {
    presenter: Option<WgpuPresenter>,
    surface: wl_surface::WlSurface,
    viewport: wp_viewport::WpViewport,
    subsurface: Option<wl_subsurface::WlSubsurface>,
    parent: wl_surface::WlSurface,
    subcompositor: wl_subcompositor::WlSubcompositor,
    qh: QueueHandle<WaylandClientStatePtr>,
    bounds: Option<Bounds<i32>>,
}

pub(crate) fn raw_window_handle(
    surface: &wl_surface::WlSurface,
) -> anyhow::Result<raw_window_handle::RawWindowHandle> {
    let window = std::ptr::NonNull::new(surface.id().as_ptr().cast::<c_void>())
        .context("the underlay surface has no proxy")?;
    Ok(raw_window_handle::WaylandWindowHandle::new(window).into())
}

impl WaylandUnderlay {
    pub(crate) fn new(
        compositor: &wl_compositor::WlCompositor,
        subcompositor: wl_subcompositor::WlSubcompositor,
        viewporter: &wp_viewporter::WpViewporter,
        qh: &QueueHandle<WaylandClientStatePtr>,
        parent: wl_surface::WlSurface,
        presenter: impl FnOnce(&wl_surface::WlSurface) -> anyhow::Result<WgpuPresenter>,
    ) -> anyhow::Result<Self> {
        let surface = compositor.create_surface(qh, ());
        let region = compositor.create_region(qh, ());
        surface.set_input_region(Some(&region));
        region.destroy();
        let viewport = viewporter.get_viewport(&surface, qh, ());
        let presenter = match presenter(&surface) {
            Ok(presenter) => presenter,
            Err(error) => {
                viewport.destroy();
                surface.destroy();
                return Err(error);
            }
        };
        Ok(Self(RefCell::new(UnderlayState {
            presenter: Some(presenter),
            surface,
            viewport,
            subsurface: None,
            parent,
            subcompositor,
            qh: qh.clone(),
            bounds: None,
        })))
    }
}

impl UnderlayState {
    fn map(&mut self) {
        if self.subsurface.is_some() {
            return;
        }
        let subsurface =
            self.subcompositor
                .get_subsurface(&self.surface, &self.parent, &self.qh, ());
        subsurface.set_desync();
        subsurface.place_below(&self.parent);
        if let Some(bounds) = self.bounds {
            subsurface.set_position(bounds.origin.x, bounds.origin.y);
        }
        self.subsurface = Some(subsurface);
    }

    fn unmap(&mut self) {
        if let Some(subsurface) = self.subsurface.take() {
            subsurface.destroy();
        }
    }
}

#[allow(clippy::cast_possible_truncation)]
fn logical(bounds: Bounds<Pixels>) -> Bounds<i32> {
    let round = |value: Pixels| f32::from(value).round() as i32;
    let left = round(bounds.origin.x);
    let top = round(bounds.origin.y);
    Bounds {
        origin: zz_gpui::point(left, top),
        size: zz_gpui::size(
            (round(bounds.origin.x + bounds.size.width) - left).max(1),
            (round(bounds.origin.y + bounds.size.height) - top).max(1),
        ),
    }
}

impl PlatformUnderlay for WaylandUnderlay {
    fn set_bounds(&self, bounds: Bounds<Pixels>) {
        let mut state = self.0.borrow_mut();
        let bounds = logical(bounds);
        if state.bounds == Some(bounds) {
            return;
        }
        state.bounds = Some(bounds);
        if let Some(subsurface) = &state.subsurface {
            subsurface.set_position(bounds.origin.x, bounds.origin.y);
        }
        state
            .viewport
            .set_destination(bounds.size.width, bounds.size.height);
    }

    fn set_visible(&self, visible: bool) {
        let mut state = self.0.borrow_mut();
        if visible {
            state.map();
        } else {
            state.unmap();
        }
    }

    fn present(&self, texture: &wgpu::Texture) -> anyhow::Result<bool> {
        let mut state = self.0.borrow_mut();
        state
            .presenter
            .as_mut()
            .context("the underlay was released")?
            .present(texture)
    }
}

impl Drop for WaylandUnderlay {
    fn drop(&mut self) {
        let state = self.0.get_mut();
        state.presenter = None;
        state.unmap();
        state.viewport.destroy();
        state.surface.destroy();
    }
}
