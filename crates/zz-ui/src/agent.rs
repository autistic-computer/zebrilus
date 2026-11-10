pub mod ask;
pub mod chat;
pub mod controls;
pub mod presentation;
pub mod preview;
pub mod question;
pub mod refs;
pub mod resume;
pub mod tasks;
pub mod view;

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fmt::Write as _,
    hash::{DefaultHasher, Hash, Hasher},
    rc::Rc,
    sync::{Arc, OnceLock},
};

use web_time::Duration;

use crate::{
    ActiveTheme as _, Colorize as _, Emphasis, Ground, Icon, IconName, StyledExt as _, Tone,
    attachment::{open_attachment_preview, open_render_image_preview},
    button::{Button, ButtonLook, ButtonVariants as _},
    h_flex,
    mend::PENDING_LINK_URL,
    text::{
        CODE_BLOCK_GROUP, CodeBlock, MarkdownExtensions, MarkdownNode, MarkdownParseContext,
        MarkdownPlugin, TextView, TextViewState, TextViewStyle, markdown_ast,
    },
    v_flex,
};
use ala::{
    AnyElement, App, ClipboardItem, Context, Div, ElementId, Global, Hsla, Image, ImageSource,
    IntoElement, Pixels, RenderImage, Rgba, SharedString, Task, Window, canvas, div, img,
    prelude::*, px,
};
use penna::blocks::chat::{
    COMPOSER_ATTACHMENT, PlanState, message_action, shown_on_hover, transcript_ground,
};
#[cfg(test)]
use penna::blocks::chat::{activity_color, disclosure_icon, single_line};
use penna::{Card, CardLook, TextStyle};

const MERMAID_NODE_NAME: &str = "zz-mermaid";
const SVG_NODE_NAME: &str = "zz-svg";
const RICH_MARKDOWN_NODE_NAME: &str = "zz-rich-markdown";
const FIGURE_MAX_SOURCE_BYTES: usize = 32 * 1024;
const FIGURE_RENDER_DEBOUNCE: Duration = Duration::from_millis(250);
const FIGURE_CACHE_CAPACITY: usize = 16;
const HTML_PREVIEW_BUDGET_BYTES: usize = 64 * 1024 * 1024;
const HTML_PREVIEW_WIDTH_STEP: f32 = 32.0;
pub(crate) const MAX_STREAMING_MEND_BYTES: usize = 64 * 1024;
const INERT_LINK_URL: &str = "data:,";

pub use zz_client::agent_timeline::plan_progress;
use zz_client::agent_timeline::{PlanItemState, plan_items};

pub fn agent_jump_to_bottom_button(id: impl Into<ElementId>, cx: &App) -> Button {
    Button::compact_icon(id, IconName::ArrowDown)
        .secondary()
        .look(ButtonLook::Floating)
        .on(transcript_ground(cx))
        .tooltip("Jump to latest")
}

fn attachment_tile(
    id: impl Into<ElementId>,
    image: Arc<Image>,
    side: Pixels,
    ground: Ground,
) -> Card {
    penna::blocks::chat::attachment_tile(id, image, side, ground, |image, window, cx| {
        open_attachment_preview(image, window, cx);
    })
}

pub fn agent_attachment_thumbnail(
    id: impl Into<ElementId>,
    image: Arc<Image>,
    side: Pixels,
    ground: Ground,
) -> Card {
    attachment_tile(id, image, side, ground)
}

pub fn composer_attachment(
    id: impl Into<ElementId>,
    remove: impl Into<ElementId>,
    image: Arc<Image>,
    on_remove: impl Fn(&mut Window, &mut App) + 'static,
    ground: Ground,
) -> Div {
    penna::blocks::chat::composer_attachment(
        agent_attachment_thumbnail(id, image, COMPOSER_ATTACHMENT, ground),
        remove,
        on_remove,
    )
}

pub(crate) fn plan_entries(source: &str) -> Vec<(PlanState, SharedString)> {
    plan_items(source)
        .into_iter()
        .map(|(state, text)| {
            let state = match state {
                PlanItemState::Pending => PlanState::Pending,
                PlanItemState::InProgress => PlanState::Current,
                PlanItemState::Done => PlanState::Done,
            };
            (state, SharedString::from(text.to_owned()))
        })
        .collect()
}

fn agent_markdown_style(cx: &App) -> TextViewStyle {
    TextViewStyle {
        text: None,
        highlight_theme: Arc::clone(&cx.theme().highlight_theme),
        is_dark: cx.theme().is_dark(),
        ..TextViewStyle::default()
    }
}

pub(crate) fn agent_text_view(
    text: TextView,
    scope: u64,
    owner: Option<u64>,
    cx: &App,
) -> TextView {
    text.style(agent_markdown_style(cx))
        .code_block_actions(move |code_block, window, cx| {
            agent_code_block_chrome(code_block, scope, window, cx)
        })
        .code_block_body(move |code_block, window, cx| {
            agent_code_block_body(code_block, scope, owner, window, cx)
        })
}

pub struct AgentHtmlOpener(pub Rc<dyn Fn(String, &mut Window, &mut App)>);

impl Global for AgentHtmlOpener {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HtmlPreviewSize {
    pub width: Pixels,
    pub scale: f32,
    pub dark: bool,
}

pub type HtmlPreviewTask = Task<Result<Arc<RenderImage>, SharedString>>;

pub struct AgentHtmlPreviewer(pub Rc<dyn Fn(String, HtmlPreviewSize, &mut App) -> HtmlPreviewTask>);

impl Global for AgentHtmlPreviewer {}

pub struct AgentHtmlLive {
    pub live: Rc<dyn Fn(u64, &App) -> Option<AnyElement>>,
    pub activate: Rc<dyn Fn(HtmlLiveBlock, &mut Window, &mut App)>,
}

pub struct HtmlLiveBlock {
    pub block: u64,
    pub html: String,
    pub size: ala::Size<Pixels>,
    pub owner: Option<u64>,
}

impl Global for AgentHtmlLive {}

#[derive(Default)]
struct HtmlPreviewToggles(HashSet<u64>);

impl Global for HtmlPreviewToggles {}

#[derive(Default)]
struct HtmlPreviewCache {
    images: HashMap<u64, Arc<RenderImage>>,
    order: VecDeque<u64>,
    bytes: usize,
}

impl Global for HtmlPreviewCache {}

impl HtmlPreviewCache {
    fn get(&mut self, key: u64) -> Option<Arc<RenderImage>> {
        let image = self.images.get(&key).cloned()?;
        self.order.retain(|entry| *entry != key);
        self.order.push_back(key);
        Some(image)
    }

    fn insert(&mut self, key: u64, image: Arc<RenderImage>) {
        if self.images.contains_key(&key) {
            return;
        }
        self.bytes += render_image_bytes(&image);
        self.images.insert(key, image);
        self.order.push_back(key);
        while self.bytes > HTML_PREVIEW_BUDGET_BYTES && self.order.len() > 1 {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(image) = self.images.remove(&oldest) {
                self.bytes = self.bytes.saturating_sub(render_image_bytes(&image));
            }
        }
    }
}

fn render_image_bytes(image: &RenderImage) -> usize {
    let size = image.size(0);
    usize::try_from(size.width.0).unwrap_or(0) * usize::try_from(size.height.0).unwrap_or(0) * 4
}

fn html_block_id(scope: u64, code_block: &CodeBlock) -> u64 {
    let mut hasher = DefaultHasher::new();
    scope.hash(&mut hasher);
    code_block.span.map(|span| span.start).hash(&mut hasher);
    code_block.code().hash(&mut hasher);
    hasher.finish()
}

fn html_preview_shown(block: u64, preferred: bool, cx: &App) -> bool {
    preferred
        != cx
            .try_global::<HtmlPreviewToggles>()
            .is_some_and(|toggles| toggles.0.contains(&block))
}

fn previews_by_default(code_block: &CodeBlock) -> bool {
    code_block.meta().is_some_and(|meta| {
        meta.split_whitespace()
            .any(|word| word.eq_ignore_ascii_case("preview"))
    })
}

fn is_html_document(code: &str) -> bool {
    let head = code.trim_start();
    ["<!doctype", "<html"].iter().any(|start| {
        head.get(..start.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(start))
    })
}

fn html_block_page(code: &str, head: Option<&str>) -> String {
    match head {
        Some(head) if !is_html_document(code) => format!("{head}{code}</body></html>"),
        _ => code.to_owned(),
    }
}

fn visual_page_head(cx: &App) -> String {
    let colors = MermaidThemeColors::from_app(cx);
    let dark = cx.theme().is_dark();
    let [blue, orange, purple] = if dark {
        ["#5ea1ff", "#f0a050", "#b195ff"]
    } else {
        ["#2f6fd6", "#c96a12", "#7c4ddb"]
    };
    let variables = [
        ("background", css_color(colors.background)),
        ("foreground", css_color(colors.foreground)),
        ("card", css_color(colors.surface)),
        ("card-foreground", css_color(colors.foreground)),
        ("popover", css_color(colors.surface)),
        ("popover-foreground", css_color(colors.foreground)),
        ("primary", css_color(colors.primary)),
        ("primary-foreground", css_color(colors.primary_foreground)),
        ("secondary", css_color(colors.muted)),
        ("secondary-foreground", css_color(colors.foreground)),
        ("muted", css_color(colors.muted)),
        ("muted-foreground", css_color(colors.muted_foreground)),
        ("accent", css_color(colors.muted)),
        ("accent-foreground", css_color(colors.foreground)),
        ("destructive", css_color(colors.danger)),
        ("border", css_color(colors.border)),
        ("input", css_color(colors.border)),
        ("ring", css_color(colors.muted_foreground)),
        ("red", css_color(colors.danger)),
        ("green", css_color(colors.success)),
        ("yellow", css_color(colors.warning)),
        ("blue", blue.to_owned()),
        ("orange", orange.to_owned()),
        ("purple", purple.to_owned()),
        ("viz-series-1", css_color(cx.theme().accent)),
        ("viz-series-2", blue.to_owned()),
        ("viz-series-3", orange.to_owned()),
        ("viz-series-4", css_color(colors.success)),
        ("viz-series-5", purple.to_owned()),
        ("viz-series-6", css_color(colors.danger)),
    ];
    let root = variables
        .iter()
        .fold(String::new(), |mut root, (name, value)| {
            let _ = write!(root, "--{name}:{value};");
            root
        });
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><style>:root{{color-scheme:{};font-family:{};--font-size-base:14px;{root}}}{VISUAL_PAGE_CSS}</style><script>{VISUAL_PAGE_SCRIPT}</script></head><body>",
        if dark { "dark" } else { "light" },
        mermaid_font_family(cx.theme().font_family.as_ref(), "system-ui", "sans-serif"),
    )
}

const VISUAL_PAGE_CSS: &str = r"*,*::before,*::after{box-sizing:border-box}
html{background:var(--background);color:var(--foreground);font-size:var(--font-size-base);line-height:1.5}
body{margin:0;padding:2px}
h1,h2,h3{font-weight:500;line-height:1.25;margin:0 0 .5em}
h1{font-size:1.5em}h2{font-size:1.25em}h3{font-size:1.1em}
p{margin:0 0 .75em}
a{color:var(--viz-series-1)}
hr{border:0;border-top:1px solid var(--border);margin:12px 0}
table{border-collapse:collapse;width:100%}
th,td{padding:4px 8px;border-bottom:1px solid var(--border);text-align:left}
th{font-weight:500;color:var(--muted-foreground)}
.table-responsive{overflow-x:auto}
.text-small{font-size:12px}
.text-muted{color:var(--muted-foreground)}
.tabular-nums{font-variant-numeric:tabular-nums}
.card{background:var(--card);color:var(--card-foreground);border:1px solid var(--border);border-radius:10px;padding:12px 14px}
.viz-grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:12px}
.viz-row,.viz-controls{display:flex;flex-wrap:wrap;align-items:center;gap:8px 12px}
.viz-stat-value{font-size:1.6em;font-weight:500;font-variant-numeric:tabular-nums}
.viz-badge{display:inline-block;padding:0 8px;border-radius:999px;background:var(--muted);font-size:12px}
.btn{font:inherit;color:var(--secondary-foreground);background:var(--secondary);border:1px solid var(--border);border-radius:8px;padding:4px 12px;cursor:pointer}
.btn-primary,.btn[aria-pressed=true],.btn[aria-selected=true],.btn.is-selected{background:var(--primary);color:var(--primary-foreground);border-color:var(--primary)}
.viz-tile{width:100%;height:100%}
.form-label{display:flex;flex-direction:column;gap:4px;font-size:12px;color:var(--muted-foreground)}
.form-control,.form-select{font:inherit;color:var(--foreground);background:var(--background);border:1px solid var(--input);border-radius:6px;padding:4px 8px}
.form-range{accent-color:var(--viz-series-1)}
.nav-pills{display:flex;flex-wrap:wrap;gap:4px}
.nav-justified>.nav-link{flex:1}
.nav-link{font:inherit;color:var(--muted-foreground);background:none;border:0;border-radius:8px;padding:4px 12px;cursor:pointer}
.nav-link.active{color:var(--foreground);background:var(--muted)}
.progress{height:6px;border-radius:3px;background:var(--muted);overflow:hidden}
.progress-bar{height:100%;background:var(--viz-series-1)}
.tooltip{position:absolute;pointer-events:none;background:var(--popover);color:var(--popover-foreground);border:1px solid var(--border);border-radius:6px;padding:4px 8px;font-size:12px}
.viz-dotted-background{background-image:radial-gradient(var(--border) 1px,transparent 1px);background-size:16px 16px}
";

const VISUAL_PAGE_SCRIPT: &str = r#"window.openai={widgetState:null,setWidgetState:function(){return Promise.resolve()},sendFollowUpMessage:function(message){return window.zz&&window.zz.reply?window.zz.reply(String(message&&message.prompt||message)):Promise.reject(new Error("this page cannot send follow-ups"))}};"#;

fn previews_html(language: &str, cx: &App) -> bool {
    language.eq_ignore_ascii_case("html") && cx.has_global::<AgentHtmlPreviewer>()
}

fn html_preview_width(width: Pixels) -> Pixels {
    px((f32::from(width) / HTML_PREVIEW_WIDTH_STEP)
        .floor()
        .max(2.0)
        * HTML_PREVIEW_WIDTH_STEP)
}

#[derive(Default)]
enum HtmlPreviewResult {
    #[default]
    Waiting,
    Ready(Arc<RenderImage>, f32),
    Failed(SharedString),
}

#[derive(Default)]
struct HtmlPreviewState {
    width: Option<Pixels>,
    result: HtmlPreviewResult,
    render_key: Option<u64>,
    _task: Option<Task<()>>,
}

impl HtmlPreviewState {
    fn synchronize(
        &mut self,
        code: &str,
        head: Option<&str>,
        size: HtmlPreviewSize,
        cx: &mut Context<Self>,
    ) {
        let mut hasher = DefaultHasher::new();
        code.hash(&mut hasher);
        head.hash(&mut hasher);
        f32::from(size.width).to_bits().hash(&mut hasher);
        size.scale.to_bits().hash(&mut hasher);
        size.dark.hash(&mut hasher);
        let key = hasher.finish();
        if self.render_key == Some(key) {
            return;
        }
        self.render_key = Some(key);
        if let Some(image) = cx.default_global::<HtmlPreviewCache>().get(key) {
            self.result = HtmlPreviewResult::Ready(image, size.scale);
            self._task = None;
            cx.notify();
            return;
        }
        self.result = HtmlPreviewResult::Waiting;
        let source = html_block_page(code, head);
        let delay = cx.background_executor().timer(FIGURE_RENDER_DEBOUNCE);
        self._task = Some(cx.spawn(async move |this: ala::WeakEntity<Self>, cx| {
            delay.await;
            let Ok(Some(render)) = this.update(cx, |_, cx| {
                cx.try_global::<AgentHtmlPreviewer>()
                    .map(|previewer| Rc::clone(&previewer.0))
                    .map(|previewer| previewer(source, size, cx))
            }) else {
                return;
            };
            let result = render.await;
            let _ = this.update(cx, |state, cx| {
                if state.render_key != Some(key) {
                    return;
                }
                state.result = match result {
                    Ok(image) => {
                        cx.default_global::<HtmlPreviewCache>()
                            .insert(key, Arc::clone(&image));
                        HtmlPreviewResult::Ready(image, size.scale)
                    }
                    Err(error) => HtmlPreviewResult::Failed(error),
                };
                cx.notify();
            });
        }));
        cx.notify();
    }
}

fn agent_code_block_body(
    code_block: &CodeBlock,
    scope: u64,
    owner: Option<u64>,
    window: &mut Window,
    cx: &mut App,
) -> Option<AnyElement> {
    let language = code_block_language(code_block.lang());
    let code = code_block.code();
    let visual = previews_by_default(code_block);
    let block = html_block_id(scope, code_block);
    if !previews_html(&language, cx) || !html_preview_shown(block, visual, cx) {
        return None;
    }
    let head = visual.then(|| visual_page_head(cx));
    let state = window.use_keyed_state(
        SharedString::from(format!("zz-html-preview/{block}")),
        cx,
        |_, _| HtmlPreviewState::default(),
    );
    if let Some(width) = state.read(cx).width {
        let size = HtmlPreviewSize {
            width: html_preview_width(width),
            scale: window.scale_factor(),
            dark: cx.theme().is_dark(),
        };
        state.update(cx, |state, cx| {
            state.synchronize(&code, head.as_deref(), size, cx);
        });
    }
    let measure = canvas(
        {
            let state = state.clone();
            move |bounds, window, cx| {
                let changed = state.update(cx, |state, _| {
                    state.width.replace(bounds.size.width) != Some(bounds.size.width)
                });
                if changed {
                    window.request_animation_frame();
                }
            }
        },
        |_, (), _, _| {},
    )
    .absolute()
    .size_full();
    let content = match &state.read(cx).result {
        HtmlPreviewResult::Waiting => h_flex()
            .w_full()
            .min_h(px(120.0))
            .items_center()
            .justify_center()
            .gap_2()
            .type_style(TextStyle::Caption, cx)
            .text_color(cx.theme().foreground.muted())
            .font_family(cx.theme().font_family.clone())
            .child(
                Icon::new(IconName::Loader)
                    .medium()
                    .text_color(cx.theme().foreground.muted()),
            )
            .child("Rendering preview…")
            .into_any_element(),
        HtmlPreviewResult::Ready(image, scale) => {
            let size = image.size(0);
            let scale = scale.max(1.0);
            let picture = ala::size(
                px(size.width.0 as f32 / scale),
                px(size.height.0 as f32 / scale),
            );
            let live = cx
                .try_global::<AgentHtmlLive>()
                .map(|live| (Rc::clone(&live.live), Rc::clone(&live.activate)));
            let opener = cx
                .try_global::<AgentHtmlOpener>()
                .map(|opener| Rc::clone(&opener.0));
            let page = code.clone();
            let head = head.clone();
            if let Some(element) = live.as_ref().and_then(|(live, _)| live(block, cx)) {
                div()
                    .debug_selector(|| "agent-html-live".to_owned())
                    .w(picture.width)
                    .h(picture.height)
                    .child(element)
                    .into_any_element()
            } else {
                let tooltip = if live.is_some() {
                    "Click to use the page"
                } else {
                    "Open as a page"
                };
                div()
                    .id(("agent-html-preview-open", block))
                    .w_full()
                    .min_w_0()
                    .overflow_hidden()
                    .when(live.is_some() || opener.is_some(), |this| {
                        this.click_cursor().tooltip(move |window, cx| {
                            crate::tooltip::Tooltip::new(tooltip).build(window, cx)
                        })
                    })
                    .on_click(move |_, window, cx| {
                        let page = html_block_page(&page, head.as_deref());
                        match (&live, &opener) {
                            (Some((_, activate)), _) => activate(
                                HtmlLiveBlock {
                                    block,
                                    html: page,
                                    size: picture,
                                    owner,
                                },
                                window,
                                cx,
                            ),
                            (None, Some(opener)) => opener(page, window, cx),
                            (None, None) => {}
                        }
                    })
                    .child(
                        img(ImageSource::Render(Arc::clone(image)))
                            .debug_selector(|| "agent-html-preview".to_owned())
                            .w(picture.width)
                            .h(picture.height),
                    )
                    .into_any_element()
            }
        }
        HtmlPreviewResult::Failed(error) => h_flex()
            .w_full()
            .min_h(px(40.0))
            .items_center()
            .gap_2()
            .type_style(TextStyle::Caption, cx)
            .font_family(cx.theme().font_family.clone())
            .text_color(cx.theme().foreground.muted())
            .child(
                Icon::new(IconName::TriangleAlert)
                    .medium()
                    .text_color(cx.theme().warning),
            )
            .child(error.clone())
            .into_any_element(),
    };
    Some(
        div()
            .relative()
            .w_full()
            .min_w_0()
            .child(measure)
            .child(content)
            .into_any_element(),
    )
}

fn agent_code_block_chrome(
    code_block: &CodeBlock,
    scope: u64,
    _window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let language = code_block_language(code_block.lang());
    let code = code_block.code();
    let source_offset = code_block.span.map_or(0, |span| span.start);
    let opener = language
        .eq_ignore_ascii_case("html")
        .then(|| {
            cx.try_global::<AgentHtmlOpener>()
                .map(|opener| Rc::clone(&opener.0))
        })
        .flatten();
    let visual = previews_by_default(code_block);
    let block = html_block_id(scope, code_block);
    let preview = previews_html(&language, cx).then(|| html_preview_shown(block, visual, cx));
    let page = code.clone();
    let ground = cx.theme().ground_of(CardLook::Code);

    h_flex()
        .w_full()
        .h(px(28.0))
        .items_center()
        .justify_between()
        .pl_1()
        .child(
            div()
                .debug_selector(|| "agent-code-language".to_owned())
                .type_style(TextStyle::Micro, cx)
                .text_color(cx.theme().foreground.muted())
                .child(preview::code_name(
                    code_block
                        .lang()
                        .as_deref()
                        .filter(|language| !language.trim().is_empty()),
                )),
        )
        .child(
            shown_on_hover(CODE_BLOCK_GROUP, cx).child(
                h_flex()
                    .gap_1()
                    .when_some(preview, |this, shown| {
                        let (icon, tooltip) = if shown {
                            (IconName::Braces, "Show code")
                        } else {
                            (IconName::AppWindow, "Preview")
                        };
                        this.child(
                            message_action(
                                Button::compact_icon(("agent-preview-html", source_offset), icon)
                                    .on(ground)
                                    .tooltip(tooltip)
                                    .on_click(move |_, window, cx| {
                                        let toggles =
                                            &mut cx.default_global::<HtmlPreviewToggles>().0;
                                        if !toggles.remove(&block) {
                                            toggles.insert(block);
                                        }
                                        window.refresh();
                                    }),
                                cx,
                            )
                            .debug_selector(|| "agent-code-preview".to_owned()),
                        )
                    })
                    .when_some(opener, |this, opener| {
                        this.child(
                            message_action(
                                Button::compact_icon(
                                    ("agent-open-html", source_offset),
                                    IconName::Globe,
                                )
                                .on(ground)
                                .tooltip("Open as a page")
                                .on_click(move |_, window, cx| {
                                    let head = visual.then(|| visual_page_head(cx));
                                    opener(html_block_page(&page, head.as_deref()), window, cx);
                                }),
                                cx,
                            )
                            .debug_selector(|| "agent-code-open".to_owned()),
                        )
                    })
                    .child(
                        message_action(
                            Button::compact_icon(
                                ("agent-copy-code", source_offset),
                                IconName::Copy,
                            )
                            .on(ground)
                            .tooltip("Copy code")
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(code.to_string()));
                            }),
                            cx,
                        )
                        .debug_selector(|| "agent-code-copy".to_owned()),
                    ),
            ),
        )
        .into_any_element()
}

fn code_block_language(language: Option<SharedString>) -> SharedString {
    language
        .filter(|language| !language.trim().is_empty())
        .unwrap_or_else(|| SharedString::from("text"))
}

fn standard_markdown_extensions() -> MarkdownExtensions {
    static EXTENSIONS: OnceLock<MarkdownExtensions> = OnceLock::new();
    EXTENSIONS
        .get_or_init(|| {
            MarkdownExtensions::default()
                .plugin(MermaidPlugin)
                .plugin(SvgPlugin)
        })
        .clone()
}

pub(crate) fn assistant_markdown_extensions() -> MarkdownExtensions {
    static EXTENSIONS: OnceLock<MarkdownExtensions> = OnceLock::new();
    EXTENSIONS
        .get_or_init(|| assistant_plugins(MarkdownExtensions::default()))
        .clone()
}

pub(crate) fn previewing_markdown_extensions(open: preview::ViewOpener) -> MarkdownExtensions {
    assistant_plugins(MarkdownExtensions::default().plugin(preview::PreviewPlugin { open }))
}

fn assistant_plugins(extensions: MarkdownExtensions) -> MarkdownExtensions {
    extensions
        .plugin(MermaidPlugin)
        .plugin(SvgPlugin)
        .plugin(RichMarkdownPlugin)
        .link_rewriter(|url| (url == PENDING_LINK_URL).then(|| INERT_LINK_URL.to_owned()))
}

#[derive(Clone)]
struct RichMarkdownSource {
    source: String,
    source_offset: usize,
}

#[derive(Clone, Copy)]
struct RichMarkdownPlugin;

impl MarkdownPlugin for RichMarkdownPlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        RICH_MARKDOWN_NODE_NAME
    }

    fn parse<'a>(
        &self,
        node: &'a markdown_ast::AstNode<'a>,
        cx: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        let code = cx.code_fence(node)?;
        if !code.lang.as_deref().is_some_and(is_markdown_language) {
            return None;
        }
        if code.value.len() > MAX_STREAMING_MEND_BYTES {
            return None;
        }
        let source_offset = code.start;
        Some(
            MarkdownNode::new(
                RICH_MARKDOWN_NODE_NAME,
                RichMarkdownSource {
                    source: code.value.clone(),
                    source_offset,
                },
            )
            .text(code.value.clone())
            .markdown(cx.node_source(node).unwrap_or(&code.value)),
        )
    }

    fn render(&self, node: &MarkdownNode, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let source = node
            .data::<RichMarkdownSource>()
            .expect("rich markdown node data");
        let key = SharedString::from(format!("zz-rich-markdown/{}", source.source_offset));
        let initial_source = source.source.clone();
        let retained = window.use_keyed_state(key, cx, move |_, cx| {
            cx.new(|cx| TextViewState::markdown(&initial_source, cx))
        });
        let state = retained.read(cx).clone();
        state.update(cx, |state, cx| {
            state.synchronize_markdown(&source.source, false, cx);
        });
        let scope = state.entity_id().as_u64();
        div().w_full().min_w_0().child(
            agent_text_view(TextView::new(&state), scope, None, cx)
                .max_w_full()
                .min_w_0()
                .selectable(true)
                .markdown_extensions(standard_markdown_extensions()),
        )
    }
}

fn is_markdown_language(language: &str) -> bool {
    language.eq_ignore_ascii_case("markdown") || language.eq_ignore_ascii_case("md")
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum FigureKind {
    Mermaid,
    Svg,
}

impl FigureKind {
    fn language(self) -> &'static str {
        match self {
            Self::Mermaid => "mermaid",
            Self::Svg => "svg",
        }
    }

    fn node_name(self) -> &'static str {
        match self {
            Self::Mermaid => MERMAID_NODE_NAME,
            Self::Svg => SVG_NODE_NAME,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Mermaid => "Mermaid",
            Self::Svg => "SVG",
        }
    }
}

#[derive(Clone)]
struct FigureSource {
    kind: FigureKind,
    source: String,
    source_offset: usize,
    scale: f32,
}

#[derive(Clone, Copy)]
struct MermaidPlugin;

impl MarkdownPlugin for MermaidPlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        MERMAID_NODE_NAME
    }

    fn parse<'a>(
        &self,
        node: &'a markdown_ast::AstNode<'a>,
        cx: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        parse_figure(FigureKind::Mermaid, node, cx)
    }

    fn render(&self, node: &MarkdownNode, window: &mut Window, cx: &mut App) -> impl IntoElement {
        render_figure(node, window, cx)
    }
}

#[derive(Clone, Copy)]
struct SvgPlugin;

impl MarkdownPlugin for SvgPlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        SVG_NODE_NAME
    }

    fn parse<'a>(
        &self,
        node: &'a markdown_ast::AstNode<'a>,
        cx: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        parse_figure(FigureKind::Svg, node, cx)
    }

    fn render(&self, node: &MarkdownNode, window: &mut Window, cx: &mut App) -> impl IntoElement {
        render_figure(node, window, cx)
    }
}

fn parse_figure(
    kind: FigureKind,
    node: &markdown_ast::AstNode<'_>,
    cx: &MarkdownParseContext<'_>,
) -> Option<MarkdownNode> {
    let code = cx.code_fence(node)?;
    if !code
        .lang
        .as_deref()
        .is_some_and(|language| language.eq_ignore_ascii_case(kind.language()))
        || code.value.len() > FIGURE_MAX_SOURCE_BYTES
        || (kind == FigureKind::Mermaid && !pele::supports(&code.value))
    {
        return None;
    }
    let scale = code
        .meta
        .as_deref()
        .and_then(|meta| meta.split_whitespace().next())
        .and_then(|scale| scale.parse::<f32>().ok())
        .unwrap_or(100.0)
        .clamp(25.0, 200.0);
    let source_offset = code.start;
    Some(
        MarkdownNode::new(
            kind.node_name(),
            FigureSource {
                kind,
                source: code.value.clone(),
                source_offset,
                scale,
            },
        )
        .text(code.value.clone())
        .markdown(cx.node_source(node).unwrap_or(&code.value)),
    )
}

fn figure_state(
    source: &FigureSource,
    window: &mut Window,
    cx: &mut App,
) -> ala::Entity<FigureRenderState> {
    let kind = source.kind;
    let theme_key = mermaid_theme_key(cx);
    let mut hasher = DefaultHasher::new();
    kind.hash(&mut hasher);
    source.source.hash(&mut hasher);
    source.scale.to_bits().hash(&mut hasher);
    theme_key.hash(&mut hasher);
    let render_key = hasher.finish();
    let key = SharedString::from(format!(
        "{}/{}/{theme_key}/{}",
        kind.node_name(),
        source.source_offset,
        source.scale.to_bits()
    ));
    let render_state = window.use_keyed_state(key, cx, {
        let source = source.clone();
        move |_, cx| FigureRenderState::new(source, render_key, cx)
    });
    render_state.update(cx, |state, cx| {
        state.synchronize(source.clone(), render_key, cx);
    });
    render_state
}

pub(crate) fn figure_image(
    language: &str,
    source: &str,
    key: usize,
    window: &mut Window,
    cx: &mut App,
) -> FigureImage {
    let kind = if language.eq_ignore_ascii_case("svg") {
        FigureKind::Svg
    } else {
        FigureKind::Mermaid
    };
    let source = FigureSource {
        kind,
        source: source.to_owned(),
        source_offset: key,
        scale: 100.0,
    };
    match &figure_state(&source, window, cx).read(cx).result {
        FigureRenderResult::Pending => FigureImage::Pending,
        FigureRenderResult::Ready(image) => FigureImage::Ready(Arc::clone(image)),
        FigureRenderResult::Failed(error) => FigureImage::Failed(error.clone()),
    }
}

pub(crate) enum FigureImage {
    Pending,
    Ready(Arc<RenderImage>),
    Failed(SharedString),
}

fn render_figure(node: &MarkdownNode, window: &mut Window, cx: &mut App) -> impl IntoElement {
    let source = node
        .data::<FigureSource>()
        .expect("figure markdown node data");
    let kind = source.kind;
    let render_state = figure_state(source, window, cx);

    let preview_image = match &render_state.read(cx).result {
        FigureRenderResult::Ready(image) => Some(Arc::clone(image)),
        _ => None,
    };
    let mermaid_source = source.source.clone();
    let source_offset = source.source_offset;
    let content = match &render_state.read(cx).result {
        FigureRenderResult::Pending => h_flex()
            .w_full()
            .min_h(px(120.0))
            .items_center()
            .justify_center()
            .gap_2()
            .type_style(TextStyle::Caption, cx)
            .text_color(cx.theme().foreground.muted())
            .child(
                Icon::new(IconName::Loader)
                    .medium()
                    .text_color(cx.theme().foreground.muted())
                    .relative()
                    .top(crate::icon::cap_center_drop(px(11.0), px(14.0), cx)),
            )
            .child(format!("Rendering {}…", kind.label()))
            .into_any_element(),
        FigureRenderResult::Ready(image) => div()
            .id(("agent-mermaid", source.source_offset))
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .child(
                div().flex().w_full().justify_center().child(
                    img(ImageSource::Render(image.clone()))
                        .debug_selector(|| "agent-figure-image".to_owned())
                        .max_w_full()
                        .mx_auto(),
                ),
            )
            .into_any_element(),
        FigureRenderResult::Failed(error) => v_flex()
            .w_full()
            .gap_2()
            .type_style(TextStyle::Caption, cx)
            .child(
                h_flex()
                    .gap_2()
                    .text_color(cx.theme().danger)
                    .child(
                        Icon::new(IconName::TriangleAlert)
                            .medium()
                            .relative()
                            .top(crate::icon::cap_center_drop(px(11.0), px(14.0), cx)),
                    )
                    .child(format!("{} could not be rendered", kind.label())),
            )
            .child(
                div()
                    .text_color(cx.theme().foreground.muted())
                    .child(error.clone()),
            )
            .into_any_element(),
    };

    let card = cx.theme().ground_of(CardLook::Figure);
    let hover_only = !crate::touch::CoarsePointer::get(cx);
    Card::new(("agent-figure", source_offset), CardLook::Figure)
        .group(FIGURE_GROUP)
        .relative()
        .p_0()
        .v_flex()
        .w_full()
        .overflow_hidden()
        .child(div().w_full().py_2().child(content))
        .child(
            h_flex()
                .absolute()
                .top_0()
                .right_0()
                .gap_1()
                .items_center()
                .when(hover_only, |actions| {
                    actions
                        .invisible()
                        .group_hover(FIGURE_GROUP, ala::Styled::visible)
                })
                .child(message_action(
                    Button::compact_icon(("agent-mermaid-copy", source_offset), IconName::Copy)
                        .on(card)
                        .tooltip("Copy source")
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(
                                mermaid_source.clone(),
                            ));
                        }),
                    cx,
                ))
                .when_some(preview_image, |this, image| {
                    this.child(message_action(
                        Button::compact_icon(
                            ("agent-mermaid-preview", source_offset),
                            IconName::WindowMaximize,
                        )
                        .on(card)
                        .tooltip("Open full diagram")
                        .on_click(move |_, window, cx| {
                            open_render_image_preview(Arc::clone(&image), window, cx);
                        }),
                        cx,
                    ))
                }),
        )
}

const FIGURE_GROUP: &str = "agent-figure";

const MERMAID_FONT_SIZE: f64 = 13.0;
const MERMAID_STROKE_WIDTH: f64 = 1.25;
const MERMAID_LAYOUT_WIDTH: f64 = 640.0;
const MERMAID_MEASURE_SIZE: f32 = 64.0;
const MERMAID_SERIES: usize = 8;
const MERMAID_SERIES_LIGHTNESS_LIGHT: (f32, f32) = (0.5, 0.58);
const MERMAID_SERIES_LIGHTNESS_DARK: (f32, f32) = (0.68, 0.78);

fn mermaid_options(cx: &App) -> pele::RenderOptions {
    let theme = cx.theme();
    let measurer = AppTextMeasurer {
        text_system: cx.text_system().clone(),
        sans: ala::font(theme.font_family.clone()),
        mono: ala::font(theme.mono_font_family.clone()),
        advances: parking_lot::Mutex::default(),
    };
    mermaid_render_options(
        MermaidThemeColors::from_app(cx),
        [theme.font_family.as_ref(), theme.mono_font_family.as_ref()],
        f64::from(penna::blocks::chat::diagram_paint(theme).radius),
        Some(Arc::new(measurer)),
    )
}

fn mermaid_render_options(
    colors: MermaidThemeColors,
    [font_family, mono_font_family]: [&str; 2],
    radius: f64,
    measurer: Option<Arc<dyn pele::text::TextMeasurer>>,
) -> pele::RenderOptions {
    let series = colors
        .series()
        .into_iter()
        .map(css_color)
        .collect::<Vec<_>>();
    pele::RenderOptions {
        font_size: MERMAID_FONT_SIZE,
        font_family: mermaid_font_family(font_family, "system-ui", "sans-serif"),
        font_family_mono: mermaid_font_family(mono_font_family, "monospace", "monospace"),
        id_prefix: "zz-mermaid".to_owned(),
        max_width: Some(MERMAID_LAYOUT_WIDTH),
        transparent: true,
        theme: pele::Theme {
            background: css_color(colors.background),
            foreground: css_color(colors.foreground),
            muted: css_color(colors.muted_foreground),
            line: css_color(colors.muted_foreground),
            surface: css_color(colors.surface),
            surface_alt: css_color(colors.muted),
            border: css_color(colors.border),
            accent: series[0].clone(),
            node: Some(css_color(colors.node)),
            node_border: Some(css_color(colors.node_border)),
            series,
            radius,
            stroke_width: MERMAID_STROKE_WIDTH,
        },
        measurer,
        ..pele::RenderOptions::default()
    }
}

struct AppTextMeasurer {
    text_system: Arc<ala::TextSystem>,
    sans: ala::Font,
    mono: ala::Font,
    advances: parking_lot::Mutex<HashMap<(char, u8), f32>>,
}

impl AppTextMeasurer {
    fn advance(&self, character: char, style: u8) -> f32 {
        let mut font = if style & pele::text::MONO == 0 {
            self.sans.clone()
        } else {
            self.mono.clone()
        };
        if style & pele::text::BOLD != 0 {
            font = font.bold();
        }
        if style & pele::text::ITALIC != 0 {
            font = font.italic();
        }
        let font_id = self.text_system.resolve_font(&font);
        f32::from(
            self.text_system
                .layout_width(font_id, px(MERMAID_MEASURE_SIZE), character),
        ) / MERMAID_MEASURE_SIZE
    }
}

impl pele::text::TextMeasurer for AppTextMeasurer {
    fn width(&self, text: &str, size: f64, style: u8) -> f64 {
        let size = if style & (pele::text::SUB | pele::text::SUP) == 0 {
            size
        } else {
            size / 1.2
        };
        let style = style & (pele::text::BOLD | pele::text::ITALIC | pele::text::MONO);
        let mut advances = self.advances.lock();
        let em = text
            .chars()
            .filter(|character| !character.is_control())
            .map(|character| {
                *advances
                    .entry((character, style))
                    .or_insert_with(|| self.advance(character, style))
            })
            .sum::<f32>();
        f64::from(em) * size
    }
}

#[derive(Clone, Copy)]
struct MermaidThemeColors {
    background: Hsla,
    surface: Hsla,
    muted: Hsla,
    foreground: Hsla,
    muted_foreground: Hsla,
    border: Hsla,
    node: Hsla,
    node_border: Hsla,
    primary: Hsla,
    primary_foreground: Hsla,
    accent: Hsla,
    success: Hsla,
    warning: Hsla,
    danger: Hsla,
}

impl MermaidThemeColors {
    fn from_app(cx: &App) -> Self {
        Self::from_theme(cx.theme())
    }

    fn from_theme(theme: &penna::Theme) -> Self {
        let paint = penna::blocks::chat::diagram_paint(theme);
        Self {
            background: paint.ground,
            surface: paint.group,
            muted: paint.group_deep,
            foreground: theme.foreground,
            muted_foreground: theme.foreground.muted(),
            border: paint.group_edge,
            node: paint.node,
            node_border: paint.node_edge,
            primary: theme.foreground,
            primary_foreground: theme.paint(Tone::Neutral, Emphasis::Solid).text,
            accent: theme.accent.opaque(),
            success: theme.success.opaque(),
            warning: theme.warning.opaque(),
            danger: theme.danger.opaque(),
        }
    }

    fn series(&self) -> Vec<Hsla> {
        let (lightness, chroma, _) = crate::to_oklch(self.accent);
        let (low, high) = if crate::oklab_lightness(self.background) < 0.5 {
            MERMAID_SERIES_LIGHTNESS_DARK
        } else {
            MERMAID_SERIES_LIGHTNESS_LIGHT
        };
        let lightness = lightness.clamp(low, high);
        let mut hues = [self.accent, self.success, self.warning, self.danger]
            .map(|tone| {
                let (_, chroma, hue) = crate::to_oklch(tone);
                (chroma, hue)
            })
            .to_vec();
        while hues.len() < MERMAID_SERIES {
            let mut taken = hues.iter().map(|(_, hue)| *hue).collect::<Vec<_>>();
            taken.sort_by(f32::total_cmp);
            let wrap = taken
                .first()
                .zip(taken.last())
                .map(|(first, last)| (*last, first + 360.0 - last));
            let (from, gap) = taken
                .windows(2)
                .map(|pair| (pair[0], pair[1] - pair[0]))
                .chain(wrap)
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap_or((0.0, 360.0));
            hues.push((chroma, (from + gap / 2.0).rem_euclid(360.0)));
        }
        hues.into_iter()
            .map(|(chroma, hue)| oklch_in_gamut(lightness, chroma, hue))
            .collect()
    }
}

fn oklch_in_gamut(lightness: f32, mut chroma: f32, hue: f32) -> Hsla {
    loop {
        let color = crate::oklch(lightness, chroma, hue);
        let (drawn_lightness, drawn_chroma, _) = crate::to_oklch(color);
        if chroma.is_nan()
            || chroma < 0.01
            || (drawn_lightness - lightness).abs() + (drawn_chroma - chroma).abs() < 0.002
        {
            return color;
        }
        chroma *= 0.95;
    }
}

fn mermaid_font_family(font_family: &str, system: &str, generic: &str) -> String {
    let mapped = ala::font_name_with_fallbacks(font_family, system);
    let sanitized = mapped
        .chars()
        .filter(|character| !matches!(character, ';' | '{' | '}'))
        .collect::<String>();
    let sanitized = if sanitized.trim().is_empty() {
        system
    } else {
        sanitized.trim()
    };
    if sanitized
        .split(',')
        .any(|family| family.trim().eq_ignore_ascii_case(generic))
    {
        sanitized.to_owned()
    } else {
        format!("{sanitized}, {generic}")
    }
}

fn mermaid_theme_key(cx: &App) -> u64 {
    const LOOK: u32 = 8;
    let mut hasher = DefaultHasher::new();
    LOOK.hash(&mut hasher);
    let colors = MermaidThemeColors::from_app(cx);
    for color in [
        colors.background,
        colors.surface,
        colors.muted,
        colors.foreground,
        colors.muted_foreground,
        colors.border,
        colors.node,
        colors.node_border,
        colors.primary,
        colors.accent,
        colors.success,
        colors.warning,
        colors.danger,
    ] {
        color.h.to_bits().hash(&mut hasher);
        color.s.to_bits().hash(&mut hasher);
        color.l.to_bits().hash(&mut hasher);
        color.a.to_bits().hash(&mut hasher);
    }
    cx.theme().font_family.hash(&mut hasher);
    cx.theme().mono_font_family.hash(&mut hasher);
    f32::from(cx.theme().radius).to_bits().hash(&mut hasher);
    hasher.finish()
}

#[derive(Default)]
struct FigureImageCache {
    images: HashMap<u64, Arc<RenderImage>>,
    insertion_order: VecDeque<u64>,
}

impl Global for FigureImageCache {}

impl FigureImageCache {
    fn get(&self, key: u64) -> Option<Arc<RenderImage>> {
        self.images.get(&key).cloned()
    }

    fn insert(&mut self, key: u64, image: Arc<RenderImage>) {
        if self.images.contains_key(&key) {
            return;
        }
        self.images.insert(key, image);
        self.insertion_order.push_back(key);
        while self.images.len() > FIGURE_CACHE_CAPACITY {
            if let Some(expired) = self.insertion_order.pop_front() {
                self.images.remove(&expired);
            }
        }
    }
}

enum FigureRenderResult {
    Pending,
    Ready(Arc<RenderImage>),
    Failed(SharedString),
}

struct FigureRenderState {
    result: FigureRenderResult,
    render_key: Option<u64>,
    _task: Option<Task<()>>,
}

impl FigureRenderState {
    fn new(source: FigureSource, render_key: u64, cx: &mut Context<Self>) -> Self {
        let mut state = Self {
            result: FigureRenderResult::Pending,
            render_key: None,
            _task: None,
        };
        state.synchronize(source, render_key, cx);
        state
    }

    fn synchronize(&mut self, source: FigureSource, render_key: u64, cx: &mut Context<Self>) {
        if self.render_key == Some(render_key) {
            return;
        }
        self.render_key = Some(render_key);
        if let Some(image) = cx
            .try_global::<FigureImageCache>()
            .and_then(|cache| cache.get(render_key))
        {
            self.result = FigureRenderResult::Ready(image);
            self._task = None;
            cx.notify();
            return;
        }
        self.result = FigureRenderResult::Pending;
        let options = mermaid_options(cx);
        let svg_renderer = cx.svg_renderer();
        let delay = cx.background_executor().timer(FIGURE_RENDER_DEBOUNCE);
        let task = cx.spawn(async move |this: ala::WeakEntity<Self>, cx| {
            delay.await;
            let result = cx
                .background_spawn(async move {
                    let svg = match source.kind {
                        FigureKind::Mermaid => render_mermaid_svg(&source.source, &options)?,
                        FigureKind::Svg => source.source,
                    };
                    svg_renderer
                        .render_single_frame(svg.as_bytes(), source.scale / 100.0)
                        .map_err(|error| error.to_string())
                })
                .await;
            let _ = this.update(cx, |state, cx| {
                if state.render_key != Some(render_key) {
                    return;
                }
                state.result = match result {
                    Ok(image) => {
                        if cx.try_global::<FigureImageCache>().is_none() {
                            cx.set_global(FigureImageCache::default());
                        }
                        cx.global_mut::<FigureImageCache>()
                            .insert(render_key, image.clone());
                        FigureRenderResult::Ready(image)
                    }
                    Err(error) => FigureRenderResult::Failed(error.into()),
                };
                cx.notify();
            });
        });
        self._task = Some(task);
        cx.notify();
    }
}

fn render_mermaid_svg(source: &str, options: &pele::RenderOptions) -> Result<String, String> {
    pele::render(source, options)
        .map(|result| result.svg)
        .map_err(|error| {
            if error.line > 0 {
                format!(
                    "Line {}, column {}: {}",
                    error.line, error.column, error.message
                )
            } else {
                error.message
            }
        })
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn css_color(color: Hsla) -> String {
    let rgba = Rgba::from(color);
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(rgba.r),
        channel(rgba.g),
        channel(rgba.b)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ala::{Entity, Render, TestAppContext, VisualTestContext};

    struct AnswerTest {
        text: SharedString,
        owner: Option<u64>,
    }

    impl AnswerTest {
        fn new(text: &str, owner: Option<u64>) -> Self {
            Self {
                text: SharedString::from(text.to_owned()),
                owner,
            }
        }
    }

    impl Render for AnswerTest {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let scope = self.owner.unwrap_or_default();
            div().w(px(520.0)).h(px(600.0)).child(
                agent_text_view(
                    TextView::markdown("answer", self.text.clone()),
                    scope,
                    self.owner,
                    cx,
                )
                .selectable(true)
                .markdown_extensions(assistant_markdown_extensions()),
            )
        }
    }

    struct Blank;

    impl Render for Blank {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    #[test]
    fn plan_items_give_the_item_in_progress_its_own_state() {
        assert_eq!(
            plan_items("- [x] a\n- [~] b\n- [ ] c\nnot an item"),
            [
                (PlanItemState::Done, "a"),
                (PlanItemState::InProgress, "b"),
                (PlanItemState::Pending, "c"),
            ]
        );
    }

    #[ala::test]
    fn an_svg_block_renders_as_a_figure(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let answer = "Here:\n\n```svg\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"80\" height=\"40\"><rect width=\"80\" height=\"40\" fill=\"red\"/></svg>\n```\n";
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|_| AnswerTest::new(answer, None));
            penna::Root::new(view, window, cx)
        });
        let cx: &mut VisualTestContext = cx;
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        cx.executor().advance_clock(FIGURE_RENDER_DEBOUNCE);
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let figure = cx
            .debug_bounds("agent-figure-image")
            .expect("the svg block should paint as an image");
        assert_eq!(figure.size.width, px(80.0));
        assert_eq!(figure.size.height, px(40.0));
    }

    #[ala::test]
    fn a_tall_figure_draws_at_its_own_size(cx: &mut TestAppContext) {
        let cx = painted_answer(
            "```svg\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"120\" height=\"900\"><rect width=\"120\" height=\"900\" fill=\"red\"/></svg>\n```\n",
            cx,
        );
        let figure = cx
            .debug_bounds("agent-figure-image")
            .expect("the svg block should paint as an image");
        assert_eq!(figure.size.width, px(120.0));
        assert_eq!(figure.size.height, px(900.0));
    }

    #[ala::test]
    fn an_html_block_opens_as_a_page_when_the_app_can_show_one(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let opened = Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let answer = "```html\n<p>hi</p>\n```\n";
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|_| AnswerTest::new(answer, None));
            penna::Root::new(view, window, cx)
        });
        let cx: &mut VisualTestContext = cx;
        let hover_html_block = |cx: &mut VisualTestContext| {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            let language = cx
                .debug_bounds("agent-code-language")
                .expect("the html block is painted");
            cx.simulate_mouse_move(language.center(), None, ala::Modifiers::none());
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
        };
        hover_html_block(cx);
        assert!(cx.debug_bounds("agent-code-copy").is_some());
        assert!(
            cx.debug_bounds("agent-code-open").is_none(),
            "no opener, no button"
        );

        let sink = Rc::clone(&opened);
        cx.update(|_, cx| {
            cx.set_global(AgentHtmlOpener(Rc::new(move |html, _, _| {
                sink.borrow_mut().push(html);
            })));
            cx.refresh_windows();
        });
        hover_html_block(cx);
        let open = cx
            .debug_bounds("agent-code-open")
            .expect("an html block offers Open");
        cx.simulate_click(open.center(), ala::Modifiers::none());
        cx.run_until_parked();

        assert_eq!(opened.borrow().as_slice(), ["<p>hi</p>"]);
    }

    #[ala::test]
    fn an_html_block_previews_as_a_picture_when_toggled(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let asked = Rc::new(std::cell::RefCell::new(
            Vec::<(String, HtmlPreviewSize)>::new(),
        ));
        let answer = "```html\n<p>hi</p>\n```\n";
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|_| AnswerTest::new(answer, None));
            penna::Root::new(view, window, cx)
        });
        let cx: &mut VisualTestContext = cx;
        let sink = Rc::clone(&asked);
        cx.update(|_, cx| {
            cx.set_global(AgentHtmlPreviewer(Rc::new(move |html, size, cx| {
                sink.borrow_mut().push((html, size));
                let picture = cx.svg_renderer().render_single_frame(
                    br#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"/>"#,
                    1.0,
                );
                Task::ready(picture.map_err(|error| error.to_string().into()))
            })));
        });
        let draw = |cx: &mut VisualTestContext| {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
        };
        draw(cx);
        let language = cx
            .debug_bounds("agent-code-language")
            .expect("the html block is painted");
        cx.simulate_mouse_move(language.center(), None, ala::Modifiers::none());
        draw(cx);
        let toggle = cx
            .debug_bounds("agent-code-preview")
            .expect("an html block offers Preview");
        assert!(cx.debug_bounds("agent-html-preview").is_none());
        cx.simulate_click(toggle.center(), ala::Modifiers::none());
        draw(cx);
        draw(cx);
        cx.executor().advance_clock(FIGURE_RENDER_DEBOUNCE);
        cx.run_until_parked();
        draw(cx);

        let asked = asked.borrow();
        let (html, size) = asked.last().expect("the previewer was asked");
        assert_eq!(html, "<p>hi</p>");
        assert_eq!(f32::from(size.width) % HTML_PREVIEW_WIDTH_STEP, 0.0);
        let picture = cx
            .debug_bounds("agent-html-preview")
            .expect("the preview is painted");
        assert!(picture.size.width > px(0.0));
    }

    #[ala::test]
    fn a_preview_block_shows_as_a_themed_picture_without_a_toggle(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let asked = Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let answer = "```html preview\n<p id=\"hi\">hi</p>\n```\n";
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|_| AnswerTest::new(answer, None));
            penna::Root::new(view, window, cx)
        });
        let cx: &mut VisualTestContext = cx;
        let sink = Rc::clone(&asked);
        cx.update(|_, cx| {
            cx.set_global(AgentHtmlPreviewer(Rc::new(move |html, _, cx| {
                sink.borrow_mut().push(html);
                let picture = cx.svg_renderer().render_single_frame(
                    br#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"/>"#,
                    1.0,
                );
                Task::ready(picture.map_err(|error| error.to_string().into()))
            })));
        });
        let draw = |cx: &mut VisualTestContext| {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
        };
        draw(cx);
        draw(cx);
        cx.executor().advance_clock(FIGURE_RENDER_DEBOUNCE);
        cx.run_until_parked();
        draw(cx);

        let asked = asked.borrow();
        let html = asked.last().expect("the previewer was asked");
        assert!(html.starts_with("<!doctype html>"), "{html}");
        assert!(html.contains("--background:#"), "{html}");
        assert!(html.contains("window.openai="), "{html}");
        assert!(
            html.ends_with("<body><p id=\"hi\">hi</p></body></html>"),
            "{html}"
        );
        assert!(cx.debug_bounds("agent-html-preview").is_some());
    }

    #[test]
    fn whole_pages_skip_the_visual_wrapper() {
        let page = "<!DOCTYPE html><title>x</title>";
        assert_eq!(html_block_page(page, Some("<head>")), page);
        assert_eq!(
            html_block_page("  <HTML></HTML>", Some("<head>")),
            "  <HTML></HTML>"
        );
        assert_eq!(
            html_block_page("<p>x</p>", Some("<head>")),
            "<head><p>x</p></body></html>"
        );
        assert_eq!(html_block_page("<p>x</p>", None), "<p>x</p>");
    }

    #[ala::test]
    fn a_previewed_html_block_goes_live_on_click(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let live_block = Rc::new(std::cell::Cell::new(None::<u64>));
        let answer = "```html\n<p>live</p>\n```\n";
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|_| AnswerTest::new(answer, Some(7)));
            penna::Root::new(view, window, cx)
        });
        let cx: &mut VisualTestContext = cx;
        let shown = Rc::clone(&live_block);
        let started = Rc::clone(&live_block);
        cx.update(|_, cx| {
            cx.set_global(AgentHtmlPreviewer(Rc::new(|_, _, cx| {
                Task::ready(
                    cx.svg_renderer()
                        .render_single_frame(
                            br#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"/>"#,
                            1.0,
                        )
                        .map_err(|error| error.to_string().into()),
                )
            })));
            cx.set_global(AgentHtmlLive {
                live: Rc::new(move |block, _| {
                    (shown.get() == Some(block)).then(|| {
                        div()
                            .debug_selector(|| "test-live-page".to_owned())
                            .size_full()
                            .into_any_element()
                    })
                }),
                activate: Rc::new(move |live, window, _| {
                    assert_eq!(live.html, "<p>live</p>");
                    assert_eq!(live.owner, Some(7));
                    started.set(Some(live.block));
                    window.refresh();
                }),
            });
        });
        let draw = |cx: &mut VisualTestContext| {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
        };
        draw(cx);
        let language = cx
            .debug_bounds("agent-code-language")
            .expect("the html block is painted");
        cx.simulate_mouse_move(language.center(), None, ala::Modifiers::none());
        draw(cx);
        let toggle = cx
            .debug_bounds("agent-code-preview")
            .expect("an html block offers Preview");
        cx.simulate_click(toggle.center(), ala::Modifiers::none());
        draw(cx);
        draw(cx);
        cx.executor().advance_clock(FIGURE_RENDER_DEBOUNCE);
        cx.run_until_parked();
        draw(cx);
        let picture = cx
            .debug_bounds("agent-html-preview")
            .expect("the preview is painted");
        assert!(cx.debug_bounds("test-live-page").is_none());

        cx.simulate_click(picture.center(), ala::Modifiers::none());
        cx.run_until_parked();
        draw(cx);

        assert!(live_block.get().is_some(), "the click asks to go live");
        let live = cx
            .debug_bounds("test-live-page")
            .expect("the live page takes the picture's place");
        assert_eq!(live.size, picture.size);
        assert!(cx.debug_bounds("agent-html-preview").is_none());
    }

    #[ala::test]
    fn identical_html_blocks_keep_separate_live_owners_and_occurrences(cx: &mut TestAppContext) {
        struct Panes {
            first: Entity<AnswerTest>,
            second: Entity<AnswerTest>,
        }

        impl Render for Panes {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                h_flex().children([self.first.clone(), self.second.clone()])
            }
        }

        cx.update(crate::init);
        let answer = "```html preview\n<p>same</p>\n```\n";
        let repeated = format!("{answer}\n{answer}");
        let (_, cx) = cx.add_window_view(|window, cx| {
            let first = cx.new(|_| AnswerTest::new(&repeated, Some(7)));
            let second = cx.new(|_| AnswerTest::new(answer, Some(8)));
            let view = cx.new(|_| Panes { first, second });
            penna::Root::new(view, window, cx)
        });
        let cx: &mut VisualTestContext = cx;
        let active = Rc::new(std::cell::Cell::new(None::<u64>));
        let visited = Rc::new(std::cell::RefCell::new(Vec::new()));
        let activated = Rc::new(std::cell::RefCell::new(Vec::new()));
        let rendered_live = Rc::new(std::cell::Cell::new(0));
        let shown = Rc::clone(&active);
        let visits = Rc::clone(&visited);
        let rendered = Rc::clone(&rendered_live);
        let started = Rc::clone(&active);
        let activations = Rc::clone(&activated);
        cx.update(|_, cx| {
            cx.set_global(AgentHtmlPreviewer(Rc::new(|_, _, cx| {
                Task::ready(
                    cx.svg_renderer()
                        .render_single_frame(
                            br#"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40"/>"#,
                            1.0,
                        )
                        .map_err(|error| error.to_string().into()),
                )
            })));
            cx.set_global(AgentHtmlLive {
                live: Rc::new(move |block, _| {
                    visits.borrow_mut().push(block);
                    (shown.get() == Some(block)).then(|| {
                        rendered.set(rendered.get() + 1);
                        div().size_full().into_any_element()
                    })
                }),
                activate: Rc::new(move |live, window, _| {
                    activations.borrow_mut().push((live.block, live.owner));
                    started.set(Some(live.block));
                    window.refresh();
                }),
            });
        });
        let draw = |cx: &mut VisualTestContext| {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
        };
        draw(cx);
        draw(cx);
        cx.executor().advance_clock(FIGURE_RENDER_DEBOUNCE);
        cx.run_until_parked();
        draw(cx);
        assert_eq!(
            visited
                .borrow()
                .iter()
                .copied()
                .collect::<HashSet<_>>()
                .len(),
            3
        );
        for owner in [8, 7] {
            let preview = cx
                .debug_bounds("agent-html-preview")
                .expect("another occurrence still shows its own preview");
            cx.simulate_click(preview.center(), ala::Modifiers::none());
            cx.run_until_parked();
            rendered_live.set(0);
            draw(cx);
            assert_eq!(rendered_live.get(), 1);
            assert_eq!(activated.borrow().last().unwrap().1, Some(owner));
        }
        let activated = activated.borrow();
        assert_ne!(activated[0].0, activated[1].0);
    }

    #[test]
    fn transcript_disclosures_point_right_when_closed_and_up_when_open() {
        assert_eq!(disclosure_icon(false), IconName::ChevronRight);
        assert_eq!(disclosure_icon(true), IconName::ChevronUp);
    }

    #[ala::test]
    fn transcript_affordances_use_the_muted_foreground(cx: &mut TestAppContext) {
        cx.update(crate::init);
        cx.update(|cx| {
            assert_eq!(activity_color(cx), cx.theme().foreground.muted());
        });
    }

    #[ala::test]
    fn inline_code_fills_paint_after_the_text_lays_out(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let answer = "A tiny bot named `cronkitty` had a single mission: `sleep(5)`.";
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|_| AnswerTest::new(answer, None));
            penna::Root::new(view, window, cx)
        });
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    }

    fn test_color([r, g, b]: [u8; 3]) -> Hsla {
        Rgba {
            r: f32::from(r) / 255.0,
            g: f32::from(g) / 255.0,
            b: f32::from(b) / 255.0,
            a: 1.0,
        }
        .into()
    }

    fn test_mermaid_options() -> pele::RenderOptions {
        mermaid_render_options(
            MermaidThemeColors {
                background: test_color([0x10, 0x11, 0x12]),
                surface: test_color([0x19, 0x1a, 0x1d]),
                muted: test_color([0x24, 0x26, 0x2b]),
                foreground: test_color([0xe8, 0xe9, 0xed]),
                muted_foreground: test_color([0x97, 0x9b, 0xa6]),
                border: test_color([0x38, 0x3a, 0x40]),
                node: test_color([0x1f, 0x1c, 0x2e]),
                node_border: test_color([0x55, 0x3b, 0x8f]),
                primary: test_color([0xe8, 0xe9, 0xed]),
                primary_foreground: test_color([0x10, 0x11, 0x12]),
                accent: test_color([0x8b, 0x5c, 0xf6]),
                success: test_color([0x2f, 0x85, 0x5a]),
                warning: test_color([0xb7, 0x79, 0x1f]),
                danger: test_color([0xc5, 0x30, 0x30]),
            },
            [".SystemUIFont", ".ZedMono"],
            6.0,
            None,
        )
    }

    const MERMAID_FAMILIES: [(&str, &str); 8] = [
        (
            "flowchart",
            "flowchart TD\n    Start([Prompt]) --> Agent[Agent loop]\n    Agent --> Tool{Use a tool?}\n    Tool -->|Yes| Result[Read result]\n    Tool -->|No| Done([Answer])",
        ),
        (
            "sequence",
            "sequenceDiagram\n    participant User\n    participant Agent\n    participant Tool\n    User->>Agent: Fix the rendering\n    Agent->>Tool: Render fixture\n    Tool-->>Agent: SVG\n    Agent-->>User: Verified result",
        ),
        (
            "class",
            "classDiagram\n    class Agent {\n        +String model\n        +run() Result\n    }\n    class Tool {\n        +String name\n        +execute() Output\n    }\n    Agent --> Tool : invokes",
        ),
        (
            "state",
            "stateDiagram-v2\n    [*] --> Idle\n    Idle --> Rendering\n    Rendering --> Ready\n    Rendering --> Failed\n    Ready --> [*]",
        ),
        (
            "er",
            "erDiagram\n    USER ||--o{ SESSION : owns\n    SESSION ||--o{ MESSAGE : contains\n    USER {\n        string id PK\n        string email UK\n    }\n    SESSION {\n        string id PK\n        datetime created_at\n    }",
        ),
        (
            "gantt",
            "gantt\n    title Project Timeline\n    dateFormat YYYY-MM-DD\n    section Design\n        Wireframes :done, design, 2026-07-01, 3d\n    section Build\n        Core Agent Loop :active, core, 2026-07-05, 5d\n        Tool Integration :tools, after core, 4d\n    section Ship\n        Testing :crit, test, after tools, 3d",
        ),
        (
            "pie",
            "pie showData\n    title Tool Usage Breakdown\n    \"Read\" : 42\n    \"Edit\" : 28\n    \"Execute\" : 18\n    \"Search\" : 12",
        ),
        (
            "quadrant",
            "quadrantChart\n    title Rendering quality\n    x-axis Clipped --> Fits\n    y-axis Low contrast --> Readable\n    ER: [0.8, 0.85]\n    Gantt: [0.75, 0.8]\n    Pie: [0.9, 0.9]",
        ),
    ];

    #[test]
    fn mermaid_options_carry_the_app_theme() {
        let options = test_mermaid_options();
        assert!(options.transparent, "the card shows through the canvas");
        assert!((options.font_size - 13.0).abs() < f64::EPSILON);
        assert_eq!(options.font_family, "system-ui, sans-serif");
        assert_eq!(options.font_family_mono, "Lilex, monospace");
        assert_eq!(options.max_width, Some(MERMAID_LAYOUT_WIDTH));
        let theme = &options.theme;
        assert_eq!(theme.background, "#101112");
        assert_eq!(theme.surface, "#191a1d");
        assert_eq!(theme.surface_alt, "#24262b");
        assert_eq!(theme.foreground, "#e8e9ed");
        assert_eq!(theme.muted, "#979ba6");
        assert_eq!(theme.line, "#979ba6");
        assert_eq!(theme.border, "#383a40");
        assert_eq!(theme.node.as_deref(), Some("#1f1c2e"));
        assert_eq!(theme.node_border.as_deref(), Some("#553b8f"));
        assert_eq!(theme.accent, theme.series[0]);
        assert_eq!(
            theme.series,
            [
                "#9f7cff", "#59ac7f", "#c98934", "#f65f58", "#1aa7d2", "#ea53c3", "#98a201",
                "#5396ff"
            ]
        );
        assert!((theme.radius - 6.0).abs() < f64::EPSILON);
    }

    fn builtin_pele_theme(id: &str, mode: crate::ThemeMode) -> pele::Theme {
        let mut theme = penna::Theme::from(&*penna::ThemeColor::for_mode(mode));
        theme.mode = mode;
        crate::interface_theme::builtin_theme(id)
            .expect("a built-in theme")
            .apply(&mut theme);
        mermaid_render_options(
            MermaidThemeColors::from_theme(&theme),
            ["", ""],
            f64::from(penna::blocks::chat::diagram_paint(&theme).radius),
            None,
        )
        .theme
    }

    #[test]
    fn the_macos_theme_paints_diagrams_in_its_own_colors() {
        let theme = builtin_pele_theme("macos", crate::ThemeMode::Light);
        assert_eq!(
            [
                &theme.background,
                &theme.foreground,
                &theme.muted,
                &theme.line,
                &theme.surface,
                &theme.surface_alt,
                &theme.border,
                &theme.accent,
            ],
            [
                "#f6f6f3", "#1c1c1a", "#575756", "#575756", "#ecece9", "#e4e4e2", "#90908e",
                "#0a64dd"
            ]
        );
        assert_eq!(theme.node.as_deref(), Some("#dae4f0"));
        assert_eq!(theme.node_border.as_deref(), Some("#74a6e7"));
        assert_eq!(
            theme.series,
            [
                "#0a64dd", "#118144", "#a55200", "#c51e32", "#9d36b1", "#0e7b85", "#786e0c",
                "#ba2179"
            ]
        );
    }

    #[test]
    fn every_supported_mermaid_family_draws_in_theme_colors_on_a_transparent_canvas() {
        let options = test_mermaid_options();
        let renderer = ala::SvgRenderer::new(Arc::new(()));
        let snapshot_directory = std::env::var_os("ZZ_MERMAID_SNAPSHOT_DIR");
        if let Some(directory) = &snapshot_directory {
            std::fs::create_dir_all(directory).expect("create Mermaid snapshot directory");
        }

        for (name, source) in MERMAID_FAMILIES {
            assert!(pele::supports(source), "{name} should be supported");
            let svg = render_mermaid_svg(source, &options)
                .unwrap_or_else(|error| panic!("{name} fixture should render: {error}"));
            assert!(
                !svg.contains("height=\"100%\""),
                "{name} paints a canvas, which shows as a box on glass"
            );
            assert!(!svg.contains("<foreignObject"), "{name} was not resvg-safe");
            assert!(
                svg.contains("font-family=\"system-ui, sans-serif\" font-size=\"13\""),
                "{name} should use the app font at 13px"
            );
            assert!(
                svg.contains("fill=\"#e8e9ed\""),
                "{name} should draw text in the foreground"
            );
            renderer
                .render_single_frame(svg.as_bytes(), 1.0)
                .unwrap_or_else(|error| panic!("{name} should rasterize: {error}"));

            if let Some(directory) = &snapshot_directory {
                std::fs::write(
                    std::path::Path::new(directory).join(format!("{name}.svg")),
                    svg,
                )
                .expect("write Mermaid snapshot");
            }
        }
    }

    #[test]
    fn mermaid_tells_unsupported_types_from_broken_sources() {
        for source in [
            "mindmap\n  root((zz))\n    panes",
            "timeline\n    2026 : the swap",
            "gitGraph\n    commit\n    branch fix",
            "journey\n    title Rendering\n    section Fix\n      Run: 5: Agent",
            "xychart-beta\n    x-axis [a, b]\n    bar [3, 9]",
        ] {
            assert!(!pele::supports(source), "{source} is not a supported type");
        }
        let broken = "flowchart LR\n    Prompt[ask the agent --> Answer";
        assert!(pele::supports(broken));
        let error = render_mermaid_svg(broken, &test_mermaid_options())
            .expect_err("an unterminated label should not render");
        assert!(error.starts_with("Line 2, column "), "{error}");
    }

    #[ala::test]
    fn mermaid_labels_are_measured_by_the_app_text_system(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let options = cx.update(|cx| mermaid_options(cx));
        let measurer = options
            .measurer
            .as_deref()
            .expect("mermaid labels should be measured by the app");
        assert!(
            (measurer.width("iiii", 13.0, 0) - measurer.width("WWWW", 13.0, 0)).abs()
                < f64::EPSILON,
            "the test text system gives every glyph the same advance"
        );
        assert!(pele::text::measure("iiii", 13.0, 0) < pele::text::measure("WWWW", 13.0, 0));
        let width = |label: &str| {
            pele::render(&format!("flowchart LR\n    A[{label}]"), &options)
                .expect("a one node flowchart renders")
                .width
        };
        assert!((width("iiiiiiii") - width("WWWWWWWW")).abs() < f64::EPSILON);
    }

    fn painted_answer<'a>(answer: &str, cx: &'a mut TestAppContext) -> &'a mut VisualTestContext {
        cx.update(crate::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|_| AnswerTest::new(answer, None));
            penna::Root::new(view, window, cx)
        });
        let cx: &mut VisualTestContext = cx;
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        cx.executor().advance_clock(FIGURE_RENDER_DEBOUNCE);
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        cx
    }

    #[ala::test]
    fn a_supported_mermaid_block_renders_as_a_figure(cx: &mut TestAppContext) {
        let cx = painted_answer("```mermaid\nflowchart LR\n    A --> B\n```\n", cx);
        assert!(cx.debug_bounds("agent-figure-image").is_some());
        assert!(cx.debug_bounds("agent-code-language").is_none());
    }

    #[ala::test]
    fn an_unsupported_mermaid_block_stays_a_code_block(cx: &mut TestAppContext) {
        let cx = painted_answer("```mermaid\nmindmap\n  root((zz))\n    panes\n```\n", cx);
        assert!(cx.debug_bounds("agent-figure-image").is_none());
        assert!(cx.debug_bounds("agent-code-language").is_some());
    }

    #[ala::test]
    fn nowrap_shaping_still_breaks_on_an_embedded_newline(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, _| Blank);
        let label = SharedString::from("git branch --show-current\ngit log --oneline -12");

        let (raw_lines, collapsed_lines) = cx.update(|window, _| {
            let shape = |text: SharedString| {
                let run = window.text_style().to_run(text.len());
                window
                    .text_system()
                    .shape_text(text, px(13.0), &[run], None, None)
                    .expect("label should shape")
                    .len()
            };
            (shape(label.clone()), shape(single_line(label.clone())))
        });

        assert_eq!(raw_lines, 2, "nowrap does not join lines split by `\\n`");
        assert_eq!(collapsed_lines, 1);
    }

    #[test]
    fn markdown_fence_language_matching_is_case_insensitive() {
        assert!(is_markdown_language("markdown"));
        assert!(is_markdown_language("MARKDOWN"));
        assert!(is_markdown_language("md"));
        assert!(!is_markdown_language("rust"));
    }

    #[test]
    fn code_block_header_uses_the_fence_language_or_text() {
        assert_eq!(code_block_language(Some("rust".into())), "rust");
        assert_eq!(code_block_language(Some("".into())), "text");
        assert_eq!(code_block_language(None), "text");
    }
}
