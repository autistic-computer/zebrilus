use std::{
    cell::{Cell, RefCell},
    collections::{BTreeSet, HashMap},
    rc::{Rc, Weak},
};

use ala::{App, Global, IntoElement, Pixels, SharedString, Styled as _, canvas, px};
use wasm_bindgen::{JsCast as _, JsValue, closure::Closure};
use web_time::{Duration, Instant};
use zz_protocol::PaneId;
use zz_ui::agent::{
    AgentHtmlFrames, FIGURE_RENDER_DEBOUNCE, HtmlFrame, visual::PREVIEW_HEIGHT_SCRIPT,
};

const MIN_HEIGHT: f64 = 8.0;
const MAX_HEIGHT: f64 = 800.0;
const KEEP_HIDDEN: Duration = Duration::from_secs(30);
const SANDBOX: &str = "allow-scripts";
const REPLY_SCRIPT_HEAD: &str = r#"<script>(()=>{const token=""#;
const REPLY_SCRIPT_TAIL: &str = r#"";Object.defineProperty(window,"zz",{value:Object.freeze({reply:(text)=>{parent.postMessage({zz:"reply",token,text:String(text)},"*");return Promise.resolve()}})})})();</script>"#;

const FRAME_SCRIPT_HEAD: &str = r#"<script>(()=>{const post=(message)=>parent.postMessage(message,"*");
let last=-1;const send=()=>{const height="#;

const FRAME_SCRIPT_TAIL: &str = r#";if(height!==last){last=height;post({zz:"height",height})}};
const start=()=>{send();const watch=new ResizeObserver(send);watch.observe(document.documentElement);if(document.body)watch.observe(document.body)};
if(document.readyState==="loading")addEventListener("DOMContentLoaded",start);else start();
addEventListener("load",send);
const room=(node,event)=>{const style=getComputedStyle(node);const root=node===document.scrollingElement;
const down=event.deltaY!==0&&(root||/auto|scroll/.test(style.overflowY))&&(event.deltaY>0?node.scrollTop+node.clientHeight<node.scrollHeight-1:node.scrollTop>0);
const across=event.deltaX!==0&&(root||/auto|scroll/.test(style.overflowX))&&(event.deltaX>0?node.scrollLeft+node.clientWidth<node.scrollWidth-1:node.scrollLeft>0);
return down||across};
addEventListener("wheel",(event)=>{if(event.defaultPrevented||event.ctrlKey)return;
for(let node=event.target;node instanceof Element;node=node.parentElement){if(room(node,event))return}
const root=document.scrollingElement;if(root&&room(root,event))return;
post({zz:"wheel",x:event.clientX,y:event.clientY,dx:event.deltaX,dy:event.deltaY,mode:event.deltaMode})},{passive:true});
})();</script>"#;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Placement {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    scale: f64,
    clip: [f64; 4],
    live: bool,
    dark: bool,
}

struct Frame {
    element: web_sys::HtmlIFrameElement,
    page: SharedString,
    key: u64,
    owner: Option<u64>,
    token: String,
    height: Option<f64>,
    pending: Option<(SharedString, Instant)>,
    placement: Option<Placement>,
    drawn: u64,
    hidden: Option<Instant>,
}

impl Frame {
    fn hide(&mut self) {
        if self.placement.take().is_some() {
            self.element.style().set_property("display", "none").ok();
        }
    }
}

struct Frames {
    document: web_sys::Document,
    layer: web_sys::HtmlElement,
    frames: RefCell<HashMap<u64, Frame>>,
    heights: RefCell<HashMap<u64, f64>>,
    origin: Cell<Option<(f64, f64)>>,
    draw: Cell<u64>,
    covered: Cell<bool>,
    shown: RefCell<BTreeSet<u64>>,
    wake: async_channel::Sender<Duration>,
    replies: async_channel::Sender<(PaneId, String)>,
}

struct WebFrames(Rc<Frames>);

impl Global for WebFrames {}

pub fn show_html_in_chats(cx: &mut App) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let Some(layer) = document
        .create_element("div")
        .ok()
        .and_then(|layer| layer.dyn_into::<web_sys::HtmlElement>().ok())
    else {
        return;
    };
    let style = layer.style();
    for (property, value) in [
        ("position", "fixed"),
        ("inset", "0"),
        ("overflow", "hidden"),
        ("pointer-events", "none"),
    ] {
        style.set_property(property, value).ok();
    }
    layer.set_attribute("data-zz-frames", "").ok();
    if let Some(body) = document.body() {
        body.append_child(&layer).ok();
    }
    let (wake, wakes) = async_channel::unbounded();
    let (replies, sent) = async_channel::unbounded::<(PaneId, String)>();
    let frames = Rc::new(Frames {
        document,
        layer,
        frames: RefCell::default(),
        heights: RefCell::default(),
        origin: Cell::new(None),
        draw: Cell::new(0),
        covered: Cell::new(false),
        shown: RefCell::default(),
        wake,
        replies,
    });
    listen(Rc::downgrade(&frames));
    cx.spawn(async move |cx| {
        while let Ok(mut delay) = wakes.recv().await {
            while let Ok(next) = wakes.try_recv() {
                delay = delay.max(next);
            }
            if !delay.is_zero() {
                cx.background_executor().timer(delay).await;
            }
            cx.update(App::refresh_windows);
        }
    })
    .detach();
    cx.spawn(async move |cx| {
        while let Ok((pane, text)) = sent.recv().await {
            cx.update(|cx| crate::app::draft_page_reply(pane, &text, cx));
        }
    })
    .detach();
    cx.set_global(AgentHtmlFrames {
        height: Rc::new({
            let frames = Rc::clone(&frames);
            move |block, key| {
                let height = frames.heights.borrow().get(&key).copied();
                height
                    .or_else(|| {
                        frames
                            .frames
                            .borrow()
                            .get(&block)
                            .and_then(|frame| frame.height)
                    })
                    .map(|height| px(height as f32))
            }
        }),
        place: Rc::new({
            let frames = Rc::clone(&frames);
            move |frame| frames.place(&frame)
        }),
    });
    cx.set_global(WebFrames(frames));
}

pub fn drawn(covered: bool, shown: &BTreeSet<PaneId>, cx: &App) -> impl IntoElement + use<> {
    if let Some(frames) = cx.try_global::<WebFrames>() {
        frames.0.covered.set(covered);
        *frames.0.shown.borrow_mut() = shown.iter().map(|pane| pane.0).collect();
    }
    canvas(
        |_, _, _| {},
        |_, (), _, cx| {
            if let Some(frames) = cx.try_global::<WebFrames>() {
                frames.0.sweep();
            }
        },
    )
    .absolute()
    .size_0()
}

fn listen(frames: Weak<Frames>) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let listener =
        Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
            if let Some(frames) = frames.upgrade() {
                frames.message(&event);
            }
        });
    window
        .add_event_listener_with_callback("message", listener.as_ref().unchecked_ref())
        .ok();
    listener.forget();
}

fn with_reply(page: &str, token: &str) -> String {
    let lower = page.to_ascii_lowercase();
    let after = |tag: &str| {
        lower.match_indices(tag).find_map(|(start, _)| {
            let rest = &lower[start + tag.len()..];
            if !rest.starts_with(|next: char| next == '>' || next.is_ascii_whitespace()) {
                return None;
            }
            rest.find('>').map(|close| start + tag.len() + close + 1)
        })
    };
    let at = after("<head").or_else(|| after("<!doctype")).unwrap_or(0);
    format!(
        "{}{REPLY_SCRIPT_HEAD}{token}{REPLY_SCRIPT_TAIL}{}",
        &page[..at],
        &page[at..]
    )
}

fn number(data: &JsValue, name: &str) -> Option<f64> {
    js_sys::Reflect::get(data, &JsValue::from_str(name))
        .ok()?
        .as_f64()
        .filter(|value| value.is_finite())
}

impl Frames {
    fn origin(&self) -> (f64, f64) {
        if let Some(origin) = self.origin.get() {
            return origin;
        }
        let origin = self
            .document
            .query_selector("canvas[data-ala-canvas]")
            .ok()
            .flatten()
            .map_or((0.0, 0.0), |canvas| {
                let rect = canvas.get_bounding_client_rect();
                (rect.left(), rect.top())
            });
        self.origin.set(Some(origin));
        origin
    }

    fn place(&self, frame: &HtmlFrame) {
        let draw = self.draw.get();
        let now = Instant::now();
        let mut frames = self.frames.borrow_mut();
        let entry = match frames.entry(frame.block) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let Some(element) = self.frame_element() else {
                    return;
                };
                entry.insert(Frame {
                    element,
                    page: SharedString::default(),
                    key: frame.key,
                    owner: frame.owner,
                    token: String::new(),
                    height: None,
                    pending: None,
                    placement: None,
                    drawn: draw,
                    hidden: None,
                })
            }
        };
        entry.drawn = draw;
        entry.hidden = None;
        entry.owner = frame.owner;
        if entry.page == frame.page {
            entry.pending = None;
        } else {
            let since = match &entry.pending {
                Some((page, since)) if *page == frame.page => *since,
                _ => {
                    entry.pending = Some((frame.page.clone(), now));
                    now
                }
            };
            if entry.page.is_empty() || now.duration_since(since) >= FIGURE_RENDER_DEBOUNCE {
                if entry.key != frame.key {
                    self.heights.borrow_mut().remove(&entry.key);
                }
                entry.key = frame.key;
                entry.page = frame.page.clone();
                entry.pending = None;
                entry.token = format!(
                    "{:x}{:x}",
                    js_sys::Math::random().to_bits(),
                    js_sys::Math::random().to_bits()
                );
                let page = if frame.owner.is_some() {
                    with_reply(&frame.page, &entry.token)
                } else {
                    frame.page.to_string()
                };
                entry.element.set_srcdoc(&format!(
                    "{page}{FRAME_SCRIPT_HEAD}{PREVIEW_HEIGHT_SCRIPT}{FRAME_SCRIPT_TAIL}"
                ));
            } else if since == now {
                self.wake.try_send(FIGURE_RENDER_DEBOUNCE).ok();
            }
        }
        let Some(placement) = self.placement(frame) else {
            entry.hide();
            return;
        };
        if entry.placement == Some(placement) {
            return;
        }
        let style = entry.element.style();
        let [top, right, bottom, left] = placement.clip;
        for (property, value) in [
            ("display", "block".to_owned()),
            ("width", format!("{}px", placement.width)),
            ("height", format!("{}px", placement.height)),
            (
                "transform",
                format!(
                    "translate({}px,{}px) scale({})",
                    placement.x, placement.y, placement.scale
                ),
            ),
            (
                "clip-path",
                format!("inset({top}px {right}px {bottom}px {left}px)"),
            ),
            (
                "pointer-events",
                if placement.live { "auto" } else { "none" }.to_owned(),
            ),
            (
                "color-scheme",
                if placement.dark { "dark" } else { "light" }.to_owned(),
            ),
        ] {
            style.set_property(property, &value).ok();
        }
        entry.placement = Some(placement);
    }

    fn placement(&self, frame: &HtmlFrame) -> Option<Placement> {
        let bounds = frame.bounds;
        let clip = bounds.intersect(&frame.clip);
        if self.covered.get()
            || frame
                .owner
                .is_some_and(|owner| !self.shown.borrow().contains(&owner))
            || clip.size.width <= px(0.0)
            || clip.size.height <= px(0.0)
        {
            return None;
        }
        let edge = |pixels: Pixels| f64::from(f32::from(pixels));
        let zoom = f64::from(frame.zoom);
        let (width, height) = frame.layout.map_or(
            (
                edge(bounds.size.width) / zoom,
                edge(bounds.size.height) / zoom,
            ),
            |layout| (edge(layout.width), edge(layout.height)),
        );
        let scale = if width > 0.0 {
            edge(bounds.size.width) / width
        } else {
            1.0
        };
        let (left, top) = self.origin();
        let inset = |outside: Pixels| (edge(outside).max(0.0) / scale).max(0.0);
        Some(Placement {
            x: left + edge(bounds.origin.x),
            y: top + edge(bounds.origin.y),
            width,
            height,
            scale,
            clip: [
                inset(clip.top() - bounds.top()),
                inset(bounds.right() - clip.right()),
                inset(bounds.bottom() - clip.bottom()),
                inset(clip.left() - bounds.left()),
            ],
            live: frame.live,
            dark: frame.dark,
        })
    }

    fn frame_element(&self) -> Option<web_sys::HtmlIFrameElement> {
        let element: web_sys::HtmlIFrameElement = self
            .document
            .create_element("iframe")
            .ok()?
            .dyn_into()
            .ok()?;
        element.set_attribute("sandbox", SANDBOX).ok()?;
        element.set_attribute("referrerpolicy", "no-referrer").ok();
        element.set_attribute("title", "HTML preview").ok();
        let style = element.style();
        for (property, value) in [
            ("position", "absolute"),
            ("left", "0"),
            ("top", "0"),
            ("border", "0"),
            ("margin", "0"),
            ("background", "transparent"),
            ("transform-origin", "0 0"),
            ("display", "none"),
        ] {
            style.set_property(property, value).ok();
        }
        self.layer.append_child(&element).ok()?;
        Some(element)
    }

    fn sweep(&self) {
        let draw = self.draw.get();
        let now = Instant::now();
        self.frames.borrow_mut().retain(|_, frame| {
            if frame.drawn == draw {
                return true;
            }
            frame.hide();
            let hidden = *frame.hidden.get_or_insert(now);
            if now.duration_since(hidden) < KEEP_HIDDEN {
                return true;
            }
            frame.element.remove();
            false
        });
        self.draw.set(draw.wrapping_add(1));
        self.origin.set(None);
    }

    fn message(&self, event: &web_sys::MessageEvent) {
        let Some(source) = event.source() else {
            return;
        };
        let data = event.data();
        let Some(kind) = js_sys::Reflect::get(&data, &JsValue::from_str("zz"))
            .ok()
            .and_then(|kind| kind.as_string())
        else {
            return;
        };
        let mut frames = self.frames.borrow_mut();
        let Some(frame) = frames.values_mut().find(|frame| {
            frame
                .element
                .content_window()
                .is_some_and(|window| js_sys::Object::is(&window, &source))
        }) else {
            return;
        };
        match kind.as_str() {
            "height" => {
                let Some(height) = number(&data, "height") else {
                    return;
                };
                let height = height.clamp(MIN_HEIGHT, MAX_HEIGHT);
                frame.height = Some(height);
                let previous = self.heights.borrow_mut().insert(frame.key, height);
                if previous != Some(height) {
                    self.wake.try_send(Duration::ZERO).ok();
                }
            }
            "reply" => {
                let field = |name: &str| {
                    js_sys::Reflect::get(&data, &JsValue::from_str(name))
                        .ok()
                        .and_then(|value| value.as_string())
                };
                let text = field("text");
                let ours = field("token").is_some_and(|token| token == frame.token);
                let used = self
                    .document
                    .active_element()
                    .is_some_and(|active| js_sys::Object::is(&active, &frame.element));
                if let (Some(text), Some(owner), true, true, true) = (
                    text,
                    frame.owner,
                    ours,
                    used,
                    frame.placement.is_some_and(|placement| placement.live),
                ) {
                    self.replies.try_send((PaneId(owner), text)).ok();
                }
            }
            "wheel" => {
                let placement = frame.placement.filter(|placement| placement.live);
                drop(frames);
                if let Some(placement) = placement {
                    self.wheel(&data, placement);
                }
            }
            _ => {}
        }
    }

    fn wheel(&self, data: &JsValue, placement: Placement) {
        let (Some(x), Some(y), Some(dx), Some(dy), Some(mode)) = (
            number(data, "x"),
            number(data, "y"),
            number(data, "dx"),
            number(data, "dy"),
            number(data, "mode"),
        ) else {
            return;
        };
        let Some(canvas) = self
            .document
            .query_selector("canvas[data-ala-canvas]")
            .ok()
            .flatten()
        else {
            return;
        };
        let init = web_sys::WheelEventInit::new();
        init.set_bubbles(true);
        init.set_cancelable(true);
        init.set_client_x((placement.x + x * placement.scale).round() as i32);
        init.set_client_y((placement.y + y * placement.scale).round() as i32);
        init.set_delta_x(dx);
        init.set_delta_y(dy);
        init.set_delta_mode(mode.clamp(0.0, 2.0) as u32);
        if let Ok(event) = web_sys::WheelEvent::new_with_event_init_dict("wheel", &init) {
            canvas.dispatch_event(&event).ok();
        }
    }
}
