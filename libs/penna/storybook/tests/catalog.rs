use penna::IconName;
use penna_storybook::{Catalog, PENNA, catalog, install};

static BOARD_APP: Catalog = Catalog {
    title: "board storybook",
    name: "board",
    storage: "board-storybook",
    logo: IconName::Layers.path(),
    ..PENNA
};

#[test]
fn reading_the_catalog_before_installing_one_keeps_it_open() {
    assert_eq!(catalog().name, "penna");
    install(&BOARD_APP);
    install(&BOARD_APP);
    assert_eq!(catalog().name, "board");
    assert_eq!(catalog().stories().count(), PENNA.stories().count());
}
