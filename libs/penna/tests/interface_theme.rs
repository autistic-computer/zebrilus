use penna::{
    Fill, Kind, Theme, ThemeMode,
    foundation::interface_theme::{self, InterfaceTheme},
    to_hex,
};

#[test]
fn an_old_theme_file_loads_as_the_look_with_its_name() {
    let ids = interface_theme::builtin_themes()
        .iter()
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    assert_eq!(ids, interface_theme::BUILTIN_THEMES);

    let old = interface_theme::parse_theme(
        r##"{"name":"Neon","materials":{"radius":3,"outline":0,"primary":"accent"},"roles":{"button":{"fill":"none","outline":true},"floating":{"fill":"solid","shadow":"none"},"sparkle":{}},"light":{"background":"#ff0000"}}"##,
    )
    .unwrap();
    assert_eq!(
        old,
        InterfaceTheme {
            name: "Neon".to_owned(),
            ..InterfaceTheme::default()
        }
    );
    let mut kit = Theme::default();
    kit.mode = ThemeMode::Light;
    old.apply(&mut kit);
    let button = kit.style(Kind::Button.role());
    let floating = kit.style(Kind::Floating.role());
    assert_eq!((button.fill, button.outline), (Fill::Tint, false));
    assert_eq!(floating.fill, Fill::Glass);
    assert_eq!(to_hex(kit.background), "#f6f6f3");
    assert_eq!(f32::from(kit.radius), 12.0);
    assert_eq!(f32::from(kit.outline), 1.0);
}
