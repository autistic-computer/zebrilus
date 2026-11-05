use ala::{
    AnyElement, App, Context, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    Styled as _, TestAppContext, Window, div, px, size,
};
use penna::{
    IconName, Kind, StyledExt as _, Surface, SurfaceRole,
    foundation::interface_theme::{self, ControlRadius, Glass, Tint},
};
use penna_storybook::{Catalog, Section, creator, story::stateless};
use serde_json::{Value, json};

const BOARD_PLATE: Section = Section {
    id: "desk",
    name: "Board",
    summary: "Two screens on the arrangement, each with its name.",
    build: |_, cx| stateless(board, cx),
};

fn board(_: &mut Window, cx: &mut App) -> AnyElement {
    let screen = |name: &'static str, cx: &App| {
        div()
            .size(px(120.0))
            .p(px(8.0))
            .surface(Surface::new(SurfaceRole::SCREEN_TILE), cx)
            .child(
                div()
                    .surface(Surface::new(SurfaceRole::CHIP), cx)
                    .child(name),
            )
    };
    div()
        .flex()
        .gap(px(16.0))
        .w(px(320.0))
        .p(px(16.0))
        .surface(Surface::new(SurfaceRole::WELL), cx)
        .child(screen("left", cx))
        .child(screen("right", cx))
        .into_any_element()
}

static BOARD_APP: Catalog = Catalog {
    title: "board storybook",
    name: "desk",
    storage: "board-storybook",
    init: penna::init,
    logo: IconName::Layers.path(),
    plates: &[creator::CONTROLS, BOARD_PLATE],
    stories: &[penna_storybook::FOUNDATION],
    knobs: &[],
    apply: |_, _| {},
    floating: &[],
};

fn install() {
    penna_storybook::install(&BOARD_APP);
}

fn tree() -> Vec<Value> {
    let regions = serde_json::from_str::<Vec<Value>>(&penna_storybook::roles_json()).unwrap();
    regions
        .into_iter()
        .flat_map(|region| region["roles"].as_array().cloned().unwrap_or_default())
        .collect()
}

fn role<'a>(tree: &'a [Value], id: &str) -> &'a Value {
    tree.iter()
        .find(|role| role["id"] == id)
        .unwrap_or_else(|| panic!("the role tree has no {id}"))
}

fn draw(cx: &mut TestAppContext) {
    cx.run_until_parked();
    cx.update(|cx| {
        for window in cx.windows() {
            window
                .update(cx, |_, window, cx| window.draw(cx).clear(cx))
                .unwrap();
        }
    });
}

#[test]
fn the_app_catalog_puts_the_creator_first_with_its_plates() {
    install();
    assert_eq!(BOARD_APP.problems(), Vec::<String>::new());
    let stories = serde_json::from_str::<Vec<Value>>(&penna_storybook::stories_json()).unwrap();
    assert_eq!(stories[0]["id"], creator::ID);
    let sections = stories[0]["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|section| section["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(sections, ["controls", "desk"]);
}

#[test]
fn the_roles_a_plate_draws_show_in_the_tree_under_their_parents() {
    install();
    let tree = tree();
    let well = role(&tree, "well");
    assert_eq!(well["kind"], "card");
    assert_eq!(well["group"], Kind::Card.role().label());
    assert_eq!(well["style"]["shadow"], "none");
    assert_eq!(well["style"]["fill"], "tint");
    let screen = role(&tree, "tile.screen");
    assert_eq!(screen["style"]["shadow"], "none");
    assert_eq!(screen["style"]["fill"], "solid");
    let chip = role(&tree, "chip");
    assert_eq!(chip["group"], SurfaceRole::BADGE.label());
    assert_eq!(chip["style"]["shape"], "pill");
}

#[ala::test]
fn a_knob_edit_lands_in_the_export_and_parses_back(cx: &mut TestAppContext) {
    install();
    cx.update(penna::init);
    cx.update(|cx| {
        penna_storybook::set_knobs(
            "radius=8&tint-color=orange&glass=none&control-radius=full",
            cx,
        )
    })
    .unwrap();
    let exported = penna_storybook::theme_file_json();
    let theme = interface_theme::parse_theme(&exported).unwrap();
    assert_eq!(
        (theme.radius, theme.control_radius, theme.glass.clone()),
        (8.0, Some(ControlRadius::Full), Some(Glass::None))
    );
    assert_eq!(theme.tint, Tint::named("orange").unwrap());
    let knobs =
        serde_json::from_str::<Value>(&penna_storybook::import_theme(&exported).unwrap()).unwrap();
    assert_eq!(knobs["light-tint-color"], "#ad4f00");
    assert_eq!(knobs["control-radius"], "full");
    let tree = tree();
    assert_eq!(role(&tree, "tile.screen")["patch"], json!({}));
}

#[test]
fn import_keeps_the_name_and_ignores_what_a_theme_no_longer_sets() {
    install();
    let theme = json!({
        "name": "Old",
        "materials": { "radius": 3 },
        "roles": { "tile.screen": { "fill": "tint" } },
    });
    let knobs =
        serde_json::from_str::<Value>(&penna_storybook::import_theme(&theme.to_string()).unwrap())
            .unwrap();
    assert_eq!(knobs["name"], "Old");
    assert_eq!(knobs["radius"], 12.0);
    assert!(knobs.get("roles").is_none());
}

#[ala::test]
fn the_app_plate_reports_the_roles_it_draws(cx: &mut TestAppContext) {
    install();
    cx.update(penna::init);
    let creator = penna_storybook::find(creator::ID).unwrap();
    cx.update(|cx| penna_storybook::show_story(creator, cx))
        .unwrap();
    draw(cx);
    let drawn = serde_json::from_str::<Value>(&penna_storybook::drawn_roles_json()).unwrap();
    let desk = drawn["desk"].as_array().unwrap();
    for id in ["well", "tile.screen", "chip"] {
        assert!(desk.iter().any(|role| role == id), "{id} in {desk:?}");
    }
    let controls = drawn["controls"].as_array().unwrap();
    assert!(controls.iter().all(|role| role != "well"));
}

struct Groups;

impl Render for Groups {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let tile = |name: &'static str| {
            div()
                .debug_selector(move || name.to_owned())
                .h(px(40.0))
                .into_any_element()
        };
        creator::tiles(
            window,
            vec![
                vec![tile("first")],
                vec![tile("second")],
                vec![tile("third")],
                vec![tile("fourth")],
            ],
        )
    }
}

#[ala::test]
fn a_fourth_group_of_tiles_stacks_under_the_first_on_a_wide_page(cx: &mut TestAppContext) {
    let (_, cx) = cx.add_window_view(|_, _| Groups);
    cx.simulate_resize(size(px(creator::WIDE + 360.0), px(900.0)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
    let mut bounds = |name| {
        cx.debug_bounds(name)
            .unwrap_or_else(|| panic!("{name} was not drawn"))
    };
    let (first, second, third, fourth) = (
        bounds("first"),
        bounds("second"),
        bounds("third"),
        bounds("fourth"),
    );
    assert!(first.left() < second.left() && second.left() < third.left());
    assert_eq!(first.top(), third.top());
    assert_eq!(fourth.left(), first.left());
    assert!(fourth.top() > first.bottom());
}
