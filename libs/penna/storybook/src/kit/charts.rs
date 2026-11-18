use ala::{
    AnyElement, App, IntoElement, ParentElement as _, SharedString, Styled as _, Window, px,
};
use penna::{
    components::chart::{Chart, ChartKind, Series},
    v_flex,
};

use crate::story::{Section, Story, stateless, states};

pub const STORY: Story = Story {
    id: "charts",
    name: "Charts",
    group: "Kit",
    summary: "Chart::new(id, kind, labels, series) draws up to four series over one list of labels, as bars or lines, on an axis that starts at zero and ends on a round value.",
    sections: &[
        Section {
            id: "bars",
            name: "Bars",
            summary: "One series needs no legend. Two to four sit side by side in each band, 2 apart, with a legend under the labels. A band lights under the pointer and its tooltip reads the values.",
            build: |_, cx| stateless(bars, cx),
        },
        Section {
            id: "lines",
            name: "Lines",
            summary: "ChartKind::Line draws each series as a 2 line through the middle of its bands. A missing value breaks the line, and a value with no neighbor is a dot.",
            build: |_, cx| stateless(lines, cx),
        },
        Section {
            id: "edges",
            name: "Edge cases",
            summary: "Values under zero hang from the zero rule. Past twelve labels only every second or third is written. With no data the axis runs from 0 to 1.",
            build: |_, cx| stateless(edges, cx),
        },
    ],
};

const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

fn labels(names: &[&'static str]) -> Vec<SharedString> {
    names.iter().copied().map(SharedString::from).collect()
}

fn framed(chart: Chart) -> AnyElement {
    v_flex().w(px(480.0)).child(chart).into_any_element()
}

fn bars(_: &mut Window, _: &mut App) -> AnyElement {
    let errors = Series::new("Errors", [12.0, 30.0, 18.0, 44.0, 87.0, 21.0, 9.0]);
    let warnings = Series::new("Warnings", [40.0, 52.0, 31.0, 60.0, 71.0, 38.0, 22.0]);
    let retries = Series::new("Retries", [5.0, 8.0, 4.0, 15.0, 33.0, 9.0, 2.0]);
    let timeouts = Series::new("Timeouts", [2.0, 3.0, 1.0, 6.0, 12.0, 4.0, 1.0]);
    states()
        .state(
            "one series",
            framed(Chart::new(
                "bars-one",
                ChartKind::Bar,
                labels(&DAYS),
                [errors.clone()],
            )),
        )
        .state(
            "two series",
            framed(Chart::new(
                "bars-two",
                ChartKind::Bar,
                labels(&DAYS),
                [errors.clone(), warnings.clone()],
            )),
        )
        .state(
            "four series",
            framed(Chart::new(
                "bars-four",
                ChartKind::Bar,
                labels(&DAYS),
                [errors, warnings, retries, timeouts],
            )),
        )
        .into_any_element()
}

fn lines(_: &mut Window, _: &mut App) -> AnyElement {
    let p50 = Series::new("p50", [41.0, 38.0, 44.0, 52.0, 47.0, 40.0, 39.0]);
    let p99 = Series::new("p99", [120.0, 96.0, 140.0, 210.0, 180.0, 110.0, 104.0]);
    let broken = Series::new("p50", [41.0, 38.0, f64::NAN, 52.0, f64::NAN, 40.0, 39.0]);
    states()
        .state(
            "one series",
            framed(Chart::new(
                "lines-one",
                ChartKind::Line,
                labels(&DAYS),
                [p50.clone()],
            )),
        )
        .state(
            "two series",
            framed(Chart::new(
                "lines-two",
                ChartKind::Line,
                labels(&DAYS),
                [p50, p99],
            )),
        )
        .state(
            "missing values",
            framed(Chart::new(
                "lines-broken",
                ChartKind::Line,
                labels(&DAYS),
                [broken],
            )),
        )
        .into_any_element()
}

fn edges(_: &mut Window, _: &mut App) -> AnyElement {
    let signed = Series::new("Change", [4.0, -2.0, 7.0, -5.0, 1.0, 3.0, -1.0]);
    let month: Vec<SharedString> = (1..=30).map(|day| day.to_string().into()).collect();
    let builds = Series::new(
        "Builds",
        (0..30).map(|day| f64::from(1200 + (day * 37) % 900)),
    );
    states()
        .state(
            "under zero",
            framed(Chart::new(
                "edges-signed",
                ChartKind::Bar,
                labels(&DAYS),
                [signed],
            )),
        )
        .state(
            "thirty labels",
            framed(Chart::new("edges-month", ChartKind::Bar, month, [builds])),
        )
        .state(
            "no data",
            framed(Chart::new(
                "edges-empty",
                ChartKind::Bar,
                labels(&DAYS),
                [Series::new("Errors", [])],
            )),
        )
        .state(
            "phone",
            v_flex()
                .w(px(358.0))
                .child(
                    Chart::new(
                        "edges-touch",
                        ChartKind::Line,
                        labels(&DAYS),
                        [
                            Series::new("p50", [41.0, 38.0, 44.0, 52.0, 47.0, 40.0, 39.0]),
                            Series::new("p99", [120.0, 96.0, 140.0, 210.0, 180.0, 110.0, 104.0]),
                        ],
                    )
                    .touch(true),
                )
                .into_any_element(),
        )
        .into_any_element()
}
