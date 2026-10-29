use penna::{Kind, Shape, SurfaceRole, Theme, interface_theme, roles};

static APP_ROLES: [SurfaceRole; 1] = [SurfaceRole::new(
    "app.card",
    Kind::Card,
    "App cards",
    "Cards the app draws",
)];

#[test]
fn app_roles_register_and_the_default_theme_parses() {
    let source = r#"{"roles":{"app.card":{"shape":"square"},"chrome.button":{"shape":"pill"}}}"#;
    let error = interface_theme::parse_theme(source).unwrap_err();
    assert!(error.contains("unknown role `app.card`"), "{error}");

    roles::register(&APP_ROLES);
    assert_eq!(roles::find("app.card"), Some(APP_ROLES[0]));
    assert!(roles::all().any(|role| role == roles::WINDOW_CONTROLS));

    let default = interface_theme::parse_theme(include_str!("../themes/default.json")).unwrap();
    assert_eq!(&default, interface_theme::default_theme());
    assert_eq!(default.name, "Default");

    let theme = interface_theme::parse_theme(source).unwrap();
    let mut kit = Theme::default();
    theme.apply(&mut kit);
    assert_eq!(kit.style(APP_ROLES[0]).shape, Shape::Square);
    assert_eq!(kit.style(roles::CHROME_BUTTON).shape, Shape::Pill);
}
