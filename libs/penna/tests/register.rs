use penna::{Kind, SurfaceRole, interface_theme, roles};

static NOTES: [SurfaceRole; 1] = [SurfaceRole::new(
    "note",
    Kind::Card,
    "Notes",
    "Cards that hold a note",
)];

static OTHER: [SurfaceRole; 1] = [SurfaceRole::new(
    "other",
    Kind::Card,
    "Others",
    "Cards that hold something else",
)];

static LAYERS: [(&str, &str); 1] = [("default", r#"{"roles":{"note":{"outline":false}}}"#)];

#[test]
fn registering_the_same_roles_again_passes() {
    roles::register(&NOTES);
    roles::register(&NOTES);
    assert_eq!(roles::find("note"), Some(NOTES[0]));
}

#[test]
#[should_panic(expected = "penna::roles::register was already called with a different list")]
fn registering_other_roles_later_panics() {
    roles::register(&NOTES);
    roles::register(&OTHER);
}

#[test]
#[should_panic(expected = "call penna::register before penna::init and before reading any theme")]
fn theme_layers_registered_after_a_theme_was_read_panic() {
    let _ = interface_theme::default_theme();
    interface_theme::register(&LAYERS);
}
