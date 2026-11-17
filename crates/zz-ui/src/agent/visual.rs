use std::{
    fmt::Write as _,
    hash::{DefaultHasher, Hash, Hasher},
    rc::Rc,
    sync::Arc,
};

use ala::{
    AnyElement, App, ClipboardItem, Global, Hsla, ImageSource, IntoElement, ObjectFit, Pixels,
    SharedString, Window, canvas, div, img, prelude::*, px,
};
use penna::{
    Card, CardLook, TextStyle,
    blocks::chat::message_action,
    foundation::{Kind, SurfaceRole, roles, theme_css},
};
use zz_protocol::ViewContent;

use super::{
    AgentHtmlFrames, AgentHtmlLive, HtmlLiveBlock, HtmlPreviewResult, HtmlPreviewSize,
    HtmlPreviewState, HtmlPreviewToggles, MermaidThemeColors, VISUAL_PAGE_SCRIPT,
    agent_markdown_style, css_color, html_frame, html_opener, html_preview_width, is_html_document,
    mermaid_font_family, mermaid_theme_key,
    preview::{ViewOpener, ViewRequest},
};
use crate::{
    ActiveTheme as _, Colorize as _, Icon, IconName, Sizable as _, StyledExt as _,
    button::Button,
    h_flex,
    spinner::Spinner,
    text::{
        MarkdownNode, MarkdownParseContext, MarkdownPlugin, TextView, TextViewState, markdown_ast,
    },
    v_flex,
};

pub(crate) const VISUAL_NODE_NAME: &str = "zz-visual";
pub(crate) const PAGE_NODE_NAME: &str = "zz-page";
const MAX_SOURCE_BYTES: usize = 1024 * 1024;
const PAGE_WIDTH: f32 = 1280.0;
const PAGE_HEIGHT: f32 = 720.0;
const STRIP_HEIGHT: f32 = 32.0;
const DRAWING_HEIGHT: f32 = 148.0;
const FAILED_HEIGHT: f32 = 96.0;
const VISUAL_GROUP: &str = "agent-visual";

pub const PREVIEW_HEIGHT_SCRIPT: &str = "(() => { const body = document.body; \
     if (!body) return document.documentElement.scrollHeight; \
     const style = getComputedStyle(body); \
     let bottom = body.getBoundingClientRect().bottom; \
     if (document.compatMode === 'BackCompat') { const range = document.createRange(); \
     range.selectNodeContents(body); \
     bottom = range.getBoundingClientRect().bottom + (parseFloat(style.paddingBottom) || 0); } \
     return Math.ceil(bottom + (parseFloat(style.marginBottom) || 0) + window.scrollY); })()";

pub fn straighten_alpha(bgra: &mut [u8]) {
    for pixel in bgra.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        if alpha == 0 || alpha == 255 {
            continue;
        }
        for channel in &mut pixel[..3] {
            *channel = (u32::from(*channel) * 255 / alpha).min(255) as u8;
        }
    }
}

const KIT_CSS: [&str; 6] = [
    include_str!("../../../../libs/penna/web/penna.css"),
    include_str!("../../../../libs/penna/src/components/button/button.css"),
    include_str!("../../../../libs/penna/src/components/card/card.css"),
    include_str!("../../../../libs/penna/src/components/badge/badge.css"),
    include_str!("../../../../libs/penna/src/components/progress/progress.css"),
    include_str!("../../../../libs/penna/src/components/segmented/segmented.css"),
];

const VISUAL_CSS: &str = r"html{background:transparent}
button,input,select{color:inherit}
body{margin:0;padding:0;display:flow-root}
h1,h2,h3{font-weight:600;line-height:1.25;margin:0 0 .5em}
h1{font-size:20px}h2{font-size:16px}h3{font-size:14px}
p{margin:0 0 .75em}
a{color:var(--penna-accent)}
hr{border:0;border-top:1px solid var(--penna-divider);margin:12px 0}
table{border-collapse:collapse;width:100%}
th,td{padding:4px 16px 4px 0;border-bottom:1px solid var(--penna-divider);text-align:left}
th{font-weight:400;color:var(--penna-fg-muted)}
.table-responsive{overflow-x:auto}
.text-small{font-size:var(--penna-text-small-size);line-height:var(--penna-text-small-line)}
.text-muted{color:var(--penna-fg-muted)}
.tabular-nums{font-variant-numeric:tabular-nums}
.card.penna-card[data-penna]{padding:12px}
.viz-grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:8px}
.viz-row,.viz-controls{display:flex;flex-wrap:wrap;align-items:center;gap:8px}
.viz-stat-value{font-size:22px;line-height:28px;font-weight:600;font-variant-numeric:tabular-nums}
.viz-tile{width:100%;height:100%}
.form-label{display:flex;flex-direction:column;gap:4px;font-size:var(--penna-text-small-size);color:var(--penna-fg-muted)}
.form-control,.form-select{font:inherit;height:var(--penna-control-md);padding:0 12px;outline:none}
.form-range{-webkit-appearance:none;appearance:none;height:var(--penna-control-sm);margin:0;background:none;cursor:var(--penna-click-cursor)}
.form-range::-webkit-slider-runnable-track{height:4px;border-radius:9999px;background:linear-gradient(var(--penna-accent),var(--penna-accent)) 0/var(--zz-range,50%) 100% no-repeat,var(--penna-control)}
.form-range::-webkit-slider-thumb{-webkit-appearance:none;width:14px;height:14px;margin-top:-5px;border-radius:50%;background:var(--penna-knob);box-shadow:var(--penna-shadow-contact)}
.nav-pills{display:flex}
.nav-justified>.nav-link{flex:1}
.progress.penna-progress>.progress-bar{position:static;height:100%}
.tooltip{position:absolute;pointer-events:none;background:var(--penna-popover);color:var(--penna-fg);border-radius:8px;padding:4px 8px;font-size:var(--penna-text-small-size);box-shadow:var(--penna-shadow-float)}
.viz-dotted-background{background-image:radial-gradient(var(--penna-divider) 1px,transparent 1px);background-size:16px 16px}
";

const DRESS_SCRIPT: &str = r#"(()=>{
const has=(e,c)=>e.classList.contains(c);
const cls=(e,c)=>{if(!has(e,c))e.classList.add(c)};
const attr=(e,n,v)=>{if(v===null){if(e.hasAttribute(n))e.removeAttribute(n)}else if(e.getAttribute(n)!==v)e.setAttribute(n,v)};
const marked=(e)=>has(e,"active")||has(e,"is-selected")||e.getAttribute("aria-pressed")==="true";
const on=(e)=>marked(e)||e.getAttribute("aria-selected")==="true";
const reach=(n)=>1-2**(-1/n);
const round=(e)=>{
  const s=getComputedStyle(e);const r=parseFloat(s.getPropertyValue("--penna-r"))||0;
  if(r<=0||r>=9999)return r>=9999?Math.min(e.offsetWidth,e.offsetHeight)/2:0;
  const w=e.offsetWidth,h=e.offsetHeight;if(!w||!h)return 0;
  const share=parseFloat(s.getPropertyValue("--penna-share"))||0;const shorter=Math.min(w,h);
  const cap=shorter*0.45;let corner=share>0?Math.min(share,0.5)*shorter:(cap>0?cap*Math.tanh(r/cap):0);
  const k=parseFloat(s.getPropertyValue("--penna-k"))||1;const exponent=2**k;let box=corner,shape=k;
  if(exponent>2&&corner>0){const circle=reach(2);const limit=shorter/2-0.05;let grow=circle/reach(exponent);let eased=exponent;
    if(corner*grow>limit){grow=Math.max(limit/corner,1);eased=-1/Math.log2(1-circle/grow)}
    box=Math.min(corner*grow,shorter/2);shape=Math.log2(eased)}
  e.style.borderRadius=box.toFixed(2)+"px";e.style.setProperty("corner-shape","superellipse("+shape.toFixed(3)+")");
  return corner};
const pills=(nav)=>{
  let wrap=nav.parentElement;
  if(!wrap||!has(wrap,"penna-segmented")){wrap=document.createElement("div");wrap.className="penna-segmented";
    wrap.setAttribute("data-penna","field");wrap.setAttribute("data-size","small");wrap.setAttribute("data-still","");
    if(has(nav,"nav-justified"))wrap.setAttribute("data-width","full");
    nav.replaceWith(wrap);wrap.append(nav);cls(nav,"penna-segments");attr(nav,"data-penna","button.quiet");attr(nav,"data-state","current");
    const pill=document.createElement("span");pill.className="penna-segment-pill";nav.prepend(pill)}
  let chosen=null;const links=[...nav.querySelectorAll(".nav-link")];
  if(links.some(marked))nav.zzMarked=true;
  links.forEach((link)=>{cls(link,"penna-segment");const picked=nav.zzMarked?marked(link):on(link);attr(link,"aria-selected",String(picked));if(picked&&!chosen)chosen=link});
  const pill=nav.querySelector(".penna-segment-pill");const outer=round(wrap);
  if(pill&&chosen){pill.style.setProperty("--pill-x",chosen.offsetLeft+"px");pill.style.setProperty("--pill-w",chosen.offsetWidth+"px");
    pill.style.setProperty("--penna-pill-r",Math.max(outer-2,0)+"px");pill.hidden=false}else if(pill){pill.hidden=true}};
const range=(input)=>{const min=parseFloat(input.min)||0;const max=input.max===""?100:parseFloat(input.max);
  const share=max>min?(parseFloat(input.value)-min)/(max-min):0;input.style.setProperty("--zz-range",(share*100).toFixed(2)+"%")};
const dress=()=>{
  document.querySelectorAll(".btn").forEach((e)=>{cls(e,"penna-button");attr(e,"data-penna","button");
    const primary=has(e,"btn-primary")||on(e);attr(e,"data-prominent",primary?"":null);attr(e,"data-tone",primary?"primary":null)});
  document.querySelectorAll(".card").forEach((e)=>{cls(e,"penna-card");attr(e,"data-penna","card.activity")});
  document.querySelectorAll(".viz-badge").forEach((e)=>{cls(e,"penna-badge");attr(e,"data-penna","badge")});
  document.querySelectorAll(".form-control,.form-select").forEach((e)=>attr(e,"data-penna","field"));
  document.querySelectorAll(".progress").forEach((e)=>{cls(e,"penna-progress");attr(e,"data-size","small");
    e.querySelectorAll(".progress-bar").forEach((bar)=>cls(bar,"penna-progress-fill"))});
  document.querySelectorAll(".form-range").forEach(range);
  document.querySelectorAll("[data-penna]").forEach(round);
  document.querySelectorAll(".nav-pills").forEach(pills)};
let queued=false;
const later=()=>{if(queued)return;queued=true;requestAnimationFrame(()=>{queued=false;dress()})};
const start=()=>{dress();
  new MutationObserver(later).observe(document.body,{subtree:true,childList:true,attributes:true,attributeFilter:["class","aria-pressed","aria-selected"]});
  document.addEventListener("input",(event)=>{if(event.target.matches&&event.target.matches(".form-range"))range(event.target)},true);
  addEventListener("resize",later)};
if(document.readyState==="loading")document.addEventListener("DOMContentLoaded",start);else start();
})();"#;

#[derive(Default)]
struct VisualKit {
    key: u64,
    head: Option<Arc<str>>,
}

impl Global for VisualKit {}

fn visual_roles() -> [SurfaceRole; 5] {
    [
        Kind::Button.role(),
        SurfaceRole::QUIET_BUTTON,
        Kind::Field.role(),
        SurfaceRole::BADGE,
        roles::CARD_ACTIVITY,
    ]
}

pub(crate) fn visual_head(cx: &mut App) -> Arc<str> {
    let key = mermaid_theme_key(cx);
    if let Some(kit) = cx.try_global::<VisualKit>()
        && kit.key == key
        && let Some(head) = &kit.head
    {
        return Arc::clone(head);
    }
    let head: Arc<str> = build_head(cx).into();
    cx.set_global(VisualKit {
        key,
        head: Some(Arc::clone(&head)),
    });
    head
}

fn build_head(cx: &App) -> String {
    let colors = MermaidThemeColors::from_app(cx);
    let dark = cx.theme().is_dark();
    let activity = cx.theme().ground_of(CardLook::Activity).fill;
    let variables = [
        ("background", "transparent".to_owned()),
        ("foreground", "var(--penna-fg)".to_owned()),
        ("card", css_color(activity)),
        ("card-foreground", "var(--penna-fg)".to_owned()),
        ("popover", "var(--penna-popover)".to_owned()),
        ("popover-foreground", "var(--penna-fg)".to_owned()),
        ("primary", css_color(colors.primary)),
        ("primary-foreground", css_color(colors.primary_foreground)),
        ("secondary", css_color(activity)),
        ("secondary-foreground", "var(--penna-fg)".to_owned()),
        ("muted", css_color(activity)),
        ("muted-foreground", "var(--penna-fg-muted)".to_owned()),
        ("accent", css_color(activity)),
        ("accent-foreground", "var(--penna-fg)".to_owned()),
        ("destructive", "var(--penna-danger)".to_owned()),
        ("border", "var(--penna-divider)".to_owned()),
        ("input", "var(--penna-divider)".to_owned()),
        ("ring", "var(--penna-fg-muted)".to_owned()),
        ("red", "var(--penna-danger)".to_owned()),
        ("green", "var(--penna-success)".to_owned()),
        ("yellow", "var(--penna-warning)".to_owned()),
        ("blue", "var(--penna-accent)".to_owned()),
        (
            "orange",
            "color-mix(in oklch,var(--penna-warning),var(--penna-danger))".to_owned(),
        ),
        (
            "purple",
            "color-mix(in oklch,var(--penna-accent),var(--penna-danger))".to_owned(),
        ),
        ("viz-series-1", "var(--penna-accent)".to_owned()),
        ("viz-series-2", "var(--penna-success)".to_owned()),
        ("viz-series-3", "var(--penna-warning)".to_owned()),
        ("viz-series-4", "var(--penna-danger)".to_owned()),
        ("viz-series-5", "var(--purple)".to_owned()),
        (
            "viz-series-6",
            "color-mix(in oklch,var(--penna-accent),var(--penna-success))".to_owned(),
        ),
    ];
    let root = variables
        .iter()
        .fold(String::new(), |mut root, (name, value)| {
            let _ = write!(root, "--{name}:{value};");
            root
        });
    let mode = if dark { "dark" } else { "light" };
    let font = mermaid_font_family(cx.theme().font_family.as_ref(), "system-ui", "sans-serif");
    let tokens = theme_css::roles_stylesheet(cx.theme(), &visual_roles());
    let kit = KIT_CSS.concat();
    format!(
        "<!doctype html><html data-penna-mode=\"{mode}\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><style>{tokens}{kit}:root{{color-scheme:{mode};--font-size-base:13px;{root}}}html{{font-family:{font}}}{VISUAL_CSS}</style><script>{VISUAL_PAGE_SCRIPT}{DRESS_SCRIPT}</script></head><body>"
    )
}

pub(crate) fn visual_page(code: &str, cx: &mut App) -> String {
    if is_html_document(code) {
        return code.to_owned();
    }
    format!("{}{code}</body></html>", visual_head(cx))
}

#[derive(Clone)]
struct HtmlSource {
    html: String,
    offset: usize,
}

fn html_fence(
    node: &markdown_ast::AstNode<'_>,
    cx: &MarkdownParseContext<'_>,
) -> Option<(HtmlSource, bool, String)> {
    let code = cx.code_fence(node)?;
    if !code
        .lang
        .as_deref()
        .is_some_and(|language| language.trim().eq_ignore_ascii_case("html"))
        || code.value.len() > MAX_SOURCE_BYTES
    {
        return None;
    }
    let preview = code.meta.as_deref().is_some_and(|meta| {
        meta.split_whitespace()
            .any(|word| word.eq_ignore_ascii_case("preview"))
    });
    let markdown = cx.node_source(node).unwrap_or(&code.value).to_owned();
    Some((
        HtmlSource {
            html: code.value.clone(),
            offset: code.start,
        },
        preview,
        markdown,
    ))
}

pub(crate) struct VisualPlugin {
    pub(crate) owner: Option<u64>,
    pub(crate) touch: bool,
    pub(crate) pages: Option<ViewOpener>,
}

impl MarkdownPlugin for VisualPlugin {
    fn name(&self) -> &str {
        VISUAL_NODE_NAME
    }

    fn parse<'a>(
        &self,
        node: &'a markdown_ast::AstNode<'a>,
        cx: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        let (source, preview, markdown) = html_fence(node, cx)?;
        if !preview {
            return None;
        }
        let text = source.html.clone();
        Some(
            MarkdownNode::new(VISUAL_NODE_NAME, source)
                .text(text)
                .markdown(markdown),
        )
    }

    fn render(&self, node: &MarkdownNode, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let source = node.data::<HtmlSource>().expect("visual node data");
        render_visual(
            source,
            self.owner,
            self.touch,
            self.pages.as_ref(),
            window,
            cx,
        )
    }
}

pub(crate) struct PagePlugin {
    pub(crate) view: Option<ViewOpener>,
}

impl MarkdownPlugin for PagePlugin {
    fn name(&self) -> &str {
        PAGE_NODE_NAME
    }

    fn parse<'a>(
        &self,
        node: &'a markdown_ast::AstNode<'a>,
        cx: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        let (source, preview, markdown) = html_fence(node, cx)?;
        if preview || !is_html_document(&source.html) {
            return None;
        }
        let text = source.html.clone();
        Some(
            MarkdownNode::new(PAGE_NODE_NAME, source)
                .text(text)
                .markdown(markdown),
        )
    }

    fn render(&self, node: &MarkdownNode, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let source = node.data::<HtmlSource>().expect("page node data");
        render_page(source, self.view.as_ref(), window, cx)
    }
}

fn block_id(state: &ala::Entity<HtmlPreviewState>, offset: usize) -> u64 {
    let mut hasher = DefaultHasher::new();
    state.entity_id().as_u64().hash(&mut hasher);
    offset.hash(&mut hasher);
    hasher.finish()
}

fn measure(state: &ala::Entity<HtmlPreviewState>) -> impl IntoElement + use<> {
    let state = state.clone();
    canvas(
        move |bounds, window, cx| {
            let changed = state.update(cx, |state, _| {
                state.origin = Some(bounds.origin);
                state.width.replace(bounds.size.width) != Some(bounds.size.width)
            });
            if changed {
                window.request_animation_frame();
            }
        },
        |_, (), _, _| {},
    )
    .absolute()
    .size_full()
}

fn drawing(id: (&'static str, usize), height: Pixels, cx: &App) -> AnyElement {
    Card::new(id, CardLook::Activity)
        .p_0()
        .flex()
        .w_full()
        .h(height)
        .items_center()
        .justify_center()
        .child(
            Spinner::new()
                .with_size(px(14.0))
                .color(cx.theme().foreground.muted()),
        )
        .into_any_element()
}

fn failure(error: &SharedString, cx: &App) -> ala::Div {
    h_flex()
        .min_w_0()
        .gap_2()
        .items_center()
        .type_style(TextStyle::Body, cx)
        .text_color(cx.theme().foreground.muted())
        .child(
            Icon::new(IconName::TriangleAlert)
                .size(px(14.0))
                .relative()
                .top(crate::icon::cap_center_drop(px(13.0), px(14.0), cx)),
        )
        .child(div().min_w_0().child(error.clone()))
}

fn fenced(code: &str) -> String {
    let longest = code
        .split(|character| character != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    format!("{fence}html\n{}\n{fence}", code.trim_end_matches('\n'))
}

fn render_visual(
    source: &HtmlSource,
    owner: Option<u64>,
    touch: bool,
    pages: Option<&ViewOpener>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let offset = source.offset;
    let state = window.use_keyed_state(
        SharedString::from(format!("zz-visual/{offset}")),
        cx,
        |_, _| HtmlPreviewState::default(),
    );
    let block = block_id(&state, offset);
    let shows_code = cx
        .try_global::<HtmlPreviewToggles>()
        .is_some_and(|toggles| toggles.0.contains(&block));
    let page = visual_page(&source.html, cx);
    let framed = cx.has_global::<AgentHtmlFrames>();
    let zoom = if cx
        .try_global::<AgentHtmlLive>()
        .is_some_and(|live| !live.overlay)
    {
        1.0
    } else if touch {
        TextStyle::TouchBody.design_px() / TextStyle::ReadBody.design_px()
    } else {
        (f32::from(window.rem_size()) / penna::BASE_UI_FONT_SIZE).max(0.5)
    };
    if !shows_code
        && !framed
        && let Some(width) = state.read(cx).width
    {
        let size = HtmlPreviewSize {
            width: html_preview_width(width) / zoom,
            height: None,
            scale: window.scale_factor() * zoom,
            dark: cx.theme().is_dark(),
        };
        state.update(cx, |state, cx| state.synchronize(&page, None, size, cx));
    }
    let opener = html_opener(pages, cx);
    let content = if shows_code {
        let markdown = fenced(&source.html);
        let retained = window.use_keyed_state(
            SharedString::from(format!("zz-visual-code/{offset}")),
            cx,
            {
                let markdown = markdown.clone();
                move |_, cx| cx.new(|cx| TextViewState::markdown(&markdown, cx))
            },
        );
        let text = retained.read(cx).clone();
        text.update(cx, |text, cx| {
            text.synchronize_markdown(&markdown, false, cx);
        });
        TextView::new(&text)
            .style(agent_markdown_style(cx))
            .max_w_full()
            .min_w_0()
            .selectable(true)
            .into_any_element()
    } else if let Some((height, frame)) =
        html_frame(block, &page, None, zoom, cx.theme().is_dark(), cx)
    {
        div()
            .w_full()
            .h(height.unwrap_or(px(DRAWING_HEIGHT)))
            .child(frame)
            .into_any_element()
    } else {
        match &state.read(cx).result {
            HtmlPreviewResult::Waiting => {
                let height = state.read(cx).height.unwrap_or(px(DRAWING_HEIGHT));
                drawing(("agent-visual-drawing", offset), height, cx)
            }
            HtmlPreviewResult::Ready(image, _) => {
                let size = image.size(0);
                let scale = window.scale_factor().max(1.0);
                let picture = ala::size(
                    px(size.width.0 as f32 / scale),
                    px(size.height.0 as f32 / scale),
                );
                let live = cx
                    .try_global::<AgentHtmlLive>()
                    .map(|live| (Rc::clone(&live.live), Rc::clone(&live.activate)));
                if let Some(element) = live.as_ref().and_then(|(live, _)| live(block, cx)) {
                    div()
                        .debug_selector(|| "agent-visual-live".to_owned())
                        .w(picture.width)
                        .h(picture.height)
                        .child(element)
                        .into_any_element()
                } else {
                    let origin = state.clone();
                    div()
                        .id(("agent-visual-picture", offset))
                        .w_full()
                        .min_w_0()
                        .overflow_hidden()
                        .when_some(live, |this, (_, activate)| {
                            this.on_click(move |event, window, cx| {
                                let click = origin
                                    .read(cx)
                                    .origin
                                    .map(|origin| event.position() - origin);
                                activate(
                                    HtmlLiveBlock {
                                        block,
                                        html: page.clone(),
                                        size: picture,
                                        owner,
                                        clear: true,
                                        click,
                                        zoom,
                                    },
                                    window,
                                    cx,
                                );
                            })
                        })
                        .child(
                            img(ImageSource::Render(Arc::clone(image)))
                                .debug_selector(|| "agent-visual-picture".to_owned())
                                .w(picture.width)
                                .h(picture.height),
                        )
                        .into_any_element()
                }
            }
            HtmlPreviewResult::Failed(error) => failure(error, cx)
                .w_full()
                .min_h(px(40.0))
                .child(div().flex_1())
                .when_some(opener.clone(), |this, opener| {
                    this.child(
                        Button::new(("agent-visual-open-failed", offset))
                            .small()
                            .label("Open as a page")
                            .on_click(move |_, window, cx| opener(page.clone(), window, cx)),
                    )
                })
                .into_any_element(),
        }
    };
    let ground = cx.theme().ground_of(CardLook::Code);
    let hover_only = crate::touch::hover_reveals(cx);
    let code = source.html.clone();
    let html = source.html.clone();
    let (toggle_icon, toggle_tip) = if shows_code {
        (IconName::Eye, "Show the visual")
    } else {
        (IconName::Braces, "Show code")
    };
    let tools = Card::new(("agent-visual-tools", offset), CardLook::Code)
        .when(!framed, |tools| tools.absolute().top_0().right_0())
        .p_0p5()
        .h_flex()
        .gap_0p5()
        .map(|tools| super::touch_action_gap(tools, cx))
        .when(hover_only && !framed, |tools| {
            tools
                .invisible()
                .group_hover(VISUAL_GROUP, ala::Styled::visible)
        })
        .child(
            super::touch_action(
                Button::compact_icon(("agent-visual-code", offset), toggle_icon)
                    .on(ground)
                    .tooltip(toggle_tip)
                    .on_click(move |_, window, cx| {
                        let toggles = &mut cx.default_global::<HtmlPreviewToggles>().0;
                        if !toggles.remove(&block) {
                            toggles.insert(block);
                        }
                        window.refresh();
                    }),
                cx,
            )
            .debug_selector(|| "agent-visual-code".to_owned()),
        )
        .when_some(opener, |this, opener| {
            this.child(
                super::touch_action(
                    Button::compact_icon(("agent-visual-open", offset), IconName::ExternalLink)
                        .on(ground)
                        .tooltip("Open as a page")
                        .on_click(move |_, window, cx| {
                            let page = visual_page(&html, cx);
                            opener(page, window, cx);
                        }),
                    cx,
                )
                .debug_selector(|| "agent-visual-open".to_owned()),
            )
        })
        .child(super::touch_action(
            Button::compact_icon(("agent-visual-copy", offset), IconName::Copy)
                .on(ground)
                .tooltip("Copy code")
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(code.clone()));
                }),
            cx,
        ));
    Card::new(("agent-visual", offset), CardLook::Figure)
        .group(VISUAL_GROUP)
        .relative()
        .p_0()
        .w_full()
        .min_w_0()
        .debug_selector(|| "agent-visual".to_owned())
        .child(measure(&state))
        .map(|visual| {
            if crate::touch::CoarsePointer::get(cx) {
                visual.child(content)
            } else if framed {
                visual
                    .child(h_flex().w_full().justify_end().child(tools))
                    .child(content)
            } else {
                visual.child(content).child(tools)
            }
        })
        .into_any_element()
}

fn page_title(html: &str) -> SharedString {
    let lower = html.to_ascii_lowercase();
    let title = lower
        .find("<title")
        .and_then(|open| {
            let start = open + lower[open..].find('>')? + 1;
            let end = start + lower[start..].find("</title")?;
            html.get(start..end)
        })
        .map(|title| {
            title
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .replace("&amp;", "&")
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&quot;", "\"")
                .replace("&#39;", "'")
        })
        .filter(|title| !title.is_empty());
    title.map_or_else(|| SharedString::from("Page"), SharedString::from)
}

fn render_page(
    source: &HtmlSource,
    view: Option<&ViewOpener>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let offset = source.offset;
    let state = window.use_keyed_state(
        SharedString::from(format!("zz-page/{offset}")),
        cx,
        |_, _| HtmlPreviewState::default(),
    );
    let width = state.read(cx).width;
    let block = block_id(&state, offset);
    if !cx.has_global::<AgentHtmlFrames>()
        && let Some(width) = width
    {
        let column = html_preview_width(width);
        let size = HtmlPreviewSize {
            width: px(PAGE_WIDTH),
            height: Some(px(PAGE_HEIGHT)),
            scale: window.scale_factor() * f32::from(column) / PAGE_WIDTH,
            dark: cx.theme().is_dark(),
        };
        state.update(cx, |state, cx| {
            state.synchronize(&source.html, None, size, cx);
        });
    }
    let title = page_title(&source.html);
    let opener = html_opener(view, cx);
    let ground = cx.theme().ground_of(CardLook::Code);
    let shows = view
        .filter(|_| source.html.len() <= zz_protocol::MAX_VIEW_TEXT_BYTES)
        .cloned();
    let strip = h_flex()
        .w_full()
        .h(px(STRIP_HEIGHT))
        .flex_none()
        .items_center()
        .gap_2()
        .pl_3()
        .pr_1()
        .child(
            Icon::new(IconName::Globe)
                .size(px(13.0))
                .text_color(cx.theme().foreground.muted())
                .relative()
                .top(crate::icon::cap_center_drop(px(12.0), px(13.0), cx)),
        )
        .child(
            div()
                .debug_selector(|| "agent-page-title".to_owned())
                .flex_1()
                .min_w_0()
                .truncate()
                .type_style(TextStyle::Small, cx)
                .text_color(cx.theme().foreground)
                .child(title.clone()),
        )
        .when_some(shows, |this, shows| {
            let text = source.html.clone();
            let title = title.to_string();
            this.child(message_action(
                Button::compact_icon(("agent-page-code", offset), IconName::Braces)
                    .on(ground)
                    .tooltip("Show code")
                    .on_click(move |_, window, cx| {
                        shows(
                            ViewRequest {
                                page: false,
                                title: title.clone(),
                                content: ViewContent::Code {
                                    language: Some("html".to_owned()),
                                    text: text.clone(),
                                },
                            },
                            window,
                            cx,
                        );
                    }),
                cx,
            ))
        })
        .when_some(opener.clone(), |this, opener| {
            let html = source.html.clone();
            this.child(
                message_action(
                    Button::compact_icon(("agent-page-open", offset), IconName::ExternalLink)
                        .on(ground)
                        .tooltip("Open as a page")
                        .on_click(move |_, window, cx| opener(html.clone(), window, cx)),
                    cx,
                )
                .debug_selector(|| "agent-page-open".to_owned()),
            )
        });
    let tall = width.map_or(px(0.0), |width| width * (PAGE_HEIGHT / PAGE_WIDTH));
    let frame = html_frame(
        block,
        &source.html,
        Some(ala::size(px(PAGE_WIDTH), px(PAGE_HEIGHT))),
        1.0,
        false,
        cx,
    )
    .map(|(_, frame)| frame);
    let view = match (frame, &state.read(cx).result) {
        (Some(frame), _) => div()
            .w_full()
            .h(tall)
            .bg(Hsla::white())
            .child(frame)
            .into_any_element(),
        (None, HtmlPreviewResult::Waiting) => div()
            .w_full()
            .h(tall)
            .flex()
            .items_center()
            .justify_center()
            .child(
                Spinner::new()
                    .with_size(px(14.0))
                    .color(cx.theme().foreground.muted()),
            )
            .into_any_element(),
        (None, HtmlPreviewResult::Ready(image, _)) => {
            let html = source.html.clone();
            div()
                .id(("agent-page-picture", offset))
                .w_full()
                .h(tall)
                .overflow_hidden()
                .bg(Hsla::white())
                .when_some(opener, |this, opener| {
                    this.click_cursor()
                        .on_click(move |_, window, cx| opener(html.clone(), window, cx))
                })
                .child(
                    img(ImageSource::Render(Arc::clone(image)))
                        .debug_selector(|| "agent-page-picture".to_owned())
                        .size_full()
                        .object_fit(ObjectFit::Fill),
                )
                .into_any_element()
        }
        (None, HtmlPreviewResult::Failed(error)) => failure(error, cx)
            .w_full()
            .h(px(FAILED_HEIGHT))
            .justify_center()
            .into_any_element(),
    };
    Card::new(("agent-page", offset), CardLook::Code)
        .p_0()
        .v_flex()
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .debug_selector(|| "agent-page".to_owned())
        .child(strip)
        .child(
            v_flex()
                .relative()
                .w_full()
                .px(px(1.0))
                .pb(px(1.0))
                .child(div().relative().w_full().child(measure(&state)).child(view)),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use ala::{Context, Render, Task, TestAppContext, VisualTestContext};

    use super::*;
    use crate::{
        agent::{
            AgentHtmlPreviewer, FIGURE_RENDER_DEBOUNCE, HtmlBlocks, HtmlFrame,
            chat_markdown_extensions,
        },
        text::MarkdownExtensions,
    };

    struct ChatAnswer {
        text: SharedString,
        extensions: MarkdownExtensions,
    }

    impl Render for ChatAnswer {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().w(px(520.0)).h(px(900.0)).child(
                TextView::markdown("answer", self.text.clone())
                    .markdown_extensions(self.extensions.clone()),
            )
        }
    }

    type Asked = Rc<RefCell<Vec<(String, HtmlPreviewSize)>>>;

    fn shown<'a>(answer: &str, cx: &'a mut TestAppContext) -> (Asked, &'a mut VisualTestContext) {
        cx.update(crate::init);
        let asked = Asked::default();
        let text = SharedString::from(answer.to_owned());
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|_| ChatAnswer {
                text,
                extensions: chat_markdown_extensions(
                    None,
                    Some(HtmlBlocks {
                        owner: 7,
                        touch: false,
                    }),
                ),
            });
            penna::Root::new(view, window, cx)
        });
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
        for _ in 0..2 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
        }
        cx.executor().advance_clock(FIGURE_RENDER_DEBOUNCE);
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        (asked, cx)
    }

    #[ala::test]
    fn a_preview_block_draws_bare_with_the_kit_and_the_live_theme(cx: &mut TestAppContext) {
        let (asked, cx) = shown(
            "```html preview\n<button class=\"btn\">Go</button>\n```\n",
            cx,
        );
        let asked = asked.borrow();
        let (html, size) = asked.last().expect("the previewer was asked");
        assert!(html.starts_with("<!doctype html>"), "{html}");
        assert!(html.contains("[data-penna=\"button\"][data-prominent]"));
        assert!(html.contains(".penna-segmented"));
        assert!(html.contains("--background:transparent;"));
        assert!(html.ends_with("<body><button class=\"btn\">Go</button></body></html>"));
        assert_eq!(size.height, None);
        assert!(cx.debug_bounds("agent-visual-picture").is_some());
        assert!(cx.debug_bounds("agent-code-language").is_none());
    }

    #[ala::test]
    fn a_whole_document_draws_as_a_page_at_desktop_size(cx: &mut TestAppContext) {
        let page = "<!doctype html>\n<html><head><title>Fig &amp;\n Kettle</title></head><body>hi</body></html>";
        let (asked, cx) = shown(&format!("```html\n{page}\n```\n"), cx);
        let asked = asked.borrow();
        let (html, size) = asked.last().expect("the previewer was asked");
        assert_eq!(html, page);
        assert_eq!((size.width, size.height), (px(1280.0), Some(px(720.0))));
        assert_eq!(page_title(page), "Fig & Kettle");
        assert!(cx.debug_bounds("agent-page-picture").is_some());
    }

    #[ala::test]
    fn frames_scale_visuals_with_the_ui_zoom_and_hold_pages_at_desktop_size(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init);
        let placed = Rc::new(RefCell::new(Vec::<HtmlFrame>::new()));
        let sink = Rc::clone(&placed);
        cx.update(|cx| {
            cx.set_global(AgentHtmlFrames {
                height: Rc::new(|_, _| Some(px(90.0))),
                place: Rc::new(move |frame| sink.borrow_mut().push(frame)),
            });
        });
        let text = SharedString::from(
            "```html preview\n<b>hi</b>\n```\n\n```html\n<!doctype html><html><body>page</body></html>\n```\n",
        );
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|_| ChatAnswer {
                text,
                extensions: chat_markdown_extensions(
                    None,
                    Some(HtmlBlocks {
                        owner: 7,
                        touch: false,
                    }),
                ),
            });
            penna::Root::new(view, window, cx)
        });
        for _ in 0..2 {
            placed.borrow_mut().clear();
            cx.update(|window, cx| {
                penna::Theme::global_mut(cx).font_size = px(penna::BASE_UI_FONT_SIZE * 1.5);
                _ = window.draw(cx);
            });
        }
        let placed = placed.borrow();
        let [visual, page] = placed.as_slice() else {
            panic!("two frames placed: {placed:?}");
        };
        assert!(visual.page.ends_with("<body><b>hi</b></body></html>"));
        assert_eq!(
            (
                visual.bounds.size.height,
                visual.layout,
                visual.zoom,
                visual.live
            ),
            (px(135.0), None, 1.5, true)
        );
        assert_eq!(
            page.page.as_ref(),
            "<!doctype html><html><body>page</body></html>"
        );
        assert_eq!(
            (page.layout, page.zoom, page.live),
            (Some(ala::size(px(PAGE_WIDTH), px(PAGE_HEIGHT))), 1.0, false)
        );
        assert!(cx.debug_bounds("agent-visual-picture").is_none());
    }

    #[test]
    fn part_covered_pixels_lose_their_premultiplied_color() {
        let mut bgra = [10, 20, 40, 128, 0, 0, 0, 0, 9, 9, 9, 255];
        straighten_alpha(&mut bgra);
        assert_eq!(bgra, [19, 39, 79, 128, 0, 0, 0, 0, 9, 9, 9, 255]);
    }

    #[test]
    fn a_page_without_a_title_is_named_page() {
        assert_eq!(page_title("<html><body>hi</body></html>"), "Page");
    }
}
