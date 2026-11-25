use ala::{
    App, ElementId, IntoElement, ParentElement as _, RenderOnce, Role, SharedString,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder as _, px,
};

use crate::{
    ActiveTheme as _, AuthoredElement as _, Colorize as _, Icon, IconName, Kind, Sizable as _,
    Space, StyledExt as _, Surface, TextStyle, Tone,
    components::button::{Button, ButtonVariants as _},
    components::separator::Separator,
    foundation::RuleLook,
    h_flex, v_flex,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiffLayout {
    #[default]
    Unified,
    Split,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffKind {
    Context,
    Added,
    Removed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffKind,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub text: SharedString,
}

impl DiffLine {
    #[must_use]
    pub fn context(old_line: u32, new_line: u32, text: impl Into<SharedString>) -> Self {
        Self {
            kind: DiffKind::Context,
            old_line: Some(old_line),
            new_line: Some(new_line),
            text: text.into(),
        }
    }

    #[must_use]
    pub fn added(new_line: u32, text: impl Into<SharedString>) -> Self {
        Self {
            kind: DiffKind::Added,
            old_line: None,
            new_line: Some(new_line),
            text: text.into(),
        }
    }

    #[must_use]
    pub fn removed(old_line: u32, text: impl Into<SharedString>) -> Self {
        Self {
            kind: DiffKind::Removed,
            old_line: Some(old_line),
            new_line: None,
            text: text.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct DiffHunk {
    pub header: SharedString,
    pub lines: Vec<DiffLine>,
}

impl DiffHunk {
    #[must_use]
    pub fn new(header: impl Into<SharedString>, lines: Vec<DiffLine>) -> Self {
        Self {
            header: header.into(),
            lines,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoElement)]
pub struct DiffStat {
    pub added: usize,
    pub removed: usize,
    blocks: usize,
}

impl DiffStat {
    #[must_use]
    pub const fn new(added: usize, removed: usize) -> Self {
        Self {
            added,
            removed,
            blocks: 5,
        }
    }

    #[must_use]
    pub const fn blocks(mut self, count: usize) -> Self {
        self.blocks = count;
        self
    }

    #[must_use]
    pub fn buckets(self) -> (usize, usize, usize) {
        let total = self.added + self.removed;
        if total == 0 || self.blocks == 0 {
            return (0, 0, self.blocks);
        }
        let mut add = (self.added * self.blocks + total / 2) / total;
        let mut rem = self.blocks.saturating_sub(add);
        if self.added > 0 && add == 0 {
            add = 1;
            rem = self.blocks.saturating_sub(1);
        }
        if self.removed > 0 && rem == 0 && self.blocks > 1 {
            rem = 1;
            add = self.blocks.saturating_sub(1);
        }
        (add, rem, self.blocks.saturating_sub(add + rem))
    }
}

impl RenderOnce for DiffStat {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let add_color = theme.tone(Tone::Success);
        let rem_color = theme.tone(Tone::Danger);
        let idle = theme.border();
        let (add_n, rem_n, neutral_n) = self.buckets();

        let pips = (0..add_n)
            .map(|_| add_color)
            .chain((0..rem_n).map(|_| rem_color))
            .chain((0..neutral_n).map(|_| idle))
            .map(|color| div().size(px(6.0)).rounded(px(2.0)).bg(color));

        h_flex()
            .flex_none()
            .items_center()
            .gap(Space::S6)
            .font_family(theme.mono_font_family.clone())
            .type_style(TextStyle::Caption, cx)
            .child(
                div()
                    .text_color(add_color)
                    .child(format!("+{}", self.added)),
            )
            .child(
                div()
                    .text_color(rem_color)
                    .child(format!("−{}", self.removed)),
            )
            .child(h_flex().items_center().gap(Space::S2).children(pips))
    }
}

#[derive(IntoElement)]
pub struct DiffView {
    id: ElementId,
    path: SharedString,
    hunks: Vec<DiffHunk>,
    layout: DiffLayout,
    style: StyleRefinement,
}

impl DiffView {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, path: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            path: path.into(),
            hunks: Vec::new(),
            layout: DiffLayout::Unified,
            style: StyleRefinement::default(),
        }
    }

    #[must_use]
    pub fn hunk(mut self, hunk: DiffHunk) -> Self {
        self.hunks.push(hunk);
        self
    }

    #[must_use]
    pub const fn layout(mut self, layout: DiffLayout) -> Self {
        self.layout = layout;
        self
    }

    #[must_use]
    pub fn stat(&self) -> DiffStat {
        let mut added = 0;
        let mut removed = 0;
        for hunk in &self.hunks {
            for line in &hunk.lines {
                match line.kind {
                    DiffKind::Added => added += 1,
                    DiffKind::Removed => removed += 1,
                    DiffKind::Context => {}
                }
            }
        }
        DiffStat::new(added, removed)
    }
}

impl Styled for DiffView {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for DiffView {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let initial = self.layout;
        let layout_state = window.use_keyed_state((self.id.clone(), "layout"), cx, |_, _| initial);
        let layout = *layout_state.read(cx);
        let stat = self.stat();
        let theme = cx.theme();
        let surface = Surface::new(Kind::Card.role());
        let ground = theme.ground_of(surface);
        let fg = theme.foreground;
        let muted = theme.foreground.muted();
        let faint = theme.foreground.faint();
        let add_color = theme.tone(Tone::Success);
        let rem_color = theme.tone(Tone::Danger);
        let mono = theme.mono_font_family.clone();

        let toggle_state = layout_state.clone();
        let header = h_flex()
            .w_full()
            .h(px(34.0))
            .px(Space::S12)
            .gap(Space::S8)
            .items_center()
            .justify_between()
            .child(
                h_flex()
                    .items_center()
                    .gap(Space::S6)
                    .min_w_0()
                    .child(Icon::new(IconName::File).xsmall().text_color(muted))
                    .child(
                        div()
                            .truncate()
                            .font_family(mono.clone())
                            .type_style(TextStyle::Small, cx)
                            .text_color(fg)
                            .child(self.path),
                    ),
            )
            .child(
                h_flex()
                    .flex_none()
                    .items_center()
                    .gap(Space::S8)
                    .child(stat)
                    .child(
                        Button::new((self.id.clone(), "mode"))
                            .ghost()
                            .xsmall()
                            .label(if layout == DiffLayout::Unified {
                                "Split"
                            } else {
                                "Unified"
                            })
                            .on(ground)
                            .on_click(move |_, _, cx| {
                                toggle_state.update(cx, |mode, cx| {
                                    *mode = match *mode {
                                        DiffLayout::Unified => DiffLayout::Split,
                                        DiffLayout::Split => DiffLayout::Unified,
                                    };
                                    cx.notify();
                                });
                            }),
                    ),
            );

        let mut body = v_flex().w_full().min_w_0();
        for hunk in self.hunks {
            body = body.child(
                div()
                    .w_full()
                    .px(Space::S12)
                    .py(Space::S4)
                    .bg(theme.background)
                    .font_family(mono.clone())
                    .type_style(TextStyle::Caption, cx)
                    .text_color(muted)
                    .child(hunk.header),
            );
            for line in hunk.lines {
                let (wash, mark, mark_color) = match line.kind {
                    DiffKind::Added => (Some(add_color.opacity(0.11)), "+", add_color),
                    DiffKind::Removed => (Some(rem_color.opacity(0.11)), "−", rem_color),
                    DiffKind::Context => (None, " ", faint),
                };
                let old = line.old_line.map_or_else(String::new, |n| n.to_string());
                let new = line.new_line.map_or_else(String::new, |n| n.to_string());
                body = body.child(
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .px(Space::S8)
                        .gap(Space::S8)
                        .when_some(wash, Styled::bg)
                        .font_family(mono.clone())
                        .type_style(TextStyle::Small, cx)
                        .child(div().flex_none().w(px(28.0)).text_color(faint).child(old))
                        .child(div().flex_none().w(px(28.0)).text_color(faint).child(new))
                        .child(
                            div()
                                .flex_none()
                                .w(px(10.0))
                                .text_color(mark_color)
                                .child(mark),
                        )
                        .child(div().flex_1().min_w_0().text_color(fg).child(line.text)),
                );
            }
        }

        v_flex()
            .author_id(self.id)
            .role(Role::Region)
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .surface(surface, cx)
            .refine_style(&self.style)
            .child(header)
            .child(Separator::horizontal().look(RuleLook::Bar))
            .child(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_stat_computes_counts_and_five_block_buckets() {
        let diff = DiffView::new("d", "src/lib.rs").hunk(DiffHunk::new(
            "@@ -1,3 +1,4 @@",
            vec![
                DiffLine::context(1, 1, "pub mod foundation;"),
                DiffLine::removed(2, "pub mod old;"),
                DiffLine::added(2, "pub mod components;"),
                DiffLine::added(3, "pub mod blocks;"),
            ],
        ));
        let stat = diff.stat();
        assert_eq!(stat.added, 2);
        assert_eq!(stat.removed, 1);
        assert_eq!(stat.buckets(), (3, 2, 0));
        assert_eq!(DiffStat::new(0, 0).buckets(), (0, 0, 5));
        assert_eq!(DiffStat::new(100, 1).buckets(), (4, 1, 0));
    }
}
