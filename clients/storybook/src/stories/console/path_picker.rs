use ala::{
    AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    Styled as _, Window, div, prelude::*, px,
};
use zz_protocol::PaneId;
use zz_ui::command::CommandPaletteView;

use super::palette::{Files, files_palette, mount};
use crate::story::{Section, Story, states};

const PANE: Option<PaneId> = Some(PaneId(10));

pub const STORY: Story = Story {
    id: "path-picker",
    name: "prefix F",
    group: "Commands",
    summary: "prefix F puts a path into the pane it came from. It opens the command palette in its & mode with the pane's folder typed after the &, so the folder lists at once and git's letter marks what changed. Typing after the last / searches everything under that folder, Tab or Right opens a folder, Shift-Tab, Left or Backspace on a folder's edge goes up one, Enter puts the highlighted path at that pane's cursor and Alt-Enter copies it.",
    sections: &[
        Section {
            id: "browse",
            name: "Browse",
            summary: "prefix F on a pane in ~/dev/zz: its folders first, then its files, with git letters on the right.",
            build: |window, cx| single(window, cx, "~/dev/zz/", &[]),
        },
        Section {
            id: "query",
            name: "Query",
            summary: "~/dev/zz/pal: typing after the last / ranks paths from every depth under the folder and highlights the matched letters.",
            build: |window, cx| single(window, cx, "~/dev/zz/", &["p", "a", "l"]),
        },
        Section {
            id: "nested",
            name: "Nested folder",
            summary: "A deeper folder, as Tab leaves it after opening crates, zz-ui and src in turn.",
            build: |window, cx| single(window, cx, "~/dev/zz/crates/zz-ui/src/", &[]),
        },
        Section {
            id: "states",
            name: "Listing states",
            summary: "What the palette says while the host walks the folder, when the host refuses, and when the folder is empty.",
            build: |window, cx| cx.new(|cx| ListingStates::new(window, cx)).into(),
        },
    ],
};

fn single(
    window: &mut Window,
    cx: &mut App,
    start: &str,
    keys: &'static [&'static str],
) -> AnyView {
    let palette = files_palette(window, cx, Files::Listed, start, PANE);
    mount(window, cx, palette, 560.0, keys)
}

struct ListingStates {
    palettes: Vec<(&'static str, Entity<CommandPaletteView>)>,
}

impl ListingStates {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let palettes = [
            ("listing", Files::Listing, "~/dev/zz/"),
            ("refused", Files::Refused, "~/Library/Mail/"),
            ("empty folder", Files::Empty, "~/dev/zz/target/"),
        ]
        .into_iter()
        .map(|(caption, files, start)| (caption, files_palette(window, cx, files, start, PANE)))
        .collect();
        Self { palettes }
    }
}

impl Render for ListingStates {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.palettes
            .iter()
            .enumerate()
            .fold(states(), |states, (index, (caption, palette))| {
                states.state(
                    *caption,
                    div()
                        .id(("path-picker-state", index))
                        .relative()
                        .w_full()
                        .h(px(240.0))
                        .child(palette.clone()),
                )
            })
    }
}
