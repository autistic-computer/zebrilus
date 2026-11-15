#![allow(unsafe_code, unsafe_op_in_unsafe_fn, unexpected_cfgs)]

use std::{
    cell::Cell,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use ala::{App, AsyncApp, RenderImage, SharedString};
use ala_platform::ios::{CGPoint, CGRect, CGSize, id, ns_string};
use futures::{
    StreamExt as _,
    channel::{mpsc, oneshot},
    future::{Either, select},
};
use objc::{
    class, msg_send,
    runtime::{BOOL, NO, YES},
    sel, sel_impl,
};
use zz_ui::agent::{
    AgentHtmlPreviewer, HtmlPreviewSize,
    visual::{PREVIEW_HEIGHT_SCRIPT, straighten_alpha},
};

use crate::ios_browser::bgra_pixels;

const LOAD_HEIGHT: f64 = 801.0;
const MIN_HEIGHT: f64 = 8.0;
const MAX_HEIGHT: f64 = 800.0;
const POLL: Duration = Duration::from_millis(50);
const QUIET: Duration = Duration::from_millis(150);
const TIMEOUT: Duration = Duration::from_secs(3);
const FIT_CAP: Duration = Duration::from_secs(5);
const IDLE: Duration = Duration::from_mins(1);
const LOADED_SCRIPT: &str = "(!window.__zzOld && document.readyState === 'complete') ? 1 : 0";
const RETIRE_SCRIPT: &str = "window.__zzOld = 1; 0";

type Picture = Result<Arc<RenderImage>, SharedString>;

struct Job {
    html: String,
    size: HtmlPreviewSize,
    done: oneshot::Sender<Picture>,
}

pub fn show_html_in_chats(cx: &mut App) {
    let (jobs, queue) = mpsc::unbounded::<Job>();
    cx.spawn(async move |cx| work(queue, cx).await).detach();
    cx.set_global(AgentHtmlPreviewer(Rc::new(move |html, size, cx| {
        let (done, result) = oneshot::channel();
        let _ = jobs.unbounded_send(Job { html, size, done });
        cx.background_executor().spawn(async move {
            result
                .await
                .unwrap_or_else(|_| Err("the preview was dropped".into()))
        })
    })));
}

async fn work(mut queue: mpsc::UnboundedReceiver<Job>, cx: &mut AsyncApp) {
    let mut page: Option<Page> = None;
    loop {
        let job = if page.is_some() {
            let idle = cx.background_executor().timer(IDLE);
            match select(queue.next(), idle).await {
                Either::Left((job, _)) => job,
                Either::Right(_) => {
                    page = None;
                    continue;
                }
            }
        } else {
            queue.next().await
        };
        let Some(job) = job else {
            break;
        };
        if job.done.is_canceled() {
            continue;
        }
        if page.is_none() {
            page = Page::new();
        }
        let picture = match &page {
            Some(page) => render(page, &job, cx).await,
            None => Err("the app has no window yet".into()),
        };
        let _ = job.done.send(picture);
    }
}

async fn render(page: &Page, job: &Job, cx: &AsyncApp) -> Picture {
    let pause = |time| cx.background_executor().timer(time);
    let width = f64::from(f32::from(job.size.width)).max(1.0);
    let fixed = job
        .size
        .height
        .map(|height| f64::from(f32::from(height)).max(1.0));
    let _ = page.number(RETIRE_SCRIPT).await;
    page.resize(width, fixed.map_or(LOAD_HEIGHT, |height| height + 1.0));
    page.load(&job.html);
    let started = Instant::now();
    loop {
        pause(POLL).await;
        if page.number(LOADED_SCRIPT).await.ok().flatten() == Some(1.0) {
            break;
        }
        if started.elapsed() >= TIMEOUT {
            return Err("Preview timed out".into());
        }
    }
    let measure = || async {
        page.number(PREVIEW_HEIGHT_SCRIPT)
            .await
            .ok()
            .flatten()
            .map(|height| height.clamp(MIN_HEIGHT, MAX_HEIGHT).ceil())
    };
    let mut height = match fixed {
        Some(height) => height,
        None => measure()
            .await
            .ok_or_else(|| SharedString::from("the page could not be measured"))?,
    };
    page.resize(width, height);
    let painting = Instant::now();
    loop {
        pause(QUIET).await;
        if fixed.is_some() || painting.elapsed() >= FIT_CAP {
            break;
        }
        match measure().await {
            Some(measured) if (measured - height).abs() >= 1.0 => {
                height = measured;
                page.resize(width, height);
            }
            _ => break,
        }
    }
    page.snapshot(width, height, f64::from(job.size.scale))
        .await
        .ok()
        .flatten()
        .ok_or_else(|| SharedString::from("the preview never painted"))
}

struct Page {
    view: id,
}

impl Page {
    fn new() -> Option<Self> {
        unsafe {
            let window = app_window()?;
            let configuration: id = msg_send![class!(WKWebViewConfiguration), new];
            let store: id = msg_send![class!(WKWebsiteDataStore), nonPersistentDataStore];
            let _: () = msg_send![configuration, setWebsiteDataStore: store];
            let view: id = msg_send![class!(WKWebView), alloc];
            let view: id =
                msg_send![view, initWithFrame: CGRect::default() configuration: configuration];
            let _: () = msg_send![configuration, release];
            let clear: id = msg_send![class!(UIColor), clearColor];
            let _: () = msg_send![view, setOpaque: NO];
            let _: () = msg_send![view, setBackgroundColor: clear];
            let scroll: id = msg_send![view, scrollView];
            let _: () = msg_send![scroll, setBackgroundColor: clear];
            let _: () = msg_send![scroll, setContentInsetAdjustmentBehavior: 2isize];
            for edge in [
                "topEdgeEffect",
                "bottomEdgeEffect",
                "leftEdgeEffect",
                "rightEdgeEffect",
            ] {
                let has: BOOL =
                    msg_send![scroll, respondsToSelector: objc::runtime::Sel::register(edge)];
                if has == YES {
                    let effect: id = msg_send![scroll, valueForKey: ns_string(edge)];
                    if !effect.is_null() {
                        let _: () = msg_send![effect, setHidden: YES];
                    }
                }
            }
            let _: () = msg_send![view, setUserInteractionEnabled: NO];
            let _: () = msg_send![window, insertSubview: view atIndex: 0isize];
            Some(Self { view })
        }
    }

    fn resize(&self, width: f64, height: f64) {
        let frame = CGRect {
            origin: CGPoint::default(),
            size: CGSize { width, height },
        };
        unsafe {
            let _: () = msg_send![self.view, setFrame: frame];
        }
    }

    fn load(&self, html: &str) {
        unsafe {
            let _: id = msg_send![self.view, loadHTMLString: ns_string(html) baseURL: std::ptr::null_mut::<objc::runtime::Object>()];
        }
    }

    fn number(&self, script: &str) -> oneshot::Receiver<Option<f64>> {
        let (answer, answered) = oneshot::channel();
        let answer = Cell::new(Some(answer));
        let completion = block2::RcBlock::new(
            move |result: *mut std::ffi::c_void, _: *mut std::ffi::c_void| {
                let result = result as id;
                let value = unsafe {
                    let numeric: BOOL = if result.is_null() {
                        NO
                    } else {
                        msg_send![result, respondsToSelector: sel!(doubleValue)]
                    };
                    (numeric == YES).then(|| {
                        let value: f64 = msg_send![result, doubleValue];
                        value
                    })
                };
                if let Some(answer) = answer.take() {
                    let _ = answer.send(value);
                }
            },
        );
        unsafe {
            let _: () = msg_send![self.view, evaluateJavaScript: ns_string(script) completionHandler: &*completion];
        }
        answered
    }

    fn snapshot(
        &self,
        width: f64,
        height: f64,
        scale: f64,
    ) -> oneshot::Receiver<Option<Arc<RenderImage>>> {
        let (answer, answered) = oneshot::channel();
        let answer = Cell::new(Some(answer));
        let wide = (width * scale).round().max(1.0) as usize;
        let high = (height * scale).round().max(1.0) as usize;
        let completion = block2::RcBlock::new(
            move |image: *mut std::ffi::c_void, _: *mut std::ffi::c_void| {
                let picture =
                    unsafe { bgra_pixels(image as id, wide, high) }.and_then(|mut pixels| {
                        straighten_alpha(&mut pixels);
                        let buffer = image::RgbaImage::from_raw(wide as u32, high as u32, pixels)?;
                        Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
                    });
                if let Some(answer) = answer.take() {
                    let _ = answer.send(picture);
                }
            },
        );
        unsafe {
            let screen: id = msg_send![class!(UIScreen), mainScreen];
            let native: f64 = msg_send![screen, scale];
            let configuration: id = msg_send![class!(WKSnapshotConfiguration), new];
            let _: () = msg_send![configuration, setRect: CGRect {
                origin: CGPoint::default(),
                size: CGSize { width, height },
            }];
            let _: () = msg_send![configuration, setAfterScreenUpdates: YES];
            let points: id =
                msg_send![class!(NSNumber), numberWithDouble: width * scale / native.max(1.0)];
            let _: () = msg_send![configuration, setSnapshotWidth: points];
            let _: () = msg_send![self.view, takeSnapshotWithConfiguration: configuration completionHandler: &*completion];
            let _: () = msg_send![configuration, release];
        }
        answered
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![self.view, stopLoading];
            let _: () = msg_send![self.view, removeFromSuperview];
            let _: () = msg_send![self.view, release];
        }
    }
}

unsafe fn app_window() -> Option<id> {
    let app: id = msg_send![class!(UIApplication), sharedApplication];
    let scenes: id = msg_send![app, connectedScenes];
    let scenes: id = msg_send![scenes, allObjects];
    let count: usize = msg_send![scenes, count];
    for index in 0..count {
        let scene: id = msg_send![scenes, objectAtIndex: index];
        let windowed: BOOL = msg_send![scene, respondsToSelector: sel!(windows)];
        if windowed != YES {
            continue;
        }
        let windows: id = msg_send![scene, windows];
        let window: id = msg_send![windows, firstObject];
        if !window.is_null() {
            return Some(window);
        }
    }
    None
}
