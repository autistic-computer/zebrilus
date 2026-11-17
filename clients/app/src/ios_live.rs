#![allow(unsafe_code, unsafe_op_in_unsafe_fn, unexpected_cfgs)]

use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    rc::Rc,
};

use ala::{
    App, Bounds, DispatchPhase, Global, IntoElement, MouseDownEvent, Pixels, Point, Size,
    Styled as _, Window, canvas,
};
use ala_platform::ios::{CGPoint, CGRect, CGSize, id, nil, ns_string, nsstring_to_string};
use futures::{
    StreamExt as _,
    channel::mpsc::{UnboundedSender, unbounded},
};
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{BOOL, Class, NO, Object, Protocol, Sel, YES},
    sel, sel_impl,
};
use raw_window_handle::RawWindowHandle;
use zz_protocol::PaneId;
use zz_ui::agent::{AgentHtmlLive, HtmlLiveBlock, html_overlay_clip};

use crate::{
    ios_browser::{REPLY_HANDLER, add_reply},
    ios_preview::clear_web_view,
};

const PAGE_SCHEME: &str = "about";

enum Event {
    Ready(u64),
    Reply(u64, String),
}

thread_local! {
    static EVENTS: RefCell<Option<UnboundedSender<Event>>> = const { RefCell::new(None) };
}

struct Page {
    block: u64,
    owner: Option<u64>,
    generation: u64,
    view: id,
    host: id,
    pan: id,
    delegate: id,
    ready: bool,
    shown: bool,
    click: Option<Point<Pixels>>,
    zoom: f32,
    drawn: u64,
    placed: Option<(Bounds<Pixels>, Bounds<Pixels>, f64)>,
}

impl Page {
    fn hide(&mut self) {
        if self.shown {
            self.shown = false;
            unsafe {
                let _: BOOL = msg_send![self.view, endEditing: YES];
                let _: () = msg_send![self.host, setHidden: YES];
            }
        }
    }

    fn press(&mut self) {
        let Some(click) = self.click.take() else {
            return;
        };
        let zoom = f64::from(self.zoom.max(0.1));
        let x = f64::from(f32::from(click.x)) / zoom;
        let y = f64::from(f32::from(click.y)) / zoom;
        let script = format!(
            "(() => {{ const target = document.elementFromPoint({x}, {y}); if (!target) return; const init = {{ bubbles: true, cancelable: true, view: window, clientX: {x}, clientY: {y} }}; for (const kind of ['mousedown', 'mouseup', 'click']) target.dispatchEvent(new MouseEvent(kind, init)); if (typeof target.focus === 'function') target.focus(); }})()"
        );
        unsafe {
            let _: () =
                msg_send![self.view, evaluateJavaScript: ns_string(&script) completionHandler: nil];
        }
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![self.view, stopLoading];
            let _: () = msg_send![self.view, setNavigationDelegate: nil];
            let configuration: id = msg_send![self.view, configuration];
            let controller: id = msg_send![configuration, userContentController];
            let _: () =
                msg_send![controller, removeScriptMessageHandlerForName: ns_string(REPLY_HANDLER)];
            let _: () = msg_send![self.view, removeFromSuperview];
            let _: () = msg_send![self.host, removeFromSuperview];
            let _: () = msg_send![self.view, release];
            let _: () = msg_send![self.host, release];
            let _: () = msg_send![self.pan, release];
            let _: () = msg_send![self.delegate, release];
        }
    }
}

#[derive(Default)]
struct Live {
    page: RefCell<Option<Page>>,
    generation: Cell<u64>,
    draw: Cell<u64>,
    covered: Cell<bool>,
    shown: RefCell<BTreeSet<u64>>,
}

struct IosLive(Rc<Live>);

impl Global for IosLive {}

pub(crate) fn install(cx: &mut App) {
    let (events, mut received) = unbounded();
    EVENTS.with_borrow_mut(|sender| *sender = Some(events));
    let live = Rc::new(Live::default());
    cx.spawn({
        let live = Rc::clone(&live);
        async move |cx| {
            while let Some(event) = received.next().await {
                match event {
                    Event::Ready(generation) => {
                        let readied = live
                            .page
                            .borrow_mut()
                            .as_mut()
                            .filter(|page| page.generation == generation && !page.ready)
                            .map(|page| {
                                page.ready = true;
                                page.press();
                            })
                            .is_some();
                        if readied {
                            cx.update(App::refresh_windows);
                        }
                    }
                    Event::Reply(generation, text) => {
                        let owner = live
                            .page
                            .borrow()
                            .as_ref()
                            .filter(|page| page.generation == generation && page.ready)
                            .and_then(|page| page.owner);
                        if let Some(owner) = owner {
                            cx.update(|cx| crate::app::draft_page_reply(PaneId(owner), &text, cx));
                        }
                    }
                }
            }
        }
    })
    .detach();
    cx.set_global(AgentHtmlLive {
        live: Rc::new({
            let live = Rc::clone(&live);
            move |block, _| {
                live.page
                    .borrow()
                    .as_ref()
                    .is_some_and(|page| page.block == block && page.ready)
                    .then(|| {
                        canvas(
                            |_, _, _| {},
                            move |bounds, (), window, cx| {
                                let (_, clip) = html_overlay_clip(window, cx);
                                let shown = bounds.intersect(&clip);
                                if let Some(live) = cx.try_global::<IosLive>() {
                                    live.0.place(block, bounds, clip, window);
                                }
                                window.on_mouse_event(
                                    move |event: &MouseDownEvent, phase, window, cx| {
                                        if phase == DispatchPhase::Capture
                                            && !shown.contains(&event.position)
                                            && let Some(live) = cx.try_global::<IosLive>()
                                            && live.0.page.borrow_mut().take().is_some()
                                        {
                                            window.refresh();
                                        }
                                    },
                                );
                            },
                        )
                        .size_full()
                        .into_any_element()
                    })
            }
        }),
        activate: Rc::new({
            let live = Rc::clone(&live);
            move |block, window, cx| {
                live.activate(&block, window);
                cx.refresh_windows();
            }
        }),
        overlay: true,
    });
    cx.set_global(IosLive(live));
}

pub(crate) fn drawn(covered: bool, shown: &BTreeSet<PaneId>, cx: &App) -> impl IntoElement + use<> {
    if let Some(live) = cx.try_global::<IosLive>() {
        live.0.covered.set(covered);
        *live.0.shown.borrow_mut() = shown.iter().map(|pane| pane.0).collect();
    }
    canvas(
        |_, _, _| {},
        |_, (), _, cx| {
            if let Some(live) = cx.try_global::<IosLive>() {
                live.0.sweep();
            }
        },
    )
    .absolute()
    .size_0()
}

fn parent_view(window: &Window) -> Option<id> {
    let handle = raw_window_handle::HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::UiKit(handle) = handle.as_raw() else {
        return None;
    };
    Some(handle.ui_view.as_ptr() as id)
}

fn points_per_pixel(parent: id, window: &Window) -> f64 {
    let native: f64 = unsafe { msg_send![parent, contentScaleFactor] };
    f64::from(window.scale_factor()) / native.max(1.0)
}

fn rect(bounds: Bounds<Pixels>, scale: f64) -> CGRect {
    let points = |value: Pixels| f64::from(f32::from(value)) * scale;
    CGRect {
        origin: CGPoint {
            x: points(bounds.origin.x),
            y: points(bounds.origin.y),
        },
        size: CGSize {
            width: points(bounds.size.width),
            height: points(bounds.size.height),
        },
    }
}

impl Live {
    fn activate(&self, block: &HtmlLiveBlock, window: &Window) {
        self.page.replace(None);
        let Some(parent) = parent_view(window) else {
            return;
        };
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        let scale = points_per_pixel(parent, window);
        let size: Size<Pixels> = block.size;
        let frame = rect(Bounds::new(Point::default(), size), scale);
        unsafe {
            let configuration: id = msg_send![class!(WKWebViewConfiguration), new];
            let store: id = msg_send![class!(WKWebsiteDataStore), nonPersistentDataStore];
            let _: () = msg_send![configuration, setWebsiteDataStore: store];
            let _: () = msg_send![configuration, setAllowsInlineMediaPlayback: YES];
            let delegate: id = msg_send![delegate_class(), new];
            (*delegate).set_ivar("generation", generation as usize);
            let controller: id = msg_send![configuration, userContentController];
            add_reply(controller, delegate, &format!("{PAGE_SCHEME}:"));
            let view: id = msg_send![class!(WKWebView), alloc];
            let view: id = msg_send![view, initWithFrame: frame configuration: configuration];
            let _: () = msg_send![configuration, release];
            clear_web_view(view);
            let scroll: id = msg_send![view, scrollView];
            let _: () = msg_send![scroll, setBounces: NO];
            let _: () = msg_send![scroll, setScrollEnabled: NO];
            let _: () = msg_send![view, setNavigationDelegate: delegate];
            let _: () = msg_send![view, setPageZoom: f64::from(block.zoom) * scale];
            let host: id = msg_send![class!(UIView), new];
            let _: () = msg_send![host, setClipsToBounds: YES];
            let _: () = msg_send![host, setHidden: YES];
            let _: () = msg_send![host, setFrame: frame];
            let _: () = msg_send![host, addSubview: view];
            let pan: id = msg_send![class!(UIPanGestureRecognizer), alloc];
            let pan: id = msg_send![pan, initWithTarget: parent action: sel!(alaScroll:)];
            let _: () = msg_send![pan, setDelegate: delegate];
            let _: () = msg_send![host, addGestureRecognizer: pan];
            let _: () = msg_send![parent, addSubview: host];
            let _: id = msg_send![view, loadHTMLString: ns_string(&block.html) baseURL: nil];
            self.page.replace(Some(Page {
                block: block.block,
                owner: block.owner,
                generation,
                view,
                host,
                pan,
                delegate,
                ready: false,
                shown: false,
                click: block.click,
                zoom: block.zoom,
                drawn: self.draw.get(),
                placed: None,
            }));
        }
    }

    fn place(&self, block: u64, bounds: Bounds<Pixels>, clip: Bounds<Pixels>, window: &Window) {
        let mut page = self.page.borrow_mut();
        let Some(page) = page.as_mut().filter(|page| page.block == block) else {
            return;
        };
        page.drawn = self.draw.get();
        let clip = bounds.intersect(&clip);
        let visible = !self.covered.get()
            && page
                .owner
                .is_none_or(|owner| self.shown.borrow().contains(&owner))
            && clip.size.width > Pixels::ZERO
            && clip.size.height > Pixels::ZERO;
        let Some(parent) = parent_view(window).filter(|_| visible) else {
            page.hide();
            return;
        };
        let scale = points_per_pixel(parent, window);
        let host = rect(clip, scale);
        let inner = rect(Bounds::new(bounds.origin - clip.origin, bounds.size), scale);
        unsafe {
            let previous: id = msg_send![page.host, superview];
            if previous != parent {
                let _: () = msg_send![page.pan, removeTarget: nil action: sel!(alaScroll:)];
                let _: () = msg_send![page.pan, addTarget: parent action: sel!(alaScroll:)];
                let _: () = msg_send![parent, addSubview: page.host];
                page.placed = None;
            }
            if page.placed != Some((bounds, clip, scale)) {
                let _: () = msg_send![class!(CATransaction), begin];
                let _: () = msg_send![class!(CATransaction), setDisableActions: YES];
                let _: () = msg_send![page.host, setFrame: host];
                let _: () = msg_send![page.view, setFrame: inner];
                let _: () = msg_send![class!(CATransaction), commit];
                page.placed = Some((bounds, clip, scale));
            }
            if !page.shown {
                page.shown = true;
                let _: () = msg_send![page.host, setHidden: NO];
            }
        }
    }

    fn sweep(&self) {
        let draw = self.draw.get();
        let stale = self
            .page
            .borrow()
            .as_ref()
            .is_some_and(|page| page.ready && page.drawn != draw);
        if stale {
            self.page.replace(None);
        }
        self.draw.set(draw.wrapping_add(1));
    }
}

fn delegate_class() -> &'static Class {
    static CLASS: std::sync::OnceLock<&Class> = std::sync::OnceLock::new();
    CLASS.get_or_init(|| unsafe {
        let mut decl = ClassDecl::new("ZZLiveVisualDelegate", class!(NSObject)).unwrap();
        decl.add_ivar::<usize>("generation");
        for name in [
            "WKNavigationDelegate",
            "WKScriptMessageHandler",
            "UIGestureRecognizerDelegate",
        ] {
            if let Some(protocol) = Protocol::get(name) {
                decl.add_protocol(protocol);
            }
        }
        decl.add_method(
            sel!(userContentController:didReceiveScriptMessage:),
            replied as extern "C" fn(&Object, Sel, id, id),
        );
        decl.add_method(
            sel!(webView:didFinishNavigation:),
            finished as extern "C" fn(&Object, Sel, id, id),
        );
        decl.add_method(
            sel!(gestureRecognizerShouldBegin:),
            vertical as extern "C" fn(&Object, Sel, id) -> BOOL,
        );
        decl.register()
    })
}

fn send(event: Event) {
    EVENTS.with_borrow(|events| {
        if let Some(events) = events {
            let _ = events.unbounded_send(event);
        }
    });
}

unsafe fn generation(this: &Object) -> u64 {
    *this.get_ivar::<usize>("generation") as u64
}

extern "C" fn finished(this: &Object, _: Sel, _: id, _: id) {
    unsafe {
        send(Event::Ready(generation(this)));
    }
}

extern "C" fn vertical(_: &Object, _: Sel, recognizer: id) -> BOOL {
    unsafe {
        let view: id = msg_send![recognizer, view];
        let velocity: CGPoint = msg_send![recognizer, velocityInView: view];
        if velocity.y.abs() > velocity.x.abs() {
            YES
        } else {
            NO
        }
    }
}

extern "C" fn replied(this: &Object, _: Sel, _: id, message: id) {
    unsafe {
        let frame: id = msg_send![message, frameInfo];
        let main: BOOL = msg_send![frame, isMainFrame];
        let request: id = msg_send![frame, request];
        let url: id = msg_send![request, URL];
        let scheme: id = msg_send![url, scheme];
        let body: id = msg_send![message, body];
        let is_string: BOOL = msg_send![body, isKindOfClass: class!(NSString)];
        if main != YES
            || is_string != YES
            || nsstring_to_string(scheme).as_deref() != Some(PAGE_SCHEME)
        {
            return;
        }
        if let Some(text) = nsstring_to_string(body) {
            send(Event::Reply(generation(this), text));
        }
    }
}
