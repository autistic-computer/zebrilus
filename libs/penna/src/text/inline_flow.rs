#![allow(clippy::pedantic, clippy::style, clippy::complexity)]

use std::{
    ops::Range,
    sync::{Arc, Mutex},
};

use ala::{
    AbsoluteLength, AnyElement, App, AvailableSpace, Bounds, DefiniteLength, Element, ElementId,
    GlobalElementId, HighlightStyle, InspectorElementId, InteractiveElement as _, IntoElement,
    LayoutId, LineFragment as WrapLineFragment, ObjectFit, Pixels, ShapedLine, SharedString,
    SharedUri, Size, StatefulInteractiveElement as _, Styled, StyledImage as _, TextRun, TextStyle,
    WhiteSpace, Window, img, point, prelude::FluentBuilder as _, px, relative, size,
};

use crate::{ActiveTheme as _, WindowExt as _, tooltip::Tooltip};

use super::{
    inline::{Inline, InlineState, code_runs},
    links::{LinkClick, SharedLinks, open_link},
    node::LinkMark,
};

const IMAGE_LEN: usize = 1;

pub(super) struct InlineFlow {
    id: ElementId,
    items: Vec<InlineFlowItem>,
    links: Option<SharedLinks>,
}

pub(super) enum InlineFlowItem {
    Text {
        state: Arc<Mutex<InlineState>>,
        text: SharedString,
        links: Vec<(Range<usize>, LinkMark)>,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
        code_ranges: Vec<Range<usize>>,
    },
    Image {
        url: SharedUri,
        link: Option<LinkMark>,
        title: String,
        width: Option<DefiniteLength>,
        height: Option<DefiniteLength>,
    },
}

#[derive(Default)]
pub(crate) struct InlineFlowLayoutState {
    layout: Arc<Mutex<Option<InlineFlowLayout>>>,
}

#[derive(Default)]
struct InlineFlowLayout {
    fragments: Vec<PositionedFragment>,
    size: Size<Pixels>,
}

#[derive(Clone)]
enum PositionedFragment {
    Text {
        item_ix: usize,
        origin: ala::Point<Pixels>,
        size: Size<Pixels>,
        source_range: Range<usize>,
        text: SharedString,
        links: Vec<(Range<usize>, LinkMark)>,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
        code_ranges: Vec<Range<usize>>,
    },
    Image {
        item_ix: usize,
        origin: ala::Point<Pixels>,
        size: Size<Pixels>,
    },
}

enum MeasureItem {
    Text {
        text: SharedString,
        links: Vec<(Range<usize>, LinkMark)>,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
        code_ranges: Vec<Range<usize>>,
    },
    Image {
        url: SharedUri,
        width: Option<DefiniteLength>,
        height: Option<DefiniteLength>,
    },
}

struct LineFragmentLayout {
    item_ix: usize,
    kind: LineFragmentKind,
    size: Size<Pixels>,
    source_range: Range<usize>,
}

enum WrapPiece {
    Text(Box<ShapedLine>),
    Image(Pixels),
}

enum LineFragmentKind {
    Text {
        text: SharedString,
        links: Vec<(Range<usize>, LinkMark)>,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
        code_ranges: Vec<Range<usize>>,
    },
    Image,
}

impl InlineFlow {
    pub(super) fn new(id: impl Into<ElementId>, items: Vec<InlineFlowItem>) -> Self {
        Self {
            id: id.into(),
            items,
            links: None,
        }
    }

    pub(super) fn links(mut self, links: Option<SharedLinks>) -> Self {
        self.links = links;
        self
    }

    fn image_element(
        ix: usize,
        url: &SharedUri,
        link: &Option<LinkMark>,
        title: &str,
        size: Size<Pixels>,
        links: Option<SharedLinks>,
    ) -> AnyElement {
        img(url.clone())
            .id(ix)
            .object_fit(ObjectFit::Contain)
            .max_w(relative(1.))
            .w(size.width)
            .h(size.height)
            .when_some(link.clone(), |this, link| {
                let title = title.to_string();
                this.cursor_pointer()
                    .tooltip(move |window, cx| Tooltip::new(title.clone()).build(window, cx))
                    .on_click(move |event, window, cx| {
                        window.end_text_selection(cx);
                        cx.stop_propagation();
                        let click = LinkClick {
                            url: link.url.clone(),
                            button: ala::MouseButton::Left,
                            modifiers: event.modifiers(),
                        };
                        open_link(links.as_ref(), &click, window, cx);
                    })
            })
            .into_any_element()
    }
}

impl IntoElement for InlineFlow {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for InlineFlow {
    type RequestLayoutState = InlineFlowLayoutState;
    type PrepaintState = Vec<AnyElement>;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let measure_items = self.items.iter().map(MeasureItem::from).collect::<Vec<_>>();
        let line_height = window.line_height();
        let rem_size = window.rem_size();
        let image_sizes = measure_items
            .iter()
            .enumerate()
            .map(|(ix, item)| match item {
                MeasureItem::Image { url, width, height } => Some(measure_image_size(
                    ix,
                    url,
                    *width,
                    *height,
                    line_height,
                    rem_size,
                    window,
                    cx,
                )),
                MeasureItem::Text { .. } => None,
            })
            .collect::<Vec<_>>();
        let layout_state = InlineFlowLayoutState::default();
        let layout_ref = layout_state.layout.clone();

        let mono = cx.theme().mono_font_family.clone();
        let text_style = window.text_style();
        let layout_id = window.request_measured_layout(Default::default(), {
            move |known_dimensions, available_space, window, _cx| {
                let wrap_width = if text_style.white_space == WhiteSpace::Normal {
                    known_dimensions.width.or(match available_space.width {
                        AvailableSpace::Definite(width) => Some(width),
                        _ => None,
                    })
                } else {
                    None
                };
                let layout = layout_flow(
                    &measure_items,
                    &image_sizes,
                    &text_style,
                    &mono,
                    wrap_width,
                    window,
                );
                let size = layout.size;
                if let Ok(mut state) = layout_ref.lock() {
                    *state = Some(layout);
                }
                size
            }
        });

        (layout_id, layout_state)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let fragments = request_layout
            .layout
            .lock()
            .ok()
            .and_then(|layout| layout.as_ref().map(|layout| layout.fragments.clone()))
            .unwrap_or_default();
        let mut elements = Vec::with_capacity(fragments.len());

        for fragment in fragments {
            match fragment {
                PositionedFragment::Text {
                    item_ix,
                    origin,
                    size: fragment_size,
                    source_range,
                    text,
                    links,
                    highlights,
                    code_ranges,
                    ..
                } => {
                    let state = match &self.items[item_ix] {
                        InlineFlowItem::Text {
                            state,
                            text: source,
                            ..
                        } if source_range == (0..source.len()) => state.clone(),
                        _ => Arc::new(Mutex::new(InlineState::default())),
                    };
                    if let Ok(mut state) = state.lock() {
                        state.set_text(text);
                    }

                    let mut element = Inline::new(elements.len(), state, links, highlights)
                        .code_ranges(code_ranges)
                        .links(self.links.clone())
                        .into_any_element();
                    element.prepaint_as_root(
                        bounds.origin + origin,
                        size(
                            AvailableSpace::Definite(fragment_size.width),
                            AvailableSpace::Definite(fragment_size.height),
                        ),
                        window,
                        cx,
                    );
                    elements.push(element);
                }
                PositionedFragment::Image {
                    item_ix,
                    origin,
                    size: fragment_size,
                } => {
                    let InlineFlowItem::Image {
                        url, link, title, ..
                    } = &self.items[item_ix]
                    else {
                        continue;
                    };
                    let mut element = Self::image_element(
                        elements.len(),
                        url,
                        link,
                        title.as_str(),
                        fragment_size,
                        self.links.clone(),
                    );
                    element.prepaint_as_root(
                        bounds.origin + origin,
                        size(
                            AvailableSpace::Definite(fragment_size.width),
                            AvailableSpace::Definite(fragment_size.height),
                        ),
                        window,
                        cx,
                    );
                    elements.push(element);
                }
            }
        }

        elements
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        for element in prepaint {
            element.paint(window, cx);
        }
    }
}

impl From<&InlineFlowItem> for MeasureItem {
    fn from(item: &InlineFlowItem) -> Self {
        match item {
            InlineFlowItem::Text {
                state: _,
                text,
                links,
                highlights,
                code_ranges,
            } => MeasureItem::Text {
                text: text.clone(),
                links: links.clone(),
                highlights: highlights.clone(),
                code_ranges: code_ranges.clone(),
            },
            InlineFlowItem::Image {
                url, width, height, ..
            } => MeasureItem::Image {
                url: url.clone(),
                width: *width,
                height: *height,
            },
        }
    }
}

impl MeasureItem {
    fn len(&self) -> usize {
        match self {
            MeasureItem::Text { text, .. } => text.len(),
            MeasureItem::Image { .. } => IMAGE_LEN,
        }
    }
}

fn layout_flow(
    items: &[MeasureItem],
    image_sizes: &[Option<Size<Pixels>>],
    text_style: &TextStyle,
    mono: &SharedString,
    wrap_width: Option<Pixels>,
    window: &mut Window,
) -> InlineFlowLayout {
    let line_height = text_style.line_height_in_pixels(window.rem_size());
    let total_len = items.iter().map(MeasureItem::len).sum::<usize>();
    if total_len == 0 {
        return InlineFlowLayout::default();
    }

    let line_ranges = line_ranges(items, image_sizes, text_style, mono, wrap_width, window);
    let soft_wrapped = line_ranges
        .windows(2)
        .any(|pair| pair[0].end == pair[1].start);
    let mut fragments = Vec::new();
    let mut max_width = Pixels::ZERO;
    let mut y = Pixels::ZERO;

    for line_range in line_ranges {
        let mut line_fragments = Vec::new();
        let mut line_width = Pixels::ZERO;
        let mut actual_line_height = line_height;
        let mut item_start = 0;

        for (item_ix, item) in items.iter().enumerate() {
            let item_end = item_start + item.len();
            if item_end <= line_range.start {
                item_start = item_end;
                continue;
            }
            if item_start >= line_range.end {
                break;
            }

            match item {
                MeasureItem::Text {
                    text,
                    links,
                    highlights,
                    code_ranges,
                } => {
                    let local_start = line_range.start.max(item_start) - item_start;
                    let local_end = line_range.end.min(item_end) - item_start;
                    if local_start < local_end {
                        let subtext = SharedString::from(text[local_start..local_end].to_string());
                        let highlights =
                            slice_ranges(highlights, local_start, local_end, |range, style| {
                                (range, *style)
                            });
                        let links = slice_ranges(links, local_start, local_end, |range, link| {
                            (range, link.clone())
                        });
                        let code_ranges = code_ranges
                            .iter()
                            .filter_map(|range| clip(range, local_start, local_end))
                            .collect::<Vec<_>>();
                        let width = shape(
                            subtext.clone(),
                            &highlights,
                            &code_ranges,
                            text_style,
                            mono,
                            window,
                        )
                        .width();
                        line_width += width;
                        line_fragments.push(LineFragmentLayout {
                            item_ix,
                            kind: LineFragmentKind::Text {
                                text: subtext,
                                links,
                                highlights,
                                code_ranges,
                            },
                            size: size(width, line_height),
                            source_range: local_start..local_end,
                        });
                    }
                }
                MeasureItem::Image { .. } => {
                    if line_range.start <= item_start && item_end <= line_range.end {
                        let size = image_sizes[item_ix]
                            .expect("image size should be measured before layout");
                        line_width += size.width;
                        actual_line_height = actual_line_height.max(size.height);
                        line_fragments.push(LineFragmentLayout {
                            item_ix,
                            kind: LineFragmentKind::Image,
                            size,
                            source_range: 0..IMAGE_LEN,
                        });
                    }
                }
            }

            item_start = item_end;
        }

        let mut x = Pixels::ZERO;
        for fragment in line_fragments {
            let origin = point(x, y + (actual_line_height - fragment.size.height) / 2.);
            let positioned = match fragment.kind {
                LineFragmentKind::Text {
                    text,
                    links,
                    highlights,
                    code_ranges,
                } => PositionedFragment::Text {
                    item_ix: fragment.item_ix,
                    origin,
                    size: fragment.size,
                    source_range: fragment.source_range,
                    text,
                    links,
                    highlights,
                    code_ranges,
                },
                LineFragmentKind::Image => PositionedFragment::Image {
                    item_ix: fragment.item_ix,
                    origin,
                    size: fragment.size,
                },
            };
            x += fragment.size.width;
            fragments.push(positioned);
        }

        max_width = max_width.max(line_width);
        y += actual_line_height;
    }

    let width = match wrap_width {
        Some(wrap_width) if soft_wrapped => wrap_width,
        _ => max_width,
    };
    InlineFlowLayout {
        fragments,
        size: size(width, y),
    }
}

fn line_ranges(
    items: &[MeasureItem],
    image_sizes: &[Option<Size<Pixels>>],
    text_style: &TextStyle,
    mono: &SharedString,
    wrap_width: Option<Pixels>,
    window: &mut Window,
) -> Vec<Range<usize>> {
    let total_len = items.iter().map(MeasureItem::len).sum::<usize>();
    let mut hard_lines = Vec::new();
    let mut line_start = 0;
    let mut item_start = 0;

    for item in items {
        if let MeasureItem::Text { text, .. } = item {
            for (newline, _) in text.match_indices('\n') {
                let newline = item_start + newline;
                hard_lines.push(line_start..newline);
                line_start = newline + 1;
            }
        }
        item_start += item.len();
    }
    hard_lines.push(line_start..total_len);

    let Some(wrap_width) = wrap_width else {
        return hard_lines;
    };
    let rem_size = window.rem_size();
    let font_size = text_style.font_size.to_pixels(rem_size);
    let mut wrapper = window
        .text_system()
        .line_wrapper(text_style.font(), font_size);
    let mut ranges = Vec::new();

    for hard_line in hard_lines {
        let mut item_start = 0;
        let pieces = items
            .iter()
            .enumerate()
            .filter_map(|(ix, item)| {
                let item_end = item_start + item.len();
                let piece = if item_end <= hard_line.start || item_start >= hard_line.end {
                    None
                } else {
                    match item {
                        MeasureItem::Text {
                            text,
                            highlights,
                            code_ranges,
                            ..
                        } => {
                            let start = hard_line.start.max(item_start) - item_start;
                            let end = hard_line.end.min(item_end) - item_start;
                            (start < end).then(|| {
                                WrapPiece::Text(Box::new(shape(
                                    SharedString::from(text[start..end].to_string()),
                                    &slice_ranges(highlights, start, end, |range, style| {
                                        (range, *style)
                                    }),
                                    &code_ranges
                                        .iter()
                                        .filter_map(|range| clip(range, start, end))
                                        .collect::<Vec<_>>(),
                                    text_style,
                                    mono,
                                    window,
                                )))
                            })
                        }
                        MeasureItem::Image { .. } => (hard_line.start <= item_start
                            && item_end <= hard_line.end)
                            .then(|| {
                                WrapPiece::Image(
                                    image_sizes[ix]
                                        .expect("image size should be measured before wrapping")
                                        .width,
                                )
                            }),
                    }
                };
                item_start = item_end;
                piece
            })
            .collect::<Vec<_>>();
        let wrap_fragments = pieces
            .iter()
            .map(|piece| match piece {
                WrapPiece::Text(line) => WrapLineFragment::shaped(line),
                WrapPiece::Image(width) => WrapLineFragment::element(*width, IMAGE_LEN),
            })
            .collect::<Vec<_>>();

        let boundaries = wrapper
            .wrap_line(&wrap_fragments, wrap_width)
            .map(|boundary| hard_line.start + boundary.ix.min(hard_line.len()))
            .collect::<Vec<_>>();
        let mut start = hard_line.start;

        for end in boundaries {
            if start < end {
                ranges.push(start..end);
            }
            start = end;
        }

        if start < hard_line.end || hard_line.is_empty() {
            ranges.push(start..hard_line.end);
        }
    }

    ranges
}

fn measure_image_size(
    ix: usize,
    url: &SharedUri,
    width: Option<DefiniteLength>,
    height: Option<DefiniteLength>,
    line_height: Pixels,
    rem_size: Pixels,
    window: &mut Window,
    cx: &mut App,
) -> Size<Pixels> {
    let intrinsic_size = if width.is_some() && height.is_some() {
        None
    } else {
        intrinsic_image_size(ix, url, width, height, window, cx)
    };
    image_size(width, height, intrinsic_size, line_height, rem_size)
}

fn intrinsic_image_size(
    ix: usize,
    url: &SharedUri,
    width: Option<DefiniteLength>,
    height: Option<DefiniteLength>,
    window: &mut Window,
    cx: &mut App,
) -> Option<Size<Pixels>> {
    let mut element = img(url.clone())
        .id(ix)
        .object_fit(ObjectFit::Contain)
        .max_w(relative(1.))
        .when_some(width, |this, width| this.w(width))
        .when_some(height, |this, height| this.h(height))
        .into_any_element();
    let measured_size = element.layout_as_root(AvailableSpace::min_size(), window, cx);

    if measured_size.width <= Pixels::ZERO || measured_size.height <= Pixels::ZERO {
        None
    } else {
        Some(measured_size)
    }
}

fn image_size(
    width: Option<DefiniteLength>,
    height: Option<DefiniteLength>,
    intrinsic_size: Option<Size<Pixels>>,
    line_height: Pixels,
    rem_size: Pixels,
) -> Size<Pixels> {
    let base_size = AbsoluteLength::Pixels(line_height);
    match (width, height) {
        (Some(width), Some(height)) => size(
            width.to_pixels(base_size, rem_size),
            height.to_pixels(base_size, rem_size),
        ),
        (Some(width), None) => {
            let width = width.to_pixels(base_size, rem_size);
            let height = intrinsic_size
                .and_then(|intrinsic_size| {
                    (intrinsic_size.width > Pixels::ZERO && intrinsic_size.height > Pixels::ZERO)
                        .then(|| width * (intrinsic_size.height / intrinsic_size.width))
                })
                .unwrap_or(line_height);
            size(width, height)
        }
        (None, Some(height)) => {
            let height = height.to_pixels(base_size, rem_size);
            let width = intrinsic_size
                .and_then(|intrinsic_size| {
                    (intrinsic_size.width > Pixels::ZERO && intrinsic_size.height > Pixels::ZERO)
                        .then(|| height * (intrinsic_size.width / intrinsic_size.height))
                })
                .unwrap_or(height);
            size(width, height)
        }
        (None, None) => inline_image_size_for_line(intrinsic_size, line_height),
    }
}

fn inline_image_size_for_line(
    intrinsic_size: Option<Size<Pixels>>,
    line_height: Pixels,
) -> Size<Pixels> {
    let height = line_height * 0.75;
    let aspect_ratio = intrinsic_size
        .and_then(|intrinsic_size| {
            (intrinsic_size.width > Pixels::ZERO && intrinsic_size.height > Pixels::ZERO)
                .then(|| intrinsic_size.width / intrinsic_size.height)
        })
        .unwrap_or(1.);

    size((height * aspect_ratio).max(px(1.)), height.max(px(1.)))
}

fn runs_for_highlights(
    text: &str,
    default_style: &TextStyle,
    highlights: &[(Range<usize>, HighlightStyle)],
) -> Vec<TextRun> {
    let mut runs = Vec::new();
    let mut ix = 0;

    for (range, highlight) in highlights {
        if ix < range.start {
            runs.push(default_style.clone().to_run(range.start - ix));
        }
        runs.push(
            default_style
                .clone()
                .highlight(*highlight)
                .to_run(range.len()),
        );
        ix = range.end;
    }

    if ix < text.len() {
        runs.push(default_style.to_run(text.len() - ix));
    }

    runs
}

fn shape(
    text: SharedString,
    highlights: &[(Range<usize>, HighlightStyle)],
    code_ranges: &[Range<usize>],
    text_style: &TextStyle,
    mono: &SharedString,
    window: &mut Window,
) -> ShapedLine {
    let mut runs = runs_for_highlights(&text, text_style, highlights);
    code_runs(&mut runs, code_ranges, mono);
    let font_size = text_style.font_size.to_pixels(window.rem_size());
    window
        .text_system()
        .shape_line(text, font_size, &runs, None)
}

fn slice_ranges<T, U>(
    ranges: &[(Range<usize>, T)],
    start: usize,
    end: usize,
    map: impl Fn(Range<usize>, &T) -> U,
) -> Vec<U> {
    ranges
        .iter()
        .filter_map(|(range, value)| clip(range, start, end).map(|range| map(range, value)))
        .collect()
}

fn clip(range: &Range<usize>, start: usize, end: usize) -> Option<Range<usize>> {
    let clipped_start = range.start.max(start);
    let clipped_end = range.end.min(end);
    (clipped_start < clipped_end).then(|| (clipped_start - start)..(clipped_end - start))
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use ala::{
        DevicePixels, Font, FontId, FontMetrics, FontRun, FontWeight, GlyphId, LineLayout,
        NoopTextSystem, PlatformTextSystem, RenderGlyphParams, ShapedGlyph, ShapedRun,
        TextRenderingMode,
    };

    use super::*;

    #[ala::test]
    fn hard_lines_keep_empty_lines_and_exclude_newline_bytes(cx: &mut ala::TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|window, _| {
            let items = vec![
                MeasureItem::Text {
                    text: "é\n\n".into(),
                    links: vec![],
                    highlights: vec![],
                    code_ranges: vec![],
                },
                MeasureItem::Image {
                    url: "https://example.com/image.svg".into(),
                    width: None,
                    height: None,
                },
                MeasureItem::Text {
                    text: "\nend\n".into(),
                    links: vec![],
                    highlights: vec![],
                    code_ranges: vec![],
                },
            ];
            let image_sizes = [None, Some(size(px(20.), px(10.))), None];
            let text_style = window.text_style();

            for wrap_width in [None, Some(px(400.))] {
                assert_eq!(
                    line_ranges(
                        &items,
                        &image_sizes,
                        &text_style,
                        &text_style.font_family,
                        wrap_width,
                        window
                    ),
                    vec![0..2, 3..3, 4..5, 6..9, 10..10],
                );
                let layout = layout_flow(
                    &items,
                    &image_sizes,
                    &text_style,
                    &text_style.font_family,
                    wrap_width,
                    window,
                );
                let PositionedFragment::Image { origin, .. } = layout.fragments[1] else {
                    panic!("expected inline image");
                };
                assert!(origin.y >= window.line_height() * 2.);
                assert_eq!(layout.size.height, window.line_height() * 5.);
            }
        });
    }

    struct WidthsByFont;

    impl WidthsByFont {
        fn units(font_id: FontId) -> f32 {
            match font_id.0 {
                1 => 700.,
                2 => 600.,
                _ => 500.,
            }
        }
    }

    impl PlatformTextSystem for WidthsByFont {
        fn add_fonts(&self, _: Vec<Cow<'static, [u8]>>) -> anyhow::Result<()> {
            Ok(())
        }

        fn all_font_names(&self) -> Vec<String> {
            Vec::new()
        }

        fn font_id(&self, font: &Font) -> anyhow::Result<FontId> {
            Ok(if font.family.as_ref() == "Mono" {
                FontId(2)
            } else if font.weight >= FontWeight::BOLD {
                FontId(1)
            } else {
                FontId(0)
            })
        }

        fn font_metrics(&self, font_id: FontId) -> FontMetrics {
            NoopTextSystem.font_metrics(font_id)
        }

        fn typographic_bounds(
            &self,
            font_id: FontId,
            glyph_id: GlyphId,
        ) -> anyhow::Result<Bounds<f32>> {
            NoopTextSystem.typographic_bounds(font_id, glyph_id)
        }

        fn advance(&self, font_id: FontId, _: GlyphId) -> anyhow::Result<Size<f32>> {
            Ok(size(Self::units(font_id), 0.))
        }

        fn glyph_for_char(&self, _: FontId, _: char) -> Option<GlyphId> {
            Some(GlyphId(1))
        }

        fn glyph_raster_bounds(
            &self,
            params: &RenderGlyphParams,
        ) -> anyhow::Result<Bounds<DevicePixels>> {
            NoopTextSystem.glyph_raster_bounds(params)
        }

        fn rasterize_glyph(
            &self,
            params: &RenderGlyphParams,
            raster_bounds: Bounds<DevicePixels>,
        ) -> anyhow::Result<(Size<DevicePixels>, Vec<u8>)> {
            NoopTextSystem.rasterize_glyph(params, raster_bounds)
        }

        fn layout_line(&self, text: &str, font_size: Pixels, runs: &[FontRun]) -> LineLayout {
            let em = font_size / 1000.;
            let mut x = px(0.);
            let mut prev = '\0';
            let mut chars = text.char_indices();
            let mut shaped_runs = Vec::new();
            for run in runs {
                let mut glyphs = Vec::new();
                for (index, ch) in chars.by_ref().take(run.len) {
                    if run.font_id.0 == 0 && prev == 'T' && ch == 'o' {
                        x -= em * 150.;
                    }
                    glyphs.push(ShapedGlyph {
                        id: GlyphId(1),
                        position: point(x, px(0.)),
                        index,
                        is_emoji: false,
                    });
                    x += em * Self::units(run.font_id);
                    prev = ch;
                }
                shaped_runs.push(ShapedRun {
                    font_id: run.font_id,
                    glyphs,
                });
            }
            LineLayout {
                font_size,
                width: x,
                ascent: font_size,
                descent: px(0.),
                runs: shaped_runs,
                len: text.len(),
            }
        }

        fn recommended_rendering_mode(&self, _: FontId, _: Pixels) -> TextRenderingMode {
            TextRenderingMode::Grayscale
        }
    }

    #[test]
    fn an_inline_image_paragraph_breaks_where_its_text_alone_breaks() {
        let mut cx = ala::TestAppContext::build_with_text_system(
            ala::TestDispatcher::new(0),
            None,
            Arc::new(WidthsByFont),
        );
        let cx = cx.add_empty_window();
        cx.update(|window, _| {
            let text: SharedString =
                "To ship it run cargo build --release and then wait for it to finish ".into();
            let highlights = vec![
                (15..36, HighlightStyle::default()),
                (
                    46..57,
                    HighlightStyle {
                        font_weight: Some(FontWeight::BOLD),
                        ..Default::default()
                    },
                ),
            ];
            let code_ranges = vec![highlights[0].0.clone()];
            let mono = SharedString::from("Mono");
            let text_style = window.text_style();
            let font_size = text_style.font_size.to_pixels(window.rem_size());
            let items = [
                MeasureItem::Text {
                    text: text.clone(),
                    links: vec![],
                    highlights: highlights.clone(),
                    code_ranges: code_ranges.clone(),
                },
                MeasureItem::Image {
                    url: "https://example.com/image.svg".into(),
                    width: None,
                    height: None,
                },
            ];
            let image_sizes = [None, Some(size(px(20.), px(10.)))];
            let wrap_width = px(110.);
            let mut runs = runs_for_highlights(&text, &text_style, &highlights);
            code_runs(&mut runs, &code_ranges, &mono);
            let alone = window
                .text_system()
                .shape_text(text.clone(), font_size, &runs, Some(wrap_width), None)
                .expect("paragraph shapes");
            let alone_breaks = alone[0]
                .wrap_boundaries()
                .iter()
                .map(|boundary| {
                    alone[0].unwrapped_layout.runs[boundary.run_ix].glyphs[boundary.glyph_ix].index
                })
                .collect::<Vec<_>>();

            assert_eq!(alone_breaks, vec![15, 27, 37, 46, 55]);
            assert_eq!(
                line_ranges(
                    &items,
                    &image_sizes,
                    &text_style,
                    &mono,
                    Some(wrap_width),
                    window
                ),
                vec![0..15, 15..27, 27..37, 37..46, 46..55, 55..68, 68..69],
            );
            assert_eq!(
                layout_flow(
                    &items,
                    &image_sizes,
                    &text_style,
                    &mono,
                    Some(wrap_width),
                    window
                )
                .size
                .width,
                wrap_width,
            );
        });
    }

    #[test]
    fn inline_image_without_explicit_size_scales_intrinsic_ratio_to_line_height() {
        let line_height = px(20.);
        let intrinsic_size = size(px(160.), px(40.));

        let measured = inline_image_size_for_line(Some(intrinsic_size), line_height);

        assert_eq!(measured, size(px(60.), px(15.)));
    }
}
