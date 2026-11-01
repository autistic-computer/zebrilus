use pele::{RenderOptions, Theme, render};

const FLOWCHART: &str = "flowchart LR\n subgraph G\n A[One] --> B[Two]\n end";
const SEQUENCE: &str =
    "sequenceDiagram\n A->>B: hi\n activate B\n loop again\n B-->>A: back\n end\n deactivate B";

fn tinted() -> RenderOptions {
    RenderOptions {
        theme: Theme {
            border: "#555555".into(),
            surface: "#202020".into(),
            surface_alt: "#303030".into(),
            node: Some("#dde6ff".into()),
            node_border: Some("#3355dd".into()),
            ..Theme::default()
        },
        ..RenderOptions::default()
    }
}

#[test]
fn nodes_take_their_own_fill_and_border_while_frames_keep_the_neutral_ones() {
    let flowchart = render(FLOWCHART, &tinted()).unwrap().svg;
    assert!(flowchart.contains(
        "<rect x=\"8\" y=\"8\" width=\"220.46\" height=\"108\" fill=\"#303030\" fill-opacity=\"0.5\" stroke=\"#555555\" rx=\"4\"/>"
    ));
    assert!(flowchart.contains(
        "<rect x=\"-33.12\" y=\"-24\" width=\"66.24\" height=\"48\" fill=\"#dde6ff\" stroke=\"#3355dd\" rx=\"4\"/>"
    ));
    assert!(!flowchart.contains("#202020"));

    let sequence = render(SEQUENCE, &tinted()).unwrap().svg;
    assert!(sequence.contains(
        "<rect x=\"-32\" y=\"0\" width=\"64\" height=\"44\" rx=\"4\" fill=\"#dde6ff\" stroke=\"#3355dd\"/>"
    ));
    assert!(sequence.contains(
        "<rect x=\"99\" y=\"87\" width=\"10\" height=\"84\" rx=\"2\" fill=\"#dde6ff\" stroke=\"#3355dd\"/>"
    ));
    assert!(sequence.contains(
        "<rect x=\"-20\" y=\"99\" width=\"144\" height=\"72\" rx=\"4\" fill=\"none\" stroke=\"#555555\"/>"
    ));
}

#[test]
fn nodes_fall_back_to_the_surface_and_border() {
    let flowchart = render(FLOWCHART, &RenderOptions::default()).unwrap().svg;
    assert!(flowchart.contains(
        "<rect x=\"-33.12\" y=\"-24\" width=\"66.24\" height=\"48\" fill=\"#f3f3f3\" stroke=\"#8a8a8a\" rx=\"4\"/>"
    ));
    let options: RenderOptions =
        serde_json::from_str("{\"theme\":{\"node\":\"#dde6ff\",\"nodeBorder\":\"#3355dd\"}}")
            .unwrap();
    assert_eq!(options.theme.node_fill(), "#dde6ff");
    assert_eq!(options.theme.node_stroke(), "#3355dd");
    assert_eq!(options.theme.surface, "#f3f3f3");
}
