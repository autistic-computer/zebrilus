use ala::{
    AnyView, AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, Styled as _, Window, div, px,
};
use penna::{
    ActiveTheme as _,
    interface_theme::Appearance,
    settings::{
        StackPosition,
        appearance::{AppearanceControls, appearance_page, appearance_page_items, kit_row},
        settings_list_disclosure_header,
    },
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
            summary: "Mode, Theme, Accent and Tint, then the Advanced rows: corner radius, contrast, UI font, UI zoom, animations and their speed. Without a theme menu the Theme row has no more button.",
            build: |window, cx| page(false, window, cx),
        },
        Section {
            id: "narrow",
            name: "Narrow",
            summary: "With stacked set, the swatches and sliders sit under their titles across the row, the way a phone or a narrow window shows them.",
            build: |window, cx| page(true, window, cx),
        },
    ],
};

struct AppearanceStory {
    appearance: Appearance,
    controls: AppearanceControls,
    advanced: bool,
    stacked: bool,
}

fn page(stacked: bool, window: &mut Window, cx: &mut ala::App) -> AnyView {
    cx.new(|cx: &mut Context<AppearanceStory>| {
        let appearance = crate::knobs().appearance();
        let story = cx.entity().downgrade();
        let controls = AppearanceControls::new(
            &appearance,
            move |change, _, cx| {
                let _ = story.update(cx, |story, cx| {
                    story.appearance.change(change, &[]);
                    cx.notify();
                });
            },
            window,
            cx,
        );
        AppearanceStory {
            appearance,
            controls,
            advanced: true,
            stacked,
        }
    })
    .into()
}

impl Render for AppearanceStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let items = appearance_page_items::<()>(self.advanced);
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
            .h(px(if self.stacked { 980.0 } else { 760.0 }))
            .rounded(cx.theme().radius)
            .framed(cx)
            .overflow_hidden()
            .child(page)
    }
}

impl AppearanceStory {
    fn row(
        &self,
        item: penna::settings::appearance::AppearancePageItem<()>,
        position: StackPosition,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ala::AnyElement {
        use penna::settings::appearance::{ADVANCED_DISCLOSURE, AppearancePageItem};
        if let AppearancePageItem::Advanced { expanded } = item {
            return settings_list_disclosure_header(ADVANCED_DISCLOSURE, "Advanced", expanded)
                .on_click(cx.listener(|story, _, _, cx| {
                    story.advanced = !story.advanced;
                    cx.notify();
                }))
                .into_any_element();
        }
        kit_row(
            item,
            &self.appearance,
            &[],
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
