use penna::{Kind, SurfaceRole, roles};

#[test]
#[should_panic(expected = "app role `button` takes the id of a penna kit role")]
fn an_app_role_with_a_kit_id_panics_naming_it() {
    static CLASH: [SurfaceRole; 1] = [SurfaceRole::new(
        "button",
        Kind::Button,
        "Buttons",
        "The app's own buttons",
    )];
    roles::register(&CLASH);
}

#[test]
#[should_panic(expected = "app role `Note Big` can't be named in a theme file")]
fn an_app_role_a_theme_file_cannot_name_panics() {
    static MISSPELLED: [SurfaceRole; 1] = [SurfaceRole::new(
        "Note Big",
        Kind::Card,
        "Big notes",
        "Cards that hold a long note",
    )];
    roles::register(&MISSPELLED);
}
