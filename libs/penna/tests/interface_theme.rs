use penna::{Kind, Shape, SurfaceRole, Theme, interface_theme, roles};

static APP_ROLES: [SurfaceRole; 1] = [SurfaceRole::new(
    "app.card",
    Kind::Card,
    "App cards",
    "Cards the app draws",
)];

static APP_THEMES: [(&str, &str); 3] = [
    ("macos", r#"{"roles":{"app.card":{"shape":"square"}}}"#),
    (
        "plain",
        r#"{"name":"Plain","roles":{"app.card":{"shape":"square"}}}"#,
    ),
    ("default", r#"{"roles":{"app.card":{"outline":false}}}"#),
];

#[test]
fn builtins_parse_with_the_kit_roles_and_app_layers_add_the_apps() {
    let sources = [
        include_str!("../themes/default.json"),
        include_str!("../themes/macos.json"),
        include_str!("../themes/yaru.json"),
    ];
    let own = sources
        .map(|source| interface_theme::parse_theme(source).expect("parses with the kit's roles"));

    let source = r#"{"roles":{"app.card":{"shape":"square"},"chrome.button":{"shape":"pill"}}}"#;
    let error = interface_theme::parse_theme(source).unwrap_err();
    assert!(error.contains("unknown role `app.card`"), "{error}");

    roles::register(&APP_ROLES);
    interface_theme::register(&APP_THEMES);
    assert_eq!(roles::find("app.card"), Some(APP_ROLES[0]));
    assert!(roles::all().any(|role| role == roles::WINDOW_CONTROLS));

    let ids = interface_theme::builtin_themes()
        .iter()
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    assert_eq!(ids, ["default", "macos", "yaru", "plain"]);
    assert_eq!(
        interface_theme::BUILTIN_THEMES,
        ["default", "macos", "yaru"]
    );
    for (id, own) in interface_theme::BUILTIN_THEMES.into_iter().zip(&own) {
        let layered = interface_theme::builtin_theme(id).unwrap();
        let mut kit_only = layered.clone();
        kit_only.roles.remove("app.card");
        assert_eq!(&kit_only, own, "{id}");
    }
    let default = interface_theme::default_theme();
    assert_eq!(default.name, "Default");
    assert_eq!(
        default.roles["app.card"],
        penna::StylePatch::EMPTY.outline(false)
    );
    assert!(
        !interface_theme::builtin_theme("yaru")
            .unwrap()
            .roles
            .contains_key("app.card")
    );
    assert_eq!(
        interface_theme::builtin_theme("plain").unwrap().name,
        "Plain"
    );

    let theme = interface_theme::parse_theme(source).unwrap();
    let mut kit = Theme::default();
    theme.apply(&mut kit);
    assert_eq!(kit.style(APP_ROLES[0]).shape, Shape::Square);
    assert_eq!(kit.style(roles::CHROME_BUTTON).shape, Shape::Pill);

    let mut kit = Theme::default();
    interface_theme::builtin_theme("macos")
        .unwrap()
        .apply(&mut kit);
    assert_eq!(kit.style(APP_ROLES[0]).shape, Shape::Square);
}
