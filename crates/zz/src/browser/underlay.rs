use std::{
    cell::RefCell,
    collections::{HashMap, hash_map::Entry},
    f64::consts::FRAC_PI_2,
    sync::{Arc, Mutex, PoisonError, Weak},
};

use ala::{
    App, Bounds, Corners, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Position, Style, Window,
};
use ala_cef::{MacFramePresenter, MacIoSurface};
use objc2::{MainThreadMarker, MainThreadOnly as _, rc::Retained, runtime::AnyObject};
use objc2_app_kit::{NSAutoresizingMaskOptions, NSView, NSWindowOrderingMode};
use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGMutablePath;
use objc2_quartz_core::{CALayer, CAShapeLayer, CATransaction, kCAGravityResize};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

const CORNER_SEGMENTS: u32 = 16;

type MaskShape = (f64, f64, [f64; 4]);

struct Layer(Retained<CALayer>, Mutex<Option<MaskShape>>);

#[allow(
    unsafe_code,
    reason = "CALayer is shared with Metal's completion thread"
)]
unsafe impl Send for Layer {}
#[allow(
    unsafe_code,
    reason = "CALayer is shared with Metal's completion thread"
)]
unsafe impl Sync for Layer {}

impl Layer {
    #[allow(
        unsafe_code,
        reason = "an IOSurfaceRef is toll-free bridged to the IOSurface object CALayer.contents takes"
    )]
    fn show(&self, surface: &MacIoSurface) {
        let contents = unsafe { &*surface.as_ptr().cast::<AnyObject>() };
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        unsafe { self.0.setContents(Some(contents)) };
        CATransaction::commit();
    }

    #[allow(
        unsafe_code,
        reason = "CALayer.mask is marked unsafe in objc2-quartz-core"
    )]
    fn round(&self, size: CGSize, radii: [f64; 4]) {
        let shape = (size.width, size.height, radii);
        let mut current = self.1.lock().unwrap_or_else(PoisonError::into_inner);
        if *current == Some(shape) {
            return;
        }
        *current = Some(shape);
        if radii.iter().all(|radius| *radius <= 0.0) {
            unsafe { self.0.setMask(None) };
            return;
        }
        let mask = CAShapeLayer::new();
        mask.setFrame(CGRect::new(CGPoint::new(0.0, 0.0), size));
        mask.setPath(Some(&corner_path(
            size.width,
            size.height,
            radii,
            f64::from(penna::CORNER_SMOOTHING),
        )));
        unsafe { self.0.setMask(Some(&mask)) };
    }
}

#[allow(
    unsafe_code,
    reason = "CGPath construction takes a raw transform pointer, null here"
)]
fn corner_path(
    width: f64,
    height: f64,
    [top_left, top_right, bottom_right, bottom_left]: [f64; 4],
    smoothing: f64,
) -> CFRetained<CGMutablePath> {
    let path = CGMutablePath::new();
    let half_minor = width.min(height) / 2.0;
    let corners = [
        (top_left, 0.0, 0.0, -1.0, -1.0, false),
        (top_right, width, 0.0, 1.0, -1.0, true),
        (bottom_right, width, height, 1.0, 1.0, false),
        (bottom_left, 0.0, height, -1.0, 1.0, true),
    ];
    let mut first = true;
    let mut point = |x: f64, y: f64| {
        let y = height - y;
        unsafe {
            if first {
                CGMutablePath::move_to_point(Some(&path), std::ptr::null(), x, y);
            } else {
                CGMutablePath::add_line_to_point(Some(&path), std::ptr::null(), x, y);
            }
        }
        first = false;
    };
    for (radius, corner_x, corner_y, out_x, out_y, reversed) in corners {
        let radius = radius.clamp(0.0, half_minor);
        if radius <= 0.0 {
            point(corner_x, corner_y);
            continue;
        }
        let exponent = if radius >= half_minor - 0.01 {
            2.0
        } else {
            smoothing
        };
        let center_x = corner_x - out_x * radius;
        let center_y = corner_y - out_y * radius;
        for step in 0..=CORNER_SEGMENTS {
            let step = if reversed {
                CORNER_SEGMENTS - step
            } else {
                step
            };
            let angle = FRAC_PI_2 * f64::from(step) / f64::from(CORNER_SEGMENTS);
            let reach = |value: f64| radius * value.max(0.0).powf(2.0 / exponent);
            point(
                center_x + out_x * reach(angle.cos()),
                center_y + out_y * reach(angle.sin()),
            );
        }
    }
    CGMutablePath::close_subpath(Some(&path));
    path
}

pub(crate) struct PaneUnderlay(Arc<Layer>);

#[derive(Clone)]
pub(crate) struct UnderlayHandle(Arc<Layer>);

impl PaneUnderlay {
    #[allow(unsafe_code, reason = "reads an immutable Core Animation constant")]
    pub(crate) fn new(first_frame: &MacIoSurface) -> Self {
        let layer = CALayer::new();
        layer.setContentsGravity(unsafe { kCAGravityResize });
        layer.setMasksToBounds(true);
        layer.setHidden(true);
        let layer = Arc::new(Layer(layer, Mutex::new(None)));
        layer.show(first_frame);
        Self(layer)
    }

    pub(crate) fn presenter(&self) -> MacFramePresenter {
        let layer = Arc::clone(&self.0);
        Arc::new(move |surface| layer.show(surface))
    }

    pub(crate) fn handle(&self) -> UnderlayHandle {
        UnderlayHandle(Arc::clone(&self.0))
    }
}

impl Drop for PaneUnderlay {
    fn drop(&mut self) {
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        self.0.0.removeFromSuperlayer();
        CATransaction::commit();
    }
}

struct Host {
    view: Retained<NSView>,
    root: Retained<CALayer>,
    placed: Vec<(Weak<Layer>, bool)>,
}

impl Host {
    fn new(ala_view: &NSView, mtm: MainThreadMarker) -> Option<Self> {
        let content = ala_view_superview(ala_view)?;
        let root = CALayer::new();
        let view = NSView::initWithFrame(NSView::alloc(mtm), content.bounds());
        view.setLayer(Some(&root));
        view.setWantsLayer(true);
        view.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        content.addSubview_positioned_relativeTo(
            &view,
            NSWindowOrderingMode::Below,
            Some(ala_view),
        );
        Some(Self {
            view,
            root,
            placed: Vec::new(),
        })
    }

    fn serves(&self, ala_view: &NSView) -> bool {
        match (ala_view_superview(&self.view), ala_view_superview(ala_view)) {
            (Some(ours), Some(theirs)) => Retained::as_ptr(&ours) == Retained::as_ptr(&theirs),
            _ => false,
        }
    }
}

thread_local! {
    static HOSTS: RefCell<HashMap<usize, Host>> = RefCell::new(HashMap::new());
}

#[allow(
    unsafe_code,
    reason = "NSView.superview is marked unsafe in objc2-app-kit"
)]
fn ala_view_superview(view: &NSView) -> Option<Retained<NSView>> {
    unsafe { view.superview() }
}

#[allow(
    unsafe_code,
    reason = "raw-window-handle hands out the window's NSView as a raw pointer"
)]
fn ala_view(window: &Window) -> Option<Retained<NSView>> {
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    unsafe { Retained::retain(handle.ns_view.as_ptr().cast()) }
}

pub(crate) fn place(
    window: &Window,
    underlay: &UnderlayHandle,
    bounds: Bounds<Pixels>,
    radii: Corners<Pixels>,
) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let Some(ala_view) = ala_view(window) else {
        return false;
    };
    let key = Retained::as_ptr(&ala_view).addr();
    HOSTS.with_borrow_mut(|hosts| {
        if hosts.get(&key).is_some_and(|host| !host.serves(&ala_view)) {
            hosts.remove(&key);
        }
        let host = match hosts.entry(key) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let Some(host) = Host::new(&ala_view, mtm) else {
                    return false;
                };
                entry.insert(host)
            }
        };
        let layer = &underlay.0.0;
        CATransaction::setDisableActions(true);
        if layer
            .superlayer()
            .is_none_or(|parent| Retained::as_ptr(&parent) != Retained::as_ptr(&host.root))
        {
            host.root.addSublayer(layer);
        }
        let zoom = f64::from(window.zoom());
        let point = |value: Pixels| f64::from(f32::from(value)) * zoom;
        layer.setContentsScale(f64::from(window.scale_factor()) / zoom);
        let height = point(bounds.size.height);
        let y = host.view.bounds().size.height - point(bounds.origin.y) - height;
        let size = CGSize::new(point(bounds.size.width), height);
        layer.setFrame(CGRect::new(CGPoint::new(point(bounds.origin.x), y), size));
        underlay.0.round(
            size,
            [
                point(radii.top_left),
                point(radii.top_right),
                point(radii.bottom_right),
                point(radii.bottom_left),
            ],
        );
        layer.setHidden(false);
        match host
            .placed
            .iter_mut()
            .find(|(placed, _)| placed.as_ptr() == Arc::as_ptr(&underlay.0))
        {
            Some((_, painted)) => *painted = true,
            None => host.placed.push((Arc::downgrade(&underlay.0), true)),
        }
        true
    })
}

fn finish_frame(window: &Window) {
    let Some(ala_view) = ala_view(window) else {
        return;
    };
    let key = Retained::as_ptr(&ala_view).addr();
    let any_painted = HOSTS.with_borrow_mut(|hosts| {
        let host = hosts.get_mut(&key)?;
        let mut any_painted = false;
        CATransaction::setDisableActions(true);
        host.placed.retain_mut(|(layer, painted)| {
            let Some(layer) = layer.upgrade() else {
                return false;
            };
            if *painted {
                any_painted = true;
            } else {
                layer.0.setHidden(true);
            }
            *painted = false;
            true
        });
        Some(any_painted)
    });
    if let Some(any_painted) = any_painted {
        window.set_underlay_active(any_painted);
    }
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
