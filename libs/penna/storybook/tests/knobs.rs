use penna::IconName;
use penna_storybook::{Catalog, Knobs, creator};

static BOARD_APP: Catalog = Catalog {
    title: "board storybook",
    name: "board",
    storage: "board-storybook",
    init: penna::init,
    logo: IconName::Layers.path(),
    plates: &[creator::CONTROLS],
    stories: &[penna_storybook::FOUNDATION],
    knobs: &[],
    apply: |_, _| {},
    floating: &[],
};

#[test]
fn the_interface_theme_knob_takes_and_names_the_builtin_themes() {
    penna_storybook::install(&BOARD_APP);
    assert!(Knobs::parse("interface-theme=yaru").is_ok());
    let error = Knobs::parse("interface-theme=nope").expect_err("nope is no theme");
    assert_eq!(
        error,
        "interface-theme must be one of default, macos, yaru, not nope"
    );
}
