use std::sync::Arc;

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "windows"))]
use ala::ExternalTextureHandle;
use ala::{
    App, Bounds, ContentMask, Context, Corners, Element, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    Pixels, RenderImage, Window,
};
#[cfg(target_os = "macos")]
use core_video::pixel_buffer::CVPixelBuffer;

#[cfg(target_os = "macos")]
use crate::browser::underlay::{self, UnderlayHandle};
use crate::{browser::view::BrowserView, diagnostics, pane};

const DIAGNOSTIC_TARGET: &str = "zz::diagnostics::browser_render";

pub(crate) trait BrowserSurfaceHost: EntityInputHandler + 'static {
    fn surface_bounds(&mut self, bounds: Bounds<Pixels>, window: &Window, cx: &mut Context<Self>);
    fn surface_focus(&self) -> FocusHandle;
    fn paints_page(&self) -> bool {
        true
    }
}

impl BrowserSurfaceHost for BrowserView {
    fn surface_bounds(&mut self, bounds: Bounds<Pixels>, window: &Window, cx: &mut Context<Self>) {
        self.update_content_bounds(bounds, window, cx);
    }

    fn surface_focus(&self) -> FocusHandle {
        self.focus_handle()
    }

    fn paints_page(&self) -> bool {
        !self.resize_covered()
    }
}

pub(crate) struct BrowserElement<V: BrowserSurfaceHost> {
    view: Entity<V>,
    image: Option<Arc<RenderImage>>,
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "windows"))]
    texture: Option<ExternalTextureHandle>,
    #[cfg(target_os = "macos")]
    surface: Option<CVPixelBuffer>,
    #[cfg(target_os = "macos")]
    underlay: Option<UnderlayHandle>,
    corner_radii: Corners<Pixels>,
}

impl<V: BrowserSurfaceHost> BrowserElement<V> {
    pub(crate) fn new(
        view: Entity<V>,
        image: Option<Arc<RenderImage>>,
        corner_radii: Corners<Pixels>,
    ) -> Self {
        Self {
            view,
            image,
            #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "windows"))]
            texture: None,
            #[cfg(target_os = "macos")]
            surface: None,
            #[cfg(target_os = "macos")]
            underlay: None,
            corner_radii,
        }
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "windows"))]
    pub(crate) fn texture(mut self, texture: Option<ExternalTextureHandle>) -> Self {
        self.texture = texture;
        self
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn surface(mut self, surface: Option<CVPixelBuffer>) -> Self {
        self.surface = surface;
        self
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn underlay(mut self, underlay: Option<UnderlayHandle>) -> Self {
        self.underlay = underlay;
        self
    }
}

impl<V: BrowserSurfaceHost> IntoElement for BrowserElement<V> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

pub(crate) struct BrowserPaintState {
    paints_page: bool,
    image: Option<Arc<RenderImage>>,
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "windows"))]
    texture: Option<ExternalTextureHandle>,
    #[cfg(target_os = "macos")]
    surface: Option<CVPixelBuffer>,
    #[cfg(target_os = "macos")]
    underlay: Option<UnderlayHandle>,
}

impl<V: BrowserSurfaceHost> Element for BrowserElement<V> {
    type RequestLayoutState = ();
    type PrepaintState = BrowserPaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (pane::fill_parent(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let started = diagnostics::timer(DIAGNOSTIC_TARGET);
        let paints_page = self.view.update(cx, |view, cx| {
            view.surface_bounds(bounds, window, cx);
            view.paints_page()
        });
        log::trace!(
            target: "zz::diagnostics::browser_render",
            "prepaint bounds={bounds:?} has_image={} image_strong_count={:?} scale_factor={} elapsed_us={}",
            self.image.is_some(),
            self.image.as_ref().map(Arc::strong_count),
            window.scale_factor(),
            diagnostics::elapsed_us(started),
        );
        BrowserPaintState {
            paints_page,
            image: self.image.clone(),
            #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "windows"))]
            texture: self.texture.clone(),
            #[cfg(target_os = "macos")]
            surface: self.surface.clone(),
            #[cfg(target_os = "macos")]
            underlay: self.underlay.clone(),
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut Self::RequestLayoutState,
        state: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let started = diagnostics::timer(DIAGNOSTIC_TARGET);
        let had_image = state.image.is_some();
        let image_strong_count = state.image.as_ref().map(Arc::strong_count);
        let focus = self.view.read(cx).surface_focus();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.view.clone()),
            cx,
        );
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            #[cfg(target_os = "macos")]
            if let Some(underlay) = state.underlay.take()
                && underlay::place(window, &underlay, bounds, self.corner_radii)
            {
                if !state.paints_page {
                    return;
                }
                if let Some(surface) = state.surface.take() {
                    window.paint_underlay(bounds, self.corner_radii, surface);
                    return;
                }
            }
            if !state.paints_page {
                return;
            }
            #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "windows"))]
            if let Some(texture) = state.texture.take() {
                window.paint_external_texture(bounds, self.corner_radii, texture);
                return;
            }
            #[cfg(target_os = "macos")]
            if let Some(surface) = state.surface.take() {
                window.paint_surface(bounds, self.corner_radii, surface);
                return;
            }
            if let Some(image) = state.image.take()
                && let Err(error) =
                    window.paint_image(bounds, bounds, self.corner_radii, image, 0, false)
            {
                log::warn!("failed to paint browser frame: {error}");
            }
        });
        log::trace!(
            target: "zz::diagnostics::browser_render",
            "paint bounds={bounds:?} had_image={had_image} image_strong_count={image_strong_count:?} focused={} elapsed_us={}",
            focus.is_focused(window),
            diagnostics::elapsed_us(started),
        );
    }
}
