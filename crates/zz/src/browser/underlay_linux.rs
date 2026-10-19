use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::{Rc, Weak},
};

use zz_gpui::{
    App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    Pixels, PlatformUnderlay, Position, Style, Window, WindowId, wgpu,
};

struct Layer {
    platform: Box<dyn PlatformUnderlay>,
    shown: Cell<bool>,
    painted: Cell<bool>,
    stale: Cell<bool>,
}

pub(crate) struct PaneUnderlay(Rc<Layer>);

#[derive(Clone)]
pub(crate) struct UnderlayHandle(Rc<Layer>);

thread_local! {
    static PLACED: RefCell<HashMap<WindowId, Vec<Weak<Layer>>>> = RefCell::new(HashMap::new());
    static UNAVAILABLE: Cell<bool> = const { Cell::new(false) };
    static LIVE: Cell<usize> = const { Cell::new(0) };
}

pub(crate) fn flush_needed() -> bool {
    LIVE.get() > 0 || PLACED.with_borrow(|placed| !placed.is_empty())
}

pub(crate) fn enabled() -> bool {
    !UNAVAILABLE.get() && std::env::var_os("ZZ_BROWSER_UNDERLAY").is_none_or(|value| value != "0")
}

impl PaneUnderlay {
    pub(crate) fn new(window: &Window) -> Option<Self> {
        let Some(platform) = window.create_underlay() else {
            UNAVAILABLE.set(true);
            return None;
        };
        LIVE.set(LIVE.get() + 1);
        Some(Self(Rc::new(Layer {
            platform,
            shown: Cell::new(false),
            painted: Cell::new(false),
            stale: Cell::new(true),
        })))
    }

    pub(crate) fn handle(&self) -> UnderlayHandle {
        UnderlayHandle(Rc::clone(&self.0))
    }

    pub(crate) fn present(&self, texture: &wgpu::Texture) -> bool {
        let layer = &self.0;
        if !layer.shown.get() {
            layer.stale.set(true);
            return false;
        }
        match layer.platform.present(texture) {
            Ok(presented) => {
                layer.stale.set(!presented);
                presented
            }
            Err(error) => {
                log::warn!("browser underlay present failed: {error:#}");
                layer.stale.set(true);
                false
            }
        }
    }
}

impl Drop for PaneUnderlay {
    fn drop(&mut self) {
        LIVE.set(LIVE.get().saturating_sub(1));
    }
}

pub(crate) fn place(
    window: &Window,
    underlay: &UnderlayHandle,
    bounds: Bounds<Pixels>,
    texture: &wgpu::Texture,
) {
    let layer = &underlay.0;
    window.place_underlay(layer.platform.as_ref(), bounds);
    if !layer.shown.replace(true) {
        layer.platform.set_visible(true);
        layer.stale.set(true);
    }
    if layer.stale.get() {
        match layer.platform.present(texture) {
            Ok(presented) => layer.stale.set(!presented),
            Err(error) => log::warn!("browser underlay present failed: {error:#}"),
        }
    }
    if !layer.painted.replace(true) {
        PLACED.with_borrow_mut(|placed| {
            let placed = placed
                .entry(window.window_handle().window_id())
                .or_default();
            if !placed
                .iter()
                .any(|entry| entry.as_ptr() == Rc::as_ptr(layer))
            {
                placed.push(Rc::downgrade(layer));
            }
        });
    }
}

fn finish_frame(window: &Window) {
    let window_id = window.window_handle().window_id();
    let any_painted = PLACED.with_borrow_mut(|windows| {
        let Some(placed) = windows.get_mut(&window_id) else {
            return false;
        };
        let mut any_painted = false;
        placed.retain(|layer| {
            let Some(layer) = layer.upgrade() else {
                return false;
            };
            if layer.painted.replace(false) {
                any_painted = true;
                true
            } else {
                if layer.shown.replace(false) {
                    layer.platform.set_visible(false);
                }
                false
            }
        });
        if placed.is_empty() {
            windows.remove(&window_id);
        }
        any_painted
    });
    window.set_underlay_active(any_painted);
}

pub(crate) struct UnderlayFlush;

impl IntoElement for UnderlayFlush {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for UnderlayFlush {
    type RequestLayoutState = ();
    type PrepaintState = ();

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
        let style = Style {
            position: Position::Absolute,
            ..Style::default()
        };
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        (): &mut Self::RequestLayoutState,
        _: &mut Window,
        _: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        (): &mut Self::RequestLayoutState,
        (): &mut Self::PrepaintState,
        window: &mut Window,
        _: &mut App,
    ) {
        finish_frame(window);
    }
}
