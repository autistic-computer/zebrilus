use penna::{Kind, SurfaceRole};

static NOTES: [SurfaceRole; 1] = [SurfaceRole::new(
    "note",
    Kind::Card,
    "Notes",
    "Cards that hold a note",
)];

static LAYERS: [(&str, &str); 1] = [("macos", r#"{"roles":{"note":{"outlin":false}}}"#)];

#[test]
#[should_panic(expected = "theme macos with the app's theme layer does not parse: ")]
fn a_bad_theme_layer_panics_at_register() {
    penna::register(&NOTES, &[], &LAYERS);
}
