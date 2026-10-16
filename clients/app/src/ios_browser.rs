#![allow(
    unsafe_code,
    unsafe_op_in_unsafe_fn,
    non_upper_case_globals,
    unexpected_cfgs
)]

use std::{
    cell::{Cell, RefCell},
    ffi::CString,
    rc::Rc,
    sync::Arc,
};

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{BOOL, Class, NO, Object, Protocol, Sel, YES},
    sel, sel_impl,
};
use raw_window_handle::RawWindowHandle;
use zz_daemon_client::InteractiveClient;
use zz_gpui::{Bounds, Pixels, Point, RenderImage, Window};

pub use zz_client::element_picker::ElementPickerAppearance;
use zz_client::element_picker::{
    ElementPickOutcome, ElementPickState, PickGeometry, element_picker_start_script,
};
use zz_gpui_platform::ios::{
    CGPoint, CGRect, CGSize, id, nil, ns_array, ns_string, nsstring_to_string,
};

const OBSERVED: &[&str] = &["URL", "title", "loading", "canGoBack", "canGoForward"];
const SCROLL_OFFSET: &str = "contentOffset";
const HOLE: [&str; 4] = ["holeX", "holeY", "holeWidth", "holeHeight"];
const PULL_TO_REFRESH: f64 = 80.0;
const GESTURE_CHANGED: isize = 2;
const GESTURE_ENDED: isize = 3;
const BGRA_PREMULTIPLIED: u32 = 2 | 2 << 12;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct UIEdgeInsets {
    top: f64,
    left: f64,
    bottom: f64,
    right: f64,
}

unsafe impl objc::Encode for UIEdgeInsets {
    fn encode() -> objc::Encoding {
        unsafe { objc::Encoding::from_str("{UIEdgeInsets=dddd}") }
    }
}

/// Where the web view sits and what floats over it: its frame, its lower
/// corners, how much of its bottom edge app UI covers, and the cutout that
/// UI shows through, all in window points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrowserMount {
    pub bounds: Bounds<Pixels>,
    pub radius: Pixels,
    pub covered_bottom: Pixels,
    pub hole: Option<(Bounds<Pixels>, Pixels)>,
    pub backdrop: Option<Bounds<Pixels>>,
}
use zz_client::element_picker::ELEMENT_PICKER_SCRIPT as PICKER_SCRIPT;
const PICKER_HANDLER: &str = "zzElementPicker";

#[derive(Clone, Debug)]
pub enum BrowserEvent {
    Changed {
        url: Option<String>,
        title: String,
        loading: bool,
        can_go_back: bool,
        can_go_forward: bool,
        committed: bool,
    },
    Error(String),
    Open(String),
    Focused,
    ElementPicked {
        text: Arc<str>,
        screenshot: Option<Vec<u8>>,
    },
    ElementPickCancelled,
    ElementPickFailed,
    Scrolled {
        travel: f32,
        at_top: bool,
    },
    Refresh,
    Backdrop {
        bounds: Bounds<Pixels>,
        image: Arc<RenderImage>,
    },
}

pub struct BrowserProfile {
    store: id,
    ssh: bool,
    client: RefCell<Option<Arc<InteractiveClient>>>,
    #[allow(clippy::option_option)]
    route: RefCell<Option<Option<u16>>>,
    configured: Cell<bool>,
}

impl BrowserProfile {
    #[must_use]
    pub fn new(host: &str, profile: &str) -> Rc<Self> {
        unsafe {
            let defaults: id = msg_send![class!(NSUserDefaults), standardUserDefaults];
            let key = ns_string(&format!(
                "zz.browser.profile:{}:{host}{profile}",
                host.len()
            ));
            let saved: id = msg_send![defaults, stringForKey: key];
            let identifier: id = if saved.is_null() {
                let identifier: id = msg_send![class!(NSUUID), UUID];
                let value: id = msg_send![identifier, UUIDString];
                let _: () = msg_send![defaults, setObject: value forKey: key];
                identifier
            } else {
                let identifier: id = msg_send![class!(NSUUID), alloc];
                let identifier: id = msg_send![identifier, initWithUUIDString: saved];
                msg_send![identifier, autorelease]
            };
            let store: id =
                msg_send![class!(WKWebsiteDataStore), dataStoreForIdentifier: identifier];
            let _: id = msg_send![store, retain];
            let profile = Rc::new(Self {
                store,
                ssh: host.starts_with("ssh://"),
                client: RefCell::new(None),
                route: RefCell::new(None),
                configured: Cell::new(false),
            });
            profile.set_client(None);
            profile
        }
    }

    pub fn set_client(&self, client: Option<Arc<InteractiveClient>>) -> bool {
        let route = client.as_ref().and_then(|client| {
            if self.ssh {
                client.socks_port().map(Some)
            } else {
                Some(None)
            }
        });
        *self.client.borrow_mut() = client;
        if self.configured.replace(true) && *self.route.borrow() == route {
            return false;
        }
        *self.route.borrow_mut() = route;
        unsafe {
            if route == Some(None) {
                let _: () = msg_send![self.store, setProxyConfigurations: ns_array(&[])];
            } else {
                let port = CString::new(route.flatten().unwrap_or(9).to_string()).unwrap();
                let endpoint = nw_endpoint_create_host(c"127.0.0.1".as_ptr(), port.as_ptr());
                let proxy = nw_proxy_config_create_socksv5(endpoint);
                nw_proxy_config_set_failover_allowed(proxy, false);
                for domain in [c"localhost", c"localhost.", c"127.0.0.1", c"::1"] {
                    nw_proxy_config_add_excluded_domain(proxy, domain.as_ptr());
                }
                let _: () = msg_send![self.store, setProxyConfigurations: ns_array(&[proxy])];
                let _: () = msg_send![proxy, release];
                let _: () = msg_send![endpoint, release];
            }
        }
        true
    }

    pub fn available(&self) -> bool {
        self.route.borrow().is_some()
    }

    unsafe fn prepare(&self, url: id) -> Result<(), String> {
        if !self.available() {
            return Err("Waiting for the host connection…".into());
        }
        if url.is_null() {
            return Err("Enter an HTTP or HTTPS address.".into());
        }
        let scheme: id = msg_send![url, scheme];
        let scheme = nsstring_to_string(scheme)
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !matches!(
            scheme.as_str(),
            "http" | "https" | "about" | "blob" | "data"
        ) {
            return Err("This address cannot open in the browser pane.".into());
        }
        if !self.ssh || !matches!(scheme.as_str(), "http" | "https") {
            return Ok(());
        }
        let host: id = msg_send![url, host];
        let host = nsstring_to_string(host)
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(
            host.as_str(),
            "localhost" | "localhost." | "127.0.0.1" | "::1" | "[::1]"
        ) {
            let number: id = msg_send![url, port];
            let port: u64 = if number.is_null() {
                if scheme == "https" { 443 } else { 80 }
            } else {
                msg_send![number, unsignedLongLongValue]
            };
            let port = u16::try_from(port)
                .ok()
                .filter(|port| *port != 0)
                .ok_or("Enter a port between 1 and 65535.")?;
            self.client
                .borrow()
                .as_ref()
                .ok_or("The host connection is unavailable.")?
                .forward_loopback(port)
                .map_err(|error| format!("Cannot forward localhost:{port}: {error}"))?;
        } else if host.starts_with("127.") || host == "0.0.0.0" {
            return Err("Use localhost to reach this host’s loopback server.".into());
        }
        Ok(())
    }
}

impl Drop for BrowserProfile {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![self.store, release];
        }
    }
}

struct DelegateState {
    profile: Rc<BrowserProfile>,
    events: UnboundedSender<BrowserEvent>,
    picker: ElementPickState,
    generation: Cell<u64>,
    picking: Cell<bool>,
    offset: Cell<f64>,
    refreshing: Cell<bool>,
    view: Cell<id>,
    backdrop: Cell<Option<(Bounds<Pixels>, CGRect)>>,
    snapshotting: Cell<bool>,
    stale: Cell<bool>,
}

pub struct BrowserView {
    view: id,
    host: id,
    mask: id,
    spinner: id,
    delegate: id,
    mounted: Cell<Option<BrowserMount>>,
    _state: Rc<DelegateState>,
}

impl BrowserView {
    pub fn new(profile: Rc<BrowserProfile>) -> (Rc<Self>, UnboundedReceiver<BrowserEvent>) {
        let (events, receiver) = unbounded();
        let state = Rc::new(DelegateState {
            profile,
            events,
            picker: ElementPickState::default(),
            generation: Cell::new(0),
            picking: Cell::new(false),
            offset: Cell::new(0.0),
            refreshing: Cell::new(false),
            view: Cell::new(nil),
            backdrop: Cell::new(None),
            snapshotting: Cell::new(false),
            stale: Cell::new(false),
        });
        unsafe {
            let configuration: id = msg_send![class!(WKWebViewConfiguration), new];
            let _: () = msg_send![configuration, setWebsiteDataStore: state.profile.store];
            let _: () = msg_send![configuration, setAllowsInlineMediaPlayback: YES];
            let view: id = msg_send![class!(WKWebView), alloc];
            let view: id =
                msg_send![view, initWithFrame: CGRect::default() configuration: configuration];
            let _: () = msg_send![configuration, release];
            let _: () = msg_send![view, setAllowsBackForwardNavigationGestures: YES];
            let _: () = msg_send![view, setInspectable: YES];
            state.view.set(view);
            let host: id = msg_send![host_class(), new];
            let _: () = msg_send![host, setHidden: YES];
            let _: () = msg_send![host, addSubview: view];
            let mask: id = msg_send![class!(CAShapeLayer), new];
            let _: () = msg_send![mask, setFillRule: ns_string("even-odd")];
            let scroll: id = msg_send![view, scrollView];
            let _: () = msg_send![scroll, setContentInsetAdjustmentBehavior: 2isize];
            let delegate: id = msg_send![delegate_class(), new];
            (*delegate).set_ivar("state", (&raw const *state) as usize);
            let _: () = msg_send![view, setNavigationDelegate: delegate];
            let _: () = msg_send![view, setUIDelegate: delegate];
            let configuration: id = msg_send![view, configuration];
            let controller: id = msg_send![configuration, userContentController];
            let _: () = msg_send![controller, addScriptMessageHandler: delegate name: ns_string(PICKER_HANDLER)];
            for key in OBSERVED {
                let _: () = msg_send![view, addObserver: delegate forKeyPath: ns_string(key) options: 0usize context: std::ptr::null_mut::<std::ffi::c_void>()];
            }
            let _: () = msg_send![scroll, addObserver: delegate forKeyPath: ns_string(SCROLL_OFFSET) options: 0usize context: std::ptr::null_mut::<std::ffi::c_void>()];
            let tap: id = msg_send![class!(UITapGestureRecognizer), alloc];
            let tap: id = msg_send![tap, initWithTarget: delegate action: sel!(pageTapped:)];
            let _: () = msg_send![tap, setCancelsTouchesInView: NO];
            let _: () = msg_send![tap, setDelegate: delegate];
            let _: () = msg_send![view, addGestureRecognizer: tap];
            let _: () = msg_send![tap, release];
            let pan: id = msg_send![scroll, panGestureRecognizer];
            let _: () = msg_send![pan, addTarget: delegate action: sel!(pagePanned:)];
            let spinner: id = msg_send![class!(UIActivityIndicatorView), alloc];
            let spinner: id = msg_send![spinner, initWithActivityIndicatorStyle: 100isize];
            let gray: id = msg_send![class!(UIColor), systemGrayColor];
            let _: () = msg_send![spinner, setColor: gray];
            let _: () = msg_send![spinner, setHidesWhenStopped: YES];
            let _: () = msg_send![host, addSubview: spinner];
            (*delegate).set_ivar("spinner", spinner as usize);
            (
                Rc::new(Self {
                    view,
                    host,
                    mask,
                    spinner,
                    delegate,
                    mounted: Cell::new(None),
                    _state: state,
                }),
                receiver,
            )
        }
    }

    pub fn load(&self, address: &str) -> Result<(), String> {
        self.cancel_element_pick();
        if address.contains('\0') {
            return Err("The address contains an invalid character.".into());
        }
        unsafe {
            let url: id = msg_send![class!(NSURL), URLWithString: ns_string(address)];
            if url.is_null() {
                return Err("Enter an HTTP or HTTPS address.".into());
            }
            self._state.profile.prepare(url)?;
            let request: id = msg_send![class!(NSURLRequest), requestWithURL: url];
            let _: id = msg_send![self.view, loadRequest: request];
        }
        Ok(())
    }

    pub fn back(&self) {
        self.cancel_element_pick();
        unsafe {
            let _: id = msg_send![self.view, goBack];
        }
    }
    pub fn forward(&self) {
        self.cancel_element_pick();
        unsafe {
            let _: id = msg_send![self.view, goForward];
        }
    }
    pub fn reload(&self) {
        self.cancel_element_pick();
        unsafe {
            let _: id = msg_send![self.view, reload];
        }
    }
    pub fn stop(&self) {
        self.cancel_element_pick();
        unsafe {
            let _: () = msg_send![self.view, stopLoading];
        }
    }

    pub fn set_visible(&self, visible: bool) {
        unsafe {
            let hidden: BOOL = msg_send![self.host, isHidden];
            if (hidden == YES) == visible {
                if !visible {
                    self.cancel_element_pick();
                    let _: BOOL = msg_send![self.view, endEditing: YES];
                }
                let _: () = msg_send![self.host, setHidden: if visible { NO } else { YES }];
                if visible {
                    request_backdrop(&self._state);
                }
            }
        }
    }

    pub fn set_interactive(&self, interactive: bool) {
        if !interactive {
            self.cancel_element_pick();
        }
        unsafe {
            let _: () =
                msg_send![self.view, setUserInteractionEnabled: if interactive { YES } else { NO }];
        }
    }

    pub fn start_element_pick(&self, appearance: &ElementPickerAppearance) -> Result<(), String> {
        self.cancel_element_pick();
        let token = self
            ._state
            .picker
            .begin()
            .ok_or("Could not start the element picker.")?;
        let start =
            element_picker_start_script(&token, appearance).map_err(|error| error.to_string())?;
        let script = format!(
            "globalThis.__zzElementPickerQuery = o => {{ window.webkit.messageHandlers.{PICKER_HANDLER}.postMessage(o.request); o.onSuccess?.(''); }};\n{PICKER_SCRIPT}\n{start}"
        );
        self._state.picking.set(true);
        let state = self._state.clone();
        let generation = state.generation.get();
        let completion = block2::RcBlock::new(
            move |_: *mut std::ffi::c_void, error: *mut std::ffi::c_void| {
                if !error.is_null() && state.generation.get() == generation {
                    state.picker.cancel();
                    state.picking.set(false);
                    let _ = state.events.unbounded_send(BrowserEvent::ElementPickFailed);
                }
            },
        );
        unsafe {
            let _: () = msg_send![self.view, evaluateJavaScript: ns_string(&script) completionHandler: &*completion];
        }
        Ok(())
    }

    pub fn cancel_element_pick(&self) {
        unsafe {
            cancel_picker(&self._state, self.view);
        }
    }

    pub fn point_element_pick(&self, at: Point<Pixels>) -> bool {
        let Some(mount) = self.mounted.get().filter(|_| self._state.picking.get()) else {
            return false;
        };
        let local = at - mount.bounds.origin;
        let script = format!(
            "globalThis.__zzElementPicker?.point({},{},{});",
            f32::from(local.x),
            f32::from(local.y),
            f32::from(mount.bounds.size.width)
        );
        unsafe {
            let _: () =
                msg_send![self.view, evaluateJavaScript: ns_string(&script) completionHandler: nil];
        }
        mount.bounds.contains(&at)
    }

    pub fn release_element_pick(&self) {
        if !self._state.picking.get() {
            return;
        }
        unsafe {
            let _: () = msg_send![self.view, evaluateJavaScript: ns_string("globalThis.__zzElementPicker?.release();") completionHandler: nil];
        }
    }

    pub fn refresh_backdrop(&self) {
        unsafe {
            request_backdrop(&self._state);
        }
    }

    pub fn end_refresh(&self) {
        self._state.refreshing.set(false);
        unsafe {
            let _: () = msg_send![self.spinner, stopAnimating];
        }
    }

    pub fn mount(&self, mount: BrowserMount, window: &Window) {
        let Ok(handle) = raw_window_handle::HasWindowHandle::window_handle(window) else {
            return;
        };
        let RawWindowHandle::UiKit(handle) = handle.as_raw() else {
            return;
        };
        unsafe {
            let parent = handle.ui_view.as_ptr() as id;
            let previous: id = msg_send![self.host, superview];
            if previous != parent {
                let _: () = msg_send![parent, addSubview: self.host];
                self.mounted.set(None);
            }
            if self.mounted.get() == Some(mount) {
                return;
            }
            self.mounted.set(Some(mount));
            let native_scale: f64 = msg_send![parent, contentScaleFactor];
            let scale = f64::from(window.scale_factor()) / native_scale;
            let points = |value: Pixels| f64::from(f32::from(value)) * scale;
            let bounds = mount.bounds;
            let frame = CGRect {
                origin: CGPoint {
                    x: points(bounds.origin.x),
                    y: points(bounds.origin.y),
                },
                size: CGSize {
                    width: points(bounds.size.width),
                    height: points(bounds.size.height),
                },
            };
            let local = CGRect {
                origin: CGPoint::default(),
                size: frame.size,
            };
            let _: () = msg_send![class!(CATransaction), begin];
            let _: () = msg_send![class!(CATransaction), setDisableActions: YES];
            let _: () = msg_send![self.host, setFrame: frame];
            let _: () = msg_send![self.view, setFrame: local];
            let _: () =
                msg_send![self.spinner, setCenter: CGPoint { x: local.size.width / 2.0, y: 28.0 }];
            let layer: id = msg_send![self.host, layer];
            let _: () = msg_send![layer, setCornerRadius: points(mount.radius)];
            let _: () = msg_send![layer, setMaskedCorners: 12usize];
            let _: () = msg_send![layer, setMasksToBounds: YES];
            let hole = mount.hole.map(|(hole, radius)| {
                let rect = CGRect {
                    origin: CGPoint {
                        x: points(hole.origin.x - bounds.origin.x),
                        y: points(hole.origin.y - bounds.origin.y),
                    },
                    size: CGSize {
                        width: points(hole.size.width),
                        height: points(hole.size.height),
                    },
                };
                (rect, points(radius).min(rect.size.height / 2.0))
            });
            let (x, y, width, height) = hole.map_or((0.0, 0.0, 0.0, 0.0), |(rect, _)| {
                (
                    rect.origin.x,
                    rect.origin.y,
                    rect.size.width,
                    rect.size.height,
                )
            });
            for (name, value) in HOLE.iter().zip([x, y, width, height]) {
                (*self.host).set_ivar::<f64>(name, value);
            }
            if let Some((rect, radius)) = hole {
                let path: id = msg_send![class!(UIBezierPath), bezierPathWithRect: local];
                let cutout: id = msg_send![class!(UIBezierPath), bezierPathWithRoundedRect: rect cornerRadius: radius];
                let _: () = msg_send![path, appendPath: cutout];
                let shape: id = msg_send![path, CGPath];
                let _: () = msg_send![self.mask, setFrame: local];
                let _: () = msg_send![self.mask, setPath: shape];
                let _: () = msg_send![layer, setMask: self.mask];
            } else {
                let _: () = msg_send![layer, setMask: nil];
            }
            let _: () = msg_send![class!(CATransaction), commit];
            let insets = UIEdgeInsets {
                bottom: points(mount.covered_bottom),
                ..UIEdgeInsets::default()
            };
            let scroll: id = msg_send![self.view, scrollView];
            let _: () = msg_send![scroll, setContentInset: insets];
            let _: () = msg_send![scroll, setVerticalScrollIndicatorInsets: insets];
            self._state.backdrop.set(mount.backdrop.map(|strip| {
                let rect = CGRect {
                    origin: CGPoint {
                        x: points(strip.origin.x - bounds.origin.x),
                        y: points(strip.origin.y - bounds.origin.y),
                    },
                    size: CGSize {
                        width: points(strip.size.width),
                        height: points(strip.size.height),
                    },
                };
                (strip, rect)
            }));
            request_backdrop(&self._state);
        }
    }
}

impl Drop for BrowserView {
    fn drop(&mut self) {
        self.cancel_element_pick();
        self._state.view.set(nil);
        self._state.backdrop.set(None);
        unsafe {
            let scroll: id = msg_send![self.view, scrollView];
            let pan: id = msg_send![scroll, panGestureRecognizer];
            let _: () = msg_send![pan, removeTarget: self.delegate action: sel!(pagePanned:)];
            self.stop();
            let _: () = msg_send![self.view, setNavigationDelegate: nil];
            let _: () = msg_send![self.view, setUIDelegate: nil];
            let configuration: id = msg_send![self.view, configuration];
            let controller: id = msg_send![configuration, userContentController];
            let _: () =
                msg_send![controller, removeScriptMessageHandlerForName: ns_string(PICKER_HANDLER)];
            for key in OBSERVED {
                let _: () =
                    msg_send![self.view, removeObserver: self.delegate forKeyPath: ns_string(key)];
            }
            let scroll: id = msg_send![self.view, scrollView];
            let _: () = msg_send![scroll, removeObserver: self.delegate forKeyPath: ns_string(SCROLL_OFFSET)];
            let _: () = msg_send![self.view, removeFromSuperview];
            let _: () = msg_send![self.host, removeFromSuperview];
            let _: () = msg_send![self.view, release];
            let _: () = msg_send![self.host, release];
            let _: () = msg_send![self.mask, release];
            let _: () = msg_send![self.spinner, release];
            let _: () = msg_send![self.delegate, release];
        }
    }
}

unsafe fn state(this: &Object) -> Rc<DelegateState> {
    let pointer = *this.get_ivar::<usize>("state") as *const DelegateState;
    Rc::increment_strong_count(pointer);
    Rc::from_raw(pointer)
}

unsafe fn cancel_picker(state: &DelegateState, view: id) {
    if !state.picking.replace(false) {
        return;
    }
    state.generation.set(state.generation.get().wrapping_add(1));
    state.picker.cancel();
    let _: () = msg_send![view, evaluateJavaScript: ns_string("globalThis.__zzElementPicker?.cancel();") completionHandler: nil];
    let _ = state
        .events
        .unbounded_send(BrowserEvent::ElementPickCancelled);
}

extern "C" fn navigation_started(this: &Object, _: Sel, view: id, _: id) {
    unsafe {
        cancel_picker(&state(this), view);
    }
}

extern "C" fn picker_message(this: &Object, _: Sel, _: id, message: id) {
    unsafe {
        let frame: id = msg_send![message, frameInfo];
        let main: BOOL = msg_send![frame, isMainFrame];
        let body: id = msg_send![message, body];
        let is_string: BOOL = msg_send![body, isKindOfClass: class!(NSString)];
        if main != YES || is_string != YES {
            return;
        }
        let Some(request) = nsstring_to_string(body) else {
            return;
        };
        let state = state(this);
        let Ok(outcome) = state.picker.consume(&request) else {
            return;
        };
        match outcome {
            ElementPickOutcome::Picked(text, geometry) => {
                let view: id = msg_send![message, webView];
                let generation = state.generation.get();
                snapshot(view, geometry, move |screenshot| {
                    if state.generation.get() == generation {
                        state.picking.set(false);
                        let _ = state.events.unbounded_send(BrowserEvent::ElementPicked {
                            text: text.clone(),
                            screenshot,
                        });
                    }
                });
            }
            ElementPickOutcome::Cancelled | ElementPickOutcome::Failed => {
                state.picking.set(false);
                let event = if matches!(outcome, ElementPickOutcome::Cancelled) {
                    BrowserEvent::ElementPickCancelled
                } else {
                    BrowserEvent::ElementPickFailed
                };
                let _ = state.events.unbounded_send(event);
            }
        }
    }
}

unsafe fn snapshot(
    view: id,
    geometry: Option<PickGeometry>,
    complete: impl Fn(Option<Vec<u8>>) + 'static,
) {
    let Some(geometry) = geometry.filter(|_| !view.is_null()) else {
        complete(None);
        return;
    };
    let bounds: CGRect = msg_send![view, bounds];
    let (offset_x, offset_y, width, height) = geometry.visual_viewport.map_or(
        (0.0, 0.0, geometry.viewport_width, geometry.viewport_height),
        |viewport| {
            (
                viewport.offset_left,
                viewport.offset_top,
                viewport.width,
                viewport.height,
            )
        },
    );
    let scale_x = bounds.size.width / width;
    let scale_y = bounds.size.height / height;
    let x = ((geometry.x - offset_x) * scale_x).max(0.0);
    let y = ((geometry.y - offset_y) * scale_y).max(0.0);
    let right = ((geometry.x - offset_x + geometry.width) * scale_x).min(bounds.size.width);
    let bottom = ((geometry.y - offset_y + geometry.height) * scale_y).min(bounds.size.height);
    if right <= x || bottom <= y {
        complete(None);
        return;
    }
    let rect = CGRect {
        origin: CGPoint { x, y },
        size: CGSize {
            width: right - x,
            height: bottom - y,
        },
    };
    let configuration: id = msg_send![class!(WKSnapshotConfiguration), new];
    let _: () = msg_send![configuration, setRect: rect];
    let _: () = msg_send![configuration, setAfterScreenUpdates: YES];
    let completion = block2::RcBlock::new(
        move |image: *mut std::ffi::c_void, _: *mut std::ffi::c_void| {
            let image = image as id;
            let data = if image.is_null() {
                nil
            } else {
                UIImagePNGRepresentation(image)
            };
            let length: usize = if data.is_null() {
                0
            } else {
                msg_send![data, length]
            };
            let screenshot = if length > 0 && length <= 16 * 1024 * 1024 {
                let bytes: *const u8 = msg_send![data, bytes];
                (!bytes.is_null()).then(|| std::slice::from_raw_parts(bytes, length).to_vec())
            } else {
                None
            };
            complete(screenshot);
        },
    );
    let _: () = msg_send![view, takeSnapshotWithConfiguration: configuration completionHandler: &*completion];
    let _: () = msg_send![configuration, release];
}

unsafe fn request_backdrop(state: &Rc<DelegateState>) {
    let view = state.view.get();
    let Some((bounds, rect)) = state.backdrop.get().filter(|_| !view.is_null()) else {
        return;
    };
    let host: id = msg_send![view, superview];
    let hidden: BOOL = if host.is_null() {
        YES
    } else {
        msg_send![host, isHidden]
    };
    if hidden == YES || rect.size.width < 1.0 || rect.size.height < 1.0 {
        return;
    }
    if state.snapshotting.replace(true) {
        state.stale.set(true);
        return;
    }
    state.stale.set(false);
    let width = rect.size.width.round() as usize;
    let height = rect.size.height.round() as usize;
    let configuration: id = msg_send![class!(WKSnapshotConfiguration), new];
    let _: () = msg_send![configuration, setRect: rect];
    let _: () = msg_send![configuration, setAfterScreenUpdates: NO];
    let snapshot_width: id = msg_send![class!(NSNumber), numberWithDouble: rect.size.width];
    let _: () = msg_send![configuration, setSnapshotWidth: snapshot_width];
    let state = state.clone();
    let completion = block2::RcBlock::new(
        move |image: *mut std::ffi::c_void, _: *mut std::ffi::c_void| {
            state.snapshotting.set(false);
            if state.view.get().is_null() {
                return;
            }
            if let Some(image) = bgra_image(image as id, width, height) {
                let _ = state
                    .events
                    .unbounded_send(BrowserEvent::Backdrop { bounds, image });
            }
            if state.stale.get() {
                request_backdrop(&state);
            }
        },
    );
    let _: () = msg_send![view, takeSnapshotWithConfiguration: configuration completionHandler: &*completion];
    let _: () = msg_send![configuration, release];
}

unsafe fn bgra_image(image: id, width: usize, height: usize) -> Option<Arc<RenderImage>> {
    if image.is_null() || width == 0 || height == 0 {
        return None;
    }
    let source: *mut std::ffi::c_void = msg_send![image, CGImage];
    if source.is_null() {
        return None;
    }
    let mut bytes = vec![0u8; width * height * 4];
    let space = CGColorSpaceCreateDeviceRGB();
    let context = CGBitmapContextCreate(
        bytes.as_mut_ptr().cast(),
        width,
        height,
        8,
        width * 4,
        space,
        BGRA_PREMULTIPLIED,
    );
    CGColorSpaceRelease(space);
    if context.is_null() {
        return None;
    }
    let target = CGRect {
        origin: CGPoint::default(),
        size: CGSize {
            width: width as f64,
            height: height as f64,
        },
    };
    CGContextDrawImage(context, target, source);
    CGContextRelease(context);
    let buffer = image::RgbaImage::from_raw(width as u32, height as u32, bytes)?;
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}

unsafe fn changed(this: &Object, view: id, committed: bool) {
    let url: id = msg_send![view, URL];
    let address: id = if url.is_null() {
        nil
    } else {
        msg_send![url, absoluteString]
    };
    let title: id = msg_send![view, title];
    let loading: BOOL = msg_send![view, isLoading];
    if loading != YES {
        request_backdrop(&state(this));
    }
    if loading != YES && state(this).refreshing.replace(false) {
        let spinner = *this.get_ivar::<usize>("spinner") as id;
        let _: () = msg_send![spinner, stopAnimating];
    }
    let back: BOOL = msg_send![view, canGoBack];
    let forward: BOOL = msg_send![view, canGoForward];
    let _ = state(this).events.unbounded_send(BrowserEvent::Changed {
        url: nsstring_to_string(address),
        title: nsstring_to_string(title).unwrap_or_default(),
        loading: loading == YES,
        can_go_back: back == YES,
        can_go_forward: forward == YES,
        committed,
    });
}

fn delegate_class() -> &'static Class {
    static CLASS: std::sync::OnceLock<&Class> = std::sync::OnceLock::new();
    CLASS.get_or_init(|| unsafe {
        let mut decl = ClassDecl::new("ZZBrowserDelegate", class!(NSObject)).unwrap();
        decl.add_ivar::<usize>("state");
        decl.add_ivar::<usize>("spinner");
        for name in [
            "WKNavigationDelegate",
            "WKUIDelegate",
            "UIGestureRecognizerDelegate",
            "WKScriptMessageHandler",
        ] {
            if let Some(protocol) = Protocol::get(name) {
                decl.add_protocol(protocol);
            }
        }
        decl.add_method(
            sel!(userContentController:didReceiveScriptMessage:),
            picker_message as extern "C" fn(&Object, Sel, id, id),
        );
        decl.add_method(
            sel!(observeValueForKeyPath:ofObject:change:context:),
            observe as extern "C" fn(&Object, Sel, id, id, id, *mut std::ffi::c_void),
        );
        decl.add_method(
            sel!(webView:decidePolicyForNavigationAction:decisionHandler:),
            policy as extern "C" fn(&Object, Sel, id, id, id),
        );
        decl.add_method(
            sel!(webView:didFailProvisionalNavigation:withError:),
            failed as extern "C" fn(&Object, Sel, id, id, id),
        );
        decl.add_method(
            sel!(webView:didFailNavigation:withError:),
            failed as extern "C" fn(&Object, Sel, id, id, id),
        );
        decl.add_method(
            sel!(webView:didCommitNavigation:),
            committed as extern "C" fn(&Object, Sel, id, id),
        );
        decl.add_method(
            sel!(webView:didStartProvisionalNavigation:),
            navigation_started as extern "C" fn(&Object, Sel, id, id),
        );
        decl.add_method(
            sel!(webView:didFinishNavigation:),
            committed as extern "C" fn(&Object, Sel, id, id),
        );
        decl.add_method(
            sel!(webViewWebContentProcessDidTerminate:),
            terminated as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(webView:createWebViewWithConfiguration:forNavigationAction:windowFeatures:),
            open as extern "C" fn(&Object, Sel, id, id, id, id) -> id,
        );
        decl.add_method(sel!(pageTapped:), tapped as extern "C" fn(&Object, Sel, id));
        decl.add_method(sel!(pagePanned:), panned as extern "C" fn(&Object, Sel, id));
        decl.add_method(
            sel!(gestureRecognizer:shouldRecognizeSimultaneouslyWithGestureRecognizer:),
            simultaneous as extern "C" fn(&Object, Sel, id, id) -> BOOL,
        );
        decl.register()
    })
}

extern "C" fn observe(this: &Object, _: Sel, key: id, object: id, _: id, _: *mut std::ffi::c_void) {
    unsafe {
        let offset: BOOL = msg_send![key, isEqualToString: ns_string(SCROLL_OFFSET)];
        if offset == YES {
            scrolled(this, object);
        } else {
            changed(this, object, false);
        }
    }
}

unsafe fn scrolled(this: &Object, scroll: id) {
    let state = state(this);
    request_backdrop(&state);
    let offset: CGPoint = msg_send![scroll, contentOffset];
    let last = state.offset.replace(offset.y);
    let tracking: BOOL = msg_send![scroll, isTracking];
    let decelerating: BOOL = msg_send![scroll, isDecelerating];
    if tracking != YES && decelerating != YES {
        return;
    }
    let _ = state.events.unbounded_send(BrowserEvent::Scrolled {
        travel: (offset.y - last) as f32,
        at_top: offset.y <= 0.5,
    });
}

fn host_class() -> &'static Class {
    static CLASS: std::sync::OnceLock<&Class> = std::sync::OnceLock::new();
    CLASS.get_or_init(|| unsafe {
        let mut decl = ClassDecl::new("ZZBrowserHost", class!(UIView)).unwrap();
        for name in HOLE {
            decl.add_ivar::<f64>(name);
        }
        decl.add_method(
            sel!(hitTest:withEvent:),
            host_hit_test as extern "C" fn(&Object, Sel, CGPoint, id) -> id,
        );
        decl.register()
    })
}

extern "C" fn host_hit_test(this: &Object, _: Sel, point: CGPoint, event: id) -> id {
    unsafe {
        let [x, y, width, height] = HOLE.map(|name| *this.get_ivar::<f64>(name));
        if width > 0.0
            && height > 0.0
            && point.x >= x
            && point.x <= x + width
            && point.y >= y
            && point.y <= y + height
        {
            return nil;
        }
        msg_send![super(this, class!(UIView)), hitTest: point withEvent: event]
    }
}

extern "C" fn committed(this: &Object, _: Sel, view: id, _: id) {
    unsafe {
        changed(this, view, true);
    }
}

extern "C" fn policy(this: &Object, _: Sel, _: id, action: id, handler: id) {
    unsafe {
        let request: id = msg_send![action, request];
        let url: id = msg_send![request, URL];
        let result = state(this).profile.prepare(url);
        let allow = result.is_ok();
        if let Err(error) = result {
            let _ = state(this)
                .events
                .unbounded_send(BrowserEvent::Error(error));
        }
        let block = &*(handler as *const block2::Block<dyn Fn(isize)>);
        block.call((isize::from(allow),));
    }
}

extern "C" fn failed(this: &Object, _: Sel, _: id, _: id, error: id) {
    unsafe {
        let code: isize = msg_send![error, code];
        if code == -999 {
            return;
        }
        let text: id = msg_send![error, localizedDescription];
        if let Some(error) = nsstring_to_string(text) {
            let _ = state(this)
                .events
                .unbounded_send(BrowserEvent::Error(error));
        }
    }
}

extern "C" fn terminated(this: &Object, _: Sel, _: id) {
    unsafe {
        let state = state(this);
        state.generation.set(state.generation.get().wrapping_add(1));
        state.picker.cancel();
        state.picking.set(false);
        let _ = state
            .events
            .unbounded_send(BrowserEvent::ElementPickCancelled);
        let _ = state.events.unbounded_send(BrowserEvent::Error(
            "The page stopped. Reload to continue.".into(),
        ));
    }
}

extern "C" fn open(this: &Object, _: Sel, _: id, _: id, action: id, _: id) -> id {
    unsafe {
        let request: id = msg_send![action, request];
        let url: id = msg_send![request, URL];
        if url.is_null() {
            return nil;
        }
        let address: id = msg_send![url, absoluteString];
        if let Some(url) = nsstring_to_string(address) {
            let _ = state(this).events.unbounded_send(BrowserEvent::Open(url));
        }
    }
    nil
}

extern "C" fn tapped(this: &Object, _: Sel, _: id) {
    unsafe {
        let _ = state(this).events.unbounded_send(BrowserEvent::Focused);
    }
}

extern "C" fn panned(this: &Object, _: Sel, gesture: id) {
    unsafe {
        let state = state(this);
        if state.refreshing.get() {
            return;
        }
        let phase: isize = msg_send![gesture, state];
        let scroll: id = msg_send![gesture, view];
        let offset: CGPoint = msg_send![scroll, contentOffset];
        let spinner = *this.get_ivar::<usize>("spinner") as id;
        let pulled = offset.y <= -PULL_TO_REFRESH;
        match phase {
            GESTURE_ENDED if pulled => {
                state.refreshing.set(true);
                let _: () = msg_send![spinner, startAnimating];
                let _ = state.events.unbounded_send(BrowserEvent::Refresh);
            }
            GESTURE_CHANGED if pulled => {
                let _: () = msg_send![spinner, startAnimating];
            }
            _ => {
                let _: () = msg_send![spinner, stopAnimating];
            }
        }
    }
}

extern "C" fn simultaneous(_: &Object, _: Sel, _: id, _: id) -> BOOL {
    YES
}

#[link(name = "WebKit", kind = "framework")]
unsafe extern "C" {}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGColorSpaceCreateDeviceRGB() -> *mut std::ffi::c_void;
    fn CGColorSpaceRelease(space: *mut std::ffi::c_void);
    fn CGBitmapContextCreate(
        data: *mut std::ffi::c_void,
        width: usize,
        height: usize,
        bits_per_component: usize,
        bytes_per_row: usize,
        space: *mut std::ffi::c_void,
        info: u32,
    ) -> *mut std::ffi::c_void;
    fn CGContextDrawImage(
        context: *mut std::ffi::c_void,
        rect: CGRect,
        image: *mut std::ffi::c_void,
    );
    fn CGContextRelease(context: *mut std::ffi::c_void);
}

#[link(name = "UIKit", kind = "framework")]
unsafe extern "C" {
    fn UIImagePNGRepresentation(image: id) -> id;
}

#[link(name = "Network", kind = "framework")]
unsafe extern "C" {
    fn nw_endpoint_create_host(host: *const std::ffi::c_char, port: *const std::ffi::c_char) -> id;
    fn nw_proxy_config_create_socksv5(endpoint: id) -> id;
    fn nw_proxy_config_set_failover_allowed(proxy: id, allowed: bool);
    fn nw_proxy_config_add_excluded_domain(proxy: id, domain: *const std::ffi::c_char);
}
