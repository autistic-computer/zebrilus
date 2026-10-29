use std::collections::VecDeque;

use ala::SharedString;
use async_channel::Sender;

use super::*;

const PREVIEW_PROFILE: &str = zz_browser::ARTIFACT_PROFILE;
const PREVIEW_LOAD_HEIGHT: u32 = 801;
const PREVIEW_MIN_HEIGHT: f64 = 8.0;
const PREVIEW_MAX_HEIGHT: f64 = 800.0;
const PREVIEW_FRAME_RATE: i32 = 30;
const PREVIEW_TIMEOUT: Duration = Duration::from_secs(3);
const PREVIEW_IDLE: Duration = Duration::from_mins(1);
const PREVIEW_QUIET: Duration = Duration::from_millis(150);
const PREVIEW_SETTLE_CAP: Duration = Duration::from_millis(1500);
const PREVIEW_BACKGROUND: u32 = 0xffff_ffff;
const PREVIEW_HEIGHT_SCRIPT: &str = "(() => { const body = document.body; \
     if (!body) return document.documentElement.scrollHeight; \
     const style = getComputedStyle(body); \
     let bottom = body.getBoundingClientRect().bottom; \
     if (document.compatMode === 'BackCompat') { const range = document.createRange(); \
     range.selectNodeContents(body); \
     bottom = range.getBoundingClientRect().bottom + (parseFloat(style.paddingBottom) || 0); } \
     return Math.ceil(bottom + (parseFloat(style.marginBottom) || 0) + window.scrollY); })()";

pub(crate) type PreviewResult = Result<Arc<RenderImage>, SharedString>;

struct PreviewJob {
    id: u64,
    url: String,
    width: u32,
    scale: f32,
    done: Sender<PreviewResult>,
}

impl Drop for PreviewJob {
    fn drop(&mut self) {
        zz_browser::drop_local_page(&self.url);
    }
}

enum PreviewStage {
    Loading {
        started: bool,
    },
    Measuring,
    Painting {
        width: u32,
        height: u32,
        since: Instant,
        latest: Option<SettledFrame>,
        generation: u64,
    },
}

struct SettledFrame {
    width: u32,
    height: u32,
    bgra: Vec<u8>,
}

#[derive(Default)]
pub(super) struct Previews {
    session: Option<BrowserSession>,
    creating: bool,
    queue: VecDeque<PreviewJob>,
    current: Option<(PreviewJob, PreviewStage)>,
    next_job: u64,
    idle_generation: u64,
}

impl Previews {
    pub(super) fn busy(&self) -> bool {
        self.creating
            || self.current.is_some()
            || !self.queue.is_empty()
            || self.session.as_ref().is_some_and(|session| {
                matches!(
                    session.phase(),
                    SessionPhase::Creating | SessionPhase::Closing
                )
            })
    }

    pub(super) fn active_session(&self) -> Option<&BrowserSession> {
        self.session
            .as_ref()
            .filter(|session| {
                matches!(
                    session.phase(),
                    SessionPhase::Creating | SessionPhase::Ready
                )
            })
            .filter(|_| self.current.is_some() || self.creating)
    }

    pub(super) fn closed(&self) -> bool {
        self.session
            .as_ref()
            .is_none_or(|session| session.phase() == SessionPhase::Closed)
    }

    pub(super) fn close(&mut self, force: bool) {
        self.fail_all("zz is shutting down");
        if let Some(session) = self.session.as_mut() {
            session.close(force);
        }
    }

    fn fail_all(&mut self, reason: &str) {
        if let Some((job, _)) = self.current.take() {
            let _ = job.done.try_send(Err(reason.to_owned().into()));
        }
        for job in self.queue.drain(..) {
            let _ = job.done.try_send(Err(reason.to_owned().into()));
        }
    }

    fn fail_current(&mut self, reason: &str) {
        if let Some((job, _)) = self.current.take() {
            let _ = job.done.try_send(Err(reason.to_owned().into()));
        }
    }
}

fn device_pixels(css: u32, scale: f32) -> u32 {
    let device = (f64::from(css) * f64::from(scale)).ceil();
    if device.is_finite() && device > 0.0 {
        device.min(f64::from(u32::MAX)) as u32
    } else {
        1
    }
}

fn preview_viewport(width: u32, height: u32, scale: f32) -> Viewport {
    Viewport {
        width,
        height,
        scale_factor: scale,
        window_zoom: 1.0,
        screen_x: 0,
        screen_y: 0,
        visible: true,
    }
}

fn preview_height(measured: f64) -> u32 {
    measured
        .clamp(PREVIEW_MIN_HEIGHT, PREVIEW_MAX_HEIGHT)
        .ceil() as u32
}

fn frame_fits(frame_width: u32, frame_height: u32, width: u32, height: u32) -> bool {
    frame_width.abs_diff(width) <= 1 && frame_height.abs_diff(height) <= 1
}

impl BrowserController {
    pub(crate) fn render_html_preview(
        &mut self,
        html: &str,
        width: u32,
        scale: f32,
        cx: &mut Context<Self>,
    ) -> Task<PreviewResult> {
        let url = match zz_browser::host_local_page(html, false) {
            Ok(url) => url,
            Err(error) => return Task::ready(Err(error.into())),
        };
        let (done, result) = async_channel::bounded(1);
        self.previews.next_job += 1;
        self.previews.queue.push_back(PreviewJob {
            id: self.previews.next_job,
            url,
            width: width.max(1),
            scale,
            done,
        });
        self.drive_previews(cx);
        cx.background_executor().spawn(async move {
            result
                .recv()
                .await
                .unwrap_or_else(|_| Err("the preview was dropped".into()))
        })
    }

    pub(super) fn drive_previews(&mut self, cx: &mut Context<Self>) {
        self.previews.queue.retain(|job| !job.done.is_closed());
        if self.shutting_down {
            self.previews.fail_all("zz is shutting down");
            return;
        }
        if self.previews.current.is_some()
            || self.previews.queue.is_empty()
            || self.previews.creating
            || self.detached_runtime_phase.is_some()
        {
            return;
        }
        if !self.ensure_runtime_started(cx) {
            self.previews.fail_all("the browser is not available");
            return;
        }
        if self.previews.session.is_none() {
            self.create_preview_session(cx);
            return;
        }
        if self
            .previews
            .session
            .as_ref()
            .is_some_and(|session| session.phase() == SessionPhase::Ready)
        {
            self.start_preview_job(cx);
        }
    }

    fn create_preview_session(&mut self, cx: &mut Context<Self>) {
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        if runtime.phase() != RuntimePhase::Running {
            return;
        }
        match runtime.ensure_profile_context(PREVIEW_PROFILE) {
            Ok(true) => {}
            Ok(false) => return,
            Err(error) => {
                self.previews.fail_all(&error.to_string());
                return;
            }
        }
        let Some(job) = self.previews.queue.front() else {
            return;
        };
        let viewport = preview_viewport(job.width, PREVIEW_LOAD_HEIGHT, job.scale);
        let Some(mut runtime) = self.detach_runtime(RuntimePhase::Running) else {
            return;
        };
        self.previews.creating = true;
        cx.spawn(async move |this, cx| {
            let background = runtime.background_color();
            runtime.set_background_color(PREVIEW_BACKGROUND);
            let result = runtime.create_session(
                PREVIEW_PROFILE,
                "about:blank",
                viewport,
                1.0,
                Some(PREVIEW_FRAME_RATE),
                None,
                false,
            );
            runtime.set_background_color(background);
            let mut payload = Some((runtime, result));
            let updated = this.update(cx, |controller, cx| {
                let (runtime, result) = payload
                    .take()
                    .expect("the reattach closure runs at most once");
                controller.reattach_runtime(runtime);
                controller.finish_preview_session(result, cx);
                controller.replay_deferred_runtime_signals(cx);
                controller.try_create_browsers(cx);
                controller.schedule_pump(0, cx);
            });
            if updated.is_err()
                && let Some((mut runtime, _)) = payload.take()
                && let Err(error) = runtime.shutdown()
            {
                log::error!("failed to shut down CEF after losing the browser controller: {error}");
            }
        })
        .detach();
    }

    fn finish_preview_session(
        &mut self,
        result: Result<BrowserSession, BrowserError>,
        cx: &mut Context<Self>,
    ) {
        self.previews.creating = false;
        let mut session = match result {
            Ok(session) => session,
            Err(error) => {
                self.previews.fail_all(&error.to_string());
                return;
            }
        };
        if self.shutting_down {
            session.close(true);
            self.previews.session = Some(session);
            return;
        }
        let events = session.events();
        self.previews.session = Some(session);
        cx.spawn(async move |this, cx| {
            while let Ok(event) = events.recv().await {
                if this
                    .update(cx, |controller, cx| {
                        controller.handle_preview_event(event, cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn start_preview_job(&mut self, cx: &mut Context<Self>) {
        let Some(job) = self.previews.queue.pop_front() else {
            return;
        };
        let Some(session) = self.previews.session.as_mut() else {
            self.previews.queue.push_front(job);
            return;
        };
        session.set_viewport(preview_viewport(job.width, PREVIEW_LOAD_HEIGHT, job.scale));
        let sink = session.command_sink();
        let url = job.url.clone();
        let id = job.id;
        self.previews.current = Some((job, PreviewStage::Loading { started: false }));
        self.queue_cef_work(move || sink.navigate(&url), cx);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PREVIEW_TIMEOUT).await;
            let _ = this.update(cx, |controller, cx| {
                if controller
                    .previews
                    .current
                    .as_ref()
                    .is_some_and(|(job, _)| job.id == id)
                {
                    controller.previews.fail_current("Preview timed out");
                    controller.finish_preview_job(cx);
                }
            });
        })
        .detach();
        self.schedule_pump(0, cx);
    }

    fn finish_preview_job(&mut self, cx: &mut Context<Self>) {
        self.drive_previews(cx);
        if self.previews.busy() {
            return;
        }
        self.previews.idle_generation += 1;
        let generation = self.previews.idle_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PREVIEW_IDLE).await;
            let _ = this.update(cx, |controller, _| {
                if controller.previews.idle_generation == generation
                    && !controller.previews.busy()
                    && let Some(session) = controller.previews.session.as_mut()
                {
                    session.close(false);
                }
            });
        })
        .detach();
    }

    fn handle_preview_event(&mut self, event: BrowserEvent, cx: &mut Context<Self>) {
        let Some(session) = self.previews.session.as_mut() else {
            return;
        };
        if event.session() != session.id() {
            return;
        }
        match event {
            BrowserEvent::Created { .. } => {
                session.mark_ready();
                self.drive_previews(cx);
            }
            BrowserEvent::LoadingChanged { loading: true, .. } => {
                if let Some((_, PreviewStage::Loading { started })) = self.previews.current.as_mut()
                {
                    *started = true;
                }
            }
            BrowserEvent::LoadingChanged { loading: false, .. } => {
                let Some((job, PreviewStage::Loading { started: true })) =
                    self.previews.current.as_ref()
                else {
                    return;
                };
                let id = job.id;
                match session.evaluate_number(PREVIEW_HEIGHT_SCRIPT) {
                    Ok((message_id, measured)) => {
                        if let Some((_, stage)) = self.previews.current.as_mut() {
                            *stage = PreviewStage::Measuring;
                        }
                        cx.spawn(async move |this, cx| {
                            let value = measured.recv().await.ok().flatten();
                            let _ = this.update(cx, |controller, cx| {
                                controller.preview_measured(id, message_id, value, cx);
                            });
                        })
                        .detach();
                    }
                    Err(error) => {
                        self.previews.fail_current(&error.to_string());
                        self.finish_preview_job(cx);
                    }
                }
            }
            BrowserEvent::LoadFailed { description, .. } => {
                if self.previews.current.is_some() {
                    self.previews.fail_current(&description);
                    self.finish_preview_job(cx);
                }
            }
            BrowserEvent::FrameReady { .. } => self.take_preview_frame(cx),
            BrowserEvent::RenderProcessTerminated { .. } => {
                self.previews.fail_current("the preview page crashed");
                if let Some(session) = self.previews.session.as_mut() {
                    session.close(true);
                }
            }
            BrowserEvent::Closed { .. } => {
                if let Some(session) = self.previews.session.as_mut() {
                    session.mark_closed();
                }
                if !self.shutting_down {
                    self.previews.session = None;
                }
                self.previews.fail_current("the preview browser closed");
                self.drive_previews(cx);
            }
            _ => {}
        }
    }

    fn preview_measured(
        &mut self,
        id: u64,
        message_id: i32,
        value: Option<f64>,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.previews.session.as_mut() else {
            return;
        };
        session.finish_site_data_clear(message_id);
        let Some((job, stage)) = self.previews.current.as_mut() else {
            return;
        };
        if job.id != id {
            return;
        }
        let Some(measured) = value else {
            self.previews.fail_current("the page could not be measured");
            self.finish_preview_job(cx);
            return;
        };
        let height = preview_height(measured);
        *stage = PreviewStage::Painting {
            width: device_pixels(job.width, job.scale),
            height: device_pixels(height, job.scale),
            since: Instant::now(),
            latest: None,
            generation: 0,
        };
        session.set_viewport(preview_viewport(job.width, height, job.scale));
        self.schedule_pump(0, cx);
    }

    fn take_preview_frame(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.previews.session.as_ref() else {
            return;
        };
        let Some(frame) = session.take_frame() else {
            return;
        };
        let OsrFrame::OwnedBgra(frame) = frame else {
            return;
        };
        let Some((
            job,
            PreviewStage::Painting {
                width,
                height,
                since,
                latest,
                generation,
            },
        )) = self.previews.current.as_mut()
        else {
            session.recycle_frame(frame.bgra);
            return;
        };
        if !frame_fits(frame.width, frame.height, *width, *height) {
            session.recycle_frame(frame.bgra);
            return;
        }
        if let Some(previous) = latest.replace(SettledFrame {
            width: frame.width,
            height: frame.height,
            bgra: frame.bgra,
        }) {
            session.recycle_frame(previous.bgra);
        }
        if since.elapsed() >= PREVIEW_SETTLE_CAP {
            self.finish_settled_preview(cx);
            return;
        }
        *generation += 1;
        let (id, generation) = (job.id, *generation);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PREVIEW_QUIET).await;
            let _ = this.update(cx, |controller, cx| {
                if matches!(
                    &controller.previews.current,
                    Some((job, PreviewStage::Painting { generation: latest, .. }))
                        if job.id == id && *latest == generation
                ) {
                    controller.finish_settled_preview(cx);
                }
            });
        })
        .detach();
    }

    fn finish_settled_preview(&mut self, cx: &mut Context<Self>) {
        let Some((job, stage)) = self.previews.current.take() else {
            return;
        };
        let PreviewStage::Painting {
            latest: Some(frame),
            ..
        } = stage
        else {
            let _ = job.done.try_send(Err("the preview never painted".into()));
            self.finish_preview_job(cx);
            return;
        };
        let result =
            ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(frame.width, frame.height, frame.bgra)
                .map(|buffer| Arc::new(RenderImage::new(vec![ImageFrame::new(buffer)])))
                .ok_or_else(|| "the preview frame was malformed".into());
        let _ = job.done.try_send(result);
        self.finish_preview_job(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ala::{AppContext as _, TestAppContext};

    #[ala::test]
    fn canceling_a_queued_preview_before_polling_releases_its_page(cx: &mut TestAppContext) {
        let (controller, render, token) = cx.update(|cx| {
            let controller =
                cx.new(|cx| BrowserController::new(Err(BrowserError::AlreadyShutdown), cx));
            let (render, token) = controller.update(cx, |controller, cx| {
                controller.previews.creating = true;
                let render = controller.render_html_preview("<p>queued</p>", 320, 1.0, cx);
                let job = controller.previews.queue.front().unwrap();
                let (token, _) = zz_browser::parse_artifact_url(&job.url).unwrap();
                (render, token)
            });
            (controller, render, token)
        });
        assert!(zz_browser::is_local_page(&token));
        drop(render);
        cx.run_until_parked();
        cx.update(|cx| {
            controller.update(cx, BrowserController::drive_previews);
            assert!(controller.read(cx).previews.queue.is_empty());
        });
        assert!(!zz_browser::is_local_page(&token));
    }

    #[ala::test]
    fn canceling_an_active_preview_keeps_its_page_until_the_job_finishes(cx: &mut TestAppContext) {
        let (controller, render, token) = cx.update(|cx| {
            let controller =
                cx.new(|cx| BrowserController::new(Err(BrowserError::AlreadyShutdown), cx));
            let (render, token) = controller.update(cx, |controller, cx| {
                controller.previews.creating = true;
                let render = controller.render_html_preview("<p>loading</p>", 320, 1.0, cx);
                let job = controller.previews.queue.pop_front().unwrap();
                let (token, _) = zz_browser::parse_artifact_url(&job.url).unwrap();
                controller.previews.current = Some((job, PreviewStage::Loading { started: true }));
                controller.previews.creating = false;
                (render, token)
            });
            (controller, render, token)
        });
        cx.run_until_parked();
        drop(render);
        cx.run_until_parked();
        cx.update(|cx| {
            controller.update(cx, |controller, cx| {
                controller.drive_previews(cx);
                assert!(zz_browser::is_local_page(&token));
                controller.previews.fail_current("Preview timed out");
            });
        });
        assert!(!zz_browser::is_local_page(&token));
    }

    #[test]
    fn previews_measure_at_a_height_they_never_end_at() {
        assert!(preview_height(f64::from(PREVIEW_LOAD_HEIGHT)) < PREVIEW_LOAD_HEIGHT);
        assert_eq!(preview_height(4000.0), 800);
        assert_eq!(preview_height(0.0), 8);
        assert_eq!(preview_height(120.4), 121);
    }

    #[test]
    fn frames_fit_their_device_size_within_a_pixel() {
        assert_eq!(device_pixels(640, 2.0), 1280);
        assert_eq!(device_pixels(333, 1.5), 500);
        assert!(frame_fits(1280, 241, 1280, 242));
        assert!(!frame_fits(1280, 1602, 1280, 242));
    }
}
