use ala::{
    AnyView, AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, Styled as _, Subscription, Window, div, px,
};
use penna::blocks::appearance::{
    AppearanceControls, appearance_page, appearance_page_items, kit_row,
};
use penna::{
    ActiveTheme as _, blocks::settings::StackPosition, foundation::interface_theme::Appearance,
};

use crate::story::{Framed as _, Section, Story};

pub const STORY: Story = Story {
    id: "appearance",
    name: "Appearance",
    group: "Kit",
    summary: "The Appearance page any app gets from the kit: appearance_page lays out appearance_page_items, and kit_row draws every row from an Appearance. Changes come back as AppearanceChange; here they edit the story's own Appearance and leave the storybook's look alone.",
    sections: &[
        Section {
            id: "page",
            name: "Page",
            summary: "Three groups, none folded away: Look (mode, accent, tint, corner radius in percent, controls, glass, UI font and Mono font, whose list opens on Monospaced), Size (text size and density) and Accessibility (increase contrast, reduce transparency, motion in three steps and the pointing hand).",
            build: |window, cx| page(false, window, cx),
        },
        Section {
            id: "narrow",
            name: "Narrow",
            summary: "With stacked set, the swatches, sliders and density segments sit under their titles across the row, the way a phone or a narrow window shows them.",
            build: |window, cx| page(true, window, cx),
        },
    ],
};

struct AppearanceStory {
    knobs: Appearance,
    appearance: Appearance,
    controls: AppearanceControls,
    stacked: bool,
    _knobs: Subscription,
}

fn page(stacked: bool, window: &mut Window, cx: &mut ala::App) -> AnyView {
    cx.new(|cx: &mut Context<AppearanceStory>| {
        let knobs = crate::knobs().appearance();
        let controls = AppearanceStory::controls(&knobs, window, cx);
        AppearanceStory {
            appearance: knobs.clone(),
            knobs,
            controls,
            stacked,
            _knobs: cx.observe_global::<penna::Theme>(|story, cx| {
                if crate::knobs().appearance() != story.knobs {
                    cx.notify();
                }
            }),
        }
    })
    .into()
}

impl Render for AppearanceStory {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let knobs = crate::knobs().appearance();
        if knobs != self.knobs {
            self.controls = Self::controls(&knobs, window, cx);
            self.appearance = knobs.clone();
            self.knobs = knobs;
        }
        let view = cx.entity();
        let items = appearance_page_items::<()>();
        let page = appearance_page("Appearance", items, move |item, position, window, cx| {
            view.update(cx, |story, cx| story.row(item, position, window, cx))
        });
        div()
            .id(if self.stacked {
                "appearance-narrow"
            } else {
                "appearance-page"
            })
            .flex()
            .w(px(if self.stacked { 360.0 } else { 640.0 }))
            .h(px(if self.stacked { 1150.0 } else { 940.0 }))
            .rounded(cx.theme().radius)
            .framed(cx)
            .overflow_hidden()
            .child(page)
    }
}

impl AppearanceStory {
    fn controls(
        appearance: &Appearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AppearanceControls {
        let story = cx.entity().downgrade();
        AppearanceControls::new(
            appearance,
            move |change, _, cx| {
                let _ = story.update(cx, |story, cx| {
                    story.appearance.change(change);
                    cx.notify();
                });
            },
            window,
            cx,
        )
    }

    fn row(
        &self,
        item: penna::blocks::appearance::AppearancePageItem<()>,
        position: StackPosition,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ala::AnyElement {
        kit_row(
            item,
            &self.appearance,
            &self.controls,
            self.stacked,
            window,
            cx,
        )
        .map_or_else(
            || div().into_any_element(),
            |entry| entry.position(position).into_any_element(),
        )
    }
}
