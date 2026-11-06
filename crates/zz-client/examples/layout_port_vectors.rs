use std::error::Error;

use serde_json::{Value, json};
use zz_client::floating::{FloatCells, PipGeometry, PixelRect, float_pixels, float_placement};
use zz_client::{PaneRect, pane_box, pane_rects};
use zz_protocol::{
    Axis, Event, EventPayload, FloatingPaneSnapshot, LayoutNode, ModalPaneSnapshot, MuxSnapshot,
    PaneBorderIndicators, PaneBorderLines, PaneBorderStatus, PaneId, PaneKindSnapshot,
    PaneSnapshot, PaneSummary, ProtocolMessage, SessionId, SessionSnapshot, SplitId, WindowId,
    WindowSnapshot, encode_protocol_message,
};

fn pane(id: u64) -> LayoutNode {
    LayoutNode::Pane(PaneId(id))
}

fn split(axis: Axis, ratio: f32, first: LayoutNode, second: LayoutNode) -> LayoutNode {
    LayoutNode::Split {
        id: SplitId(1),
        axis,
        ratio,
        first: Box::new(first),
        second: Box::new(second),
    }
}

fn descriptor(id: PaneId) -> PaneSnapshot {
    PaneSnapshot {
        id,
        title: String::new(),
        kind: PaneKindSnapshot::Terminal,
        synchronized_input: false,
        bell: false,
        dead: false,
        dead_status: None,
        border_colour: None,
        active_border_colour: None,
        border_status_text: String::new(),
        mode: None,
        status: None,
        summary: PaneSummary::default(),
    }
}

fn window(layout: LayoutNode) -> WindowSnapshot {
    WindowSnapshot {
        id: WindowId(3),
        index: 0,
        name: String::new(),
        automatic_rename: false,
        active_pane: PaneId(1),
        zoomed_pane: None,
        panes: (1..=9)
            .map(|id| (PaneId(id), descriptor(PaneId(id))))
            .collect(),
        layout,
        layout_dump: String::new(),
        visible_layout_dump: String::new(),
        status_label: String::new(),
        activity: false,
        silence: false,
        pane_border_status: PaneBorderStatus::Off,
        pane_border_lines: PaneBorderLines::Single,
        pane_border_indicators: PaneBorderIndicators::Colour,
        pane_order: vec![PaneId(1), PaneId(2), PaneId(3)],
        pane_z_order: vec![PaneId(9), PaneId(8), PaneId(7)],
        floating: Vec::new(),
        modal: None,
        sx: 80,
        sy: 24,
        pane_base_index: 0,
    }
}

fn float(id: u64, x: i32, y: i32, sx: u16, sy: u16) -> FloatingPaneSnapshot {
    FloatingPaneSnapshot {
        pane: PaneId(id),
        xoff: x,
        yoff: y,
        sx,
        sy,
        visible: true,
        border_lines: PaneBorderLines::Single,
        border_status: PaneBorderStatus::Off,
    }
}

fn bits(values: [f32; 4]) -> Vec<u32> {
    values.into_iter().map(f32::to_bits).collect()
}

fn pixel_bits(rect: PixelRect) -> Vec<u32> {
    bits([rect.x, rect.y, rect.width, rect.height])
}

fn normalized(window: &WindowSnapshot) -> Vec<Value> {
    pane_rects(&window.layout)
        .into_iter()
        .map(|(pane, rect)| {
            json!([
                pane.0,
                bits([rect.x, rect.y, rect.width, rect.height]),
                [0, 0],
                false,
                false
            ])
        })
        .collect()
}

fn pixels(window: &WindowSnapshot, args: [f32; 6]) -> Vec<Value> {
    let [width, height, cell_width, cell_height, divider, header] = args;
    let layout = window
        .zoomed_pane
        .map_or_else(|| window.layout.clone(), LayoutNode::Pane);
    let mut output = pane_rects(&layout)
        .into_iter()
        .filter(|(pane, _)| window.panes.contains_key(pane))
        .map(|(pane, rect)| {
            let bounds = pane_box(
                PaneRect {
                    x: rect.x * width,
                    y: rect.y * height,
                    width: rect.width * width,
                    height: rect.height * height,
                },
                divider,
            );
            json!([
                pane.0,
                bits([bounds.x, bounds.y, bounds.width, bounds.height]),
                [0, 0],
                false,
                false
            ])
        })
        .collect::<Vec<_>>();
    for float in window
        .floating
        .iter()
        .rev()
        .filter(|float| float.visible && window.panes.contains_key(&float.pane))
    {
        let Some((placed, _)) = float_placement(float, (cell_width, cell_height), (width, height))
        else {
            continue;
        };
        let modal = window.modal.is_some_and(|modal| modal.pane == float.pane);
        let frame = if modal {
            placed
        } else {
            PipGeometry {
                window: (window.sx, window.sy),
                cell: (cell_width, cell_height),
                canvas: (width, height),
                lines: float.border_lines,
                header,
            }
            .frame(FloatCells::from(float))
        };
        let insets = if float.border_lines == PaneBorderLines::None {
            [0.0_f32, 0.0]
        } else {
            [cell_width, cell_height]
        };
        output.push(json!([
            float.pane.0,
            pixel_bits(frame),
            insets.map(f32::to_bits),
            true,
            modal
        ]));
    }
    output
}

fn case(name: &str, window: WindowSnapshot, args: [f32; 6]) -> Result<Value, Box<dyn Error>> {
    let floats = window
        .floating
        .iter()
        .map(|float| {
            let [width, height, cell_width, cell_height, _, header] = args;
            let clipped = float_pixels(
                FloatCells::from(float),
                float.border_lines != PaneBorderLines::None,
                (cell_width, cell_height),
                (width, height),
            )
            .map(|placed| json!([pixel_bits(placed.frame), placed.content.map(pixel_bits)]));
            let placed = float_placement(float, (cell_width, cell_height), (width, height))
                .map(|(frame, content)| json!([pixel_bits(frame), pixel_bits(content)]));
            let pip = PipGeometry {
                window: (window.sx, window.sy),
                cell: (cell_width, cell_height),
                canvas: (width, height),
                lines: float.border_lines,
                header,
            }
            .frame(FloatCells::from(float));
            json!({"clipped": clipped, "placed": placed, "pip": pixel_bits(pip)})
        })
        .collect::<Vec<_>>();
    let expected_normalized = normalized(&window);
    let expected_pixels = pixels(&window, args);
    let frame = encode_protocol_message(&ProtocolMessage::Event(Event {
        sequence: 1,
        payload: EventPayload::Snapshot(MuxSnapshot {
            generation: 1,
            focused_window: Some(window.id),
            sessions: vec![SessionSnapshot {
                id: SessionId(2),
                name: String::new(),
                active_window: window.id,
                windows: vec![window],
                viewers: Vec::new(),
            }],
        }),
    }))?;
    Ok(
        json!({"name": name, "frame": frame, "args": args.map(f32::to_bits), "normalized": expected_normalized, "pixels": expected_pixels, "floats": floats}),
    )
}

fn main() -> Result<(), Box<dyn Error>> {
    let normal = [1000.0, 800.0, 8.0, 16.0, 8.0, 28.0];
    let mut cases = vec![
        case("single", window(pane(1)), normal)?,
        case("empty", window(LayoutNode::Empty), normal)?,
        case(
            "nested-mixed",
            window(split(
                Axis::Horizontal,
                0.4,
                pane(1),
                split(Axis::Vertical, 0.25, pane(2), pane(3)),
            )),
            normal,
        )?,
        case(
            "nested-same-axis-fused",
            window(split(
                Axis::Horizontal,
                0.123_456_7,
                pane(1),
                split(Axis::Horizontal, 0.734_567_8, pane(2), pane(3)),
            )),
            [999.5, 801.25, 7.2, 15.3, 3.25, 27.5],
        )?,
    ];
    for (name, ratio) in [
        ("negative", -1.0),
        ("positive-overflow", 2.0),
        ("nan", f32::NAN),
        ("infinity", f32::INFINITY),
        ("negative-infinity", f32::NEG_INFINITY),
        ("negative-zero", -0.0),
        ("zero", 0.0),
        ("one", 1.0),
        ("smallest-subnormal", f32::from_bits(1)),
    ] {
        cases.push(case(
            name,
            window(split(Axis::Vertical, ratio, pane(1), pane(2))),
            normal,
        )?);
    }
    let mut deep = pane(1);
    for index in 2..=9 {
        deep = split(
            if index % 2 == 0 {
                Axis::Horizontal
            } else {
                Axis::Vertical
            },
            0.371_234_57,
            deep,
            pane(index),
        );
    }
    cases.push(case("deep-fused-composition", window(deep), normal)?);
    let mut large = window(pane(u64::MAX));
    large
        .panes
        .insert(PaneId(u64::MAX), descriptor(PaneId(u64::MAX)));
    cases.push(case("large-pane-id", large, normal)?);
    let mut missing = window(split(Axis::Horizontal, 0.5, pane(1), pane(42)));
    missing.zoomed_pane = Some(PaneId(2));
    cases.push(case("zoom-ignores-split", missing.clone(), normal)?);
    missing.zoomed_pane = Some(PaneId(42));
    cases.push(case("zoom-missing-pane", missing.clone(), normal)?);
    missing.zoomed_pane = None;
    cases.push(case("missing-layout-pane", missing, normal)?);
    let mut floating = window(split(Axis::Horizontal, 0.4, pane(1), pane(2)));
    floating.floating = vec![
        float(5, 30, 9, 20, 6),
        float(4, -3, 30, 10, 10),
        float(3, 10, 4, 20, 6),
    ];
    floating.floating[1].border_lines = PaneBorderLines::None;
    floating.modal = Some(ModalPaneSnapshot {
        pane: PaneId(5),
        capture_keys: true,
        close_on_click: true,
        close_on_cancel: true,
    });
    cases.push(case(
        "floats-modal-and-paint-order",
        floating.clone(),
        normal,
    )?);
    floating.floating[0].visible = false;
    floating.floating[2].xoff = 600;
    cases.push(case(
        "hidden-and-offscreen-floats",
        floating.clone(),
        normal,
    )?);
    floating.zoomed_pane = Some(PaneId(5));
    cases.push(case(
        "zoomed-float-plus-visible-overlays",
        floating,
        normal,
    )?);
    for (name, value, args) in [
        (
            "partial-negative-float",
            float(3, -3, 30, 10, 10),
            [400.0, 560.0, 8.0, 16.0, 8.0, 28.0],
        ),
        (
            "border-only-visible",
            float(3, -1, 1, 1, 1),
            [400.0, 560.0, 8.0, 16.0, 8.0, 28.0],
        ),
        ("zero-float-content", float(3, 1, 1, 0, 0), normal),
        ("float-larger-than-window", float(3, 1, 1, 90, 30), normal),
        (
            "float-larger-than-canvas",
            float(3, 1, 1, 20, 6),
            [100.0, 50.0, 8.0, 16.0, 8.0, 28.0],
        ),
        (
            "unmeasured-canvas",
            float(3, -2, 4, 20, 6),
            [0.0, 0.0, 8.0, 16.0, 8.0, 28.0],
        ),
        (
            "short-canvas",
            float(3, 10, 4, 20, 6),
            [400.0, 0.0, 8.0, 16.0, 8.0, 28.0],
        ),
        (
            "fractional-cells",
            float(3, 10, 4, 20, 6),
            [900.0, 700.0, 7.2, 15.3, 3.25, 27.5],
        ),
        (
            "large-header",
            float(3, 10, 4, 20, 6),
            [1000.0, 800.0, 8.0, 16.0, 8.0, 400.0],
        ),
    ] {
        let mut item = window(LayoutNode::Empty);
        item.floating.push(value);
        cases.push(case(name, item.clone(), args)?);
        item.modal = Some(ModalPaneSnapshot {
            pane: PaneId(3),
            capture_keys: false,
            close_on_click: false,
            close_on_cancel: false,
        });
        cases.push(case(&format!("modal-{name}"), item, args)?);
    }
    for (index, lines) in [
        PaneBorderLines::Single,
        PaneBorderLines::Double,
        PaneBorderLines::Heavy,
        PaneBorderLines::Simple,
        PaneBorderLines::Number,
        PaneBorderLines::Spaces,
        PaneBorderLines::None,
    ]
    .into_iter()
    .enumerate()
    {
        let mut item = window(LayoutNode::Empty);
        let mut value = float(3, 10, 4, 20, 6);
        value.border_lines = lines;
        item.floating.push(value);
        item.sx = 0;
        item.sy = 0;
        cases.push(case(
            &format!("unmeasured-window-border-{index}"),
            item,
            normal,
        )?);
    }
    println!(
        "{}",
        json!({"reference": "zz-client layout and floating geometry with desktop snapshot composition", "cases": cases})
    );
    Ok(())
}
