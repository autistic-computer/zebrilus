use penna::{Kind, SurfaceRole};

static NOTES: [SurfaceRole; 1] = [SurfaceRole::new(
    "app.note",
    Kind::Card,
    "Notes",
    "Cards that hold a note",
)];

static LAYERS: [(&str, &str); 1] = [("macos", r#"{"roles":{"app_note":{"outline":false}}}"#)];

#[test]
fn a_misspelled_role_in_a_layer_is_named_without_the_order_hint() {
    let read = std::panic::catch_unwind(|| penna::register(&NOTES, &[], &LAYERS));
    let message = *read.unwrap_err().downcast::<String>().unwrap();
    assert!(
        message.starts_with(
            "theme macos with the app's theme layer does not parse: unknown role `app_note`"
        ),
        "{message}"
    );
    assert!(message.contains("did you mean `app.note`?"), "{message}");
    assert!(!message.contains("call penna::register"), "{message}");
}
