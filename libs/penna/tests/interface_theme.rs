use penna::{Shape, Theme, interface_theme, roles};

#[test]
fn builtins_parse_and_a_theme_reaches_every_kind_of_role() {
    let ids = interface_theme::builtin_themes()
        .iter()
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    assert_eq!(ids, interface_theme::BUILTIN_THEMES);

    let error = interface_theme::parse_theme(r#"{"roles":{"app.card":{}}}"#).unwrap_err();
    assert!(error.contains("unknown role `app.card`"), "{error}");

    let theme = interface_theme::parse_theme(
        r#"{"roles":{"pane.chip":{"shape":"square"},"chip.floating":{"shape":"square"},"chrome.button":{"shape":"pill"}}}"#,
    )
    .unwrap();
    assert_eq!(
        theme.roles.keys().collect::<Vec<_>>(),
        ["chip.floating", "chrome.button"]
    );
    let mut kit = Theme::default();
    theme.apply(&mut kit);
    assert_eq!(kit.style(roles::CHIP_FLOATING).shape, Shape::Square);
    assert_eq!(kit.style(roles::CHROME_BUTTON).shape, Shape::Pill);
}
