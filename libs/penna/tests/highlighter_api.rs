use std::time::Duration;

use penna::highlighter::{
    HighlightTheme, InputEdit, LanguageConfig, LanguageRegistry, Point, Rope, SyntaxHighlighter,
};

#[test]
fn highlighter_api_matches_with_and_without_tree_sitter() {
    let text = Rope::from("fn main() {}");
    let mut highlighter = SyntaxHighlighter::new("rust");
    assert!(highlighter.update(None, &text, None));
    assert_eq!(highlighter.text().to_string(), "fn main() {}");
    assert!(!highlighter.is_empty());

    let edited = Rope::from("fn main() { }");
    let edit = InputEdit {
        start_byte: 11,
        old_end_byte: 11,
        new_end_byte: 12,
        start_position: Point::new(0, 11),
        old_end_position: Point::new(0, 11),
        new_end_position: Point::new(0, 12),
    };
    assert!(highlighter.update(Some(edit), &edited, Some(Duration::MAX)));
    assert_eq!(highlighter.text().to_string(), "fn main() { }");
    let _ = highlighter.styles(&(0..edited.len()), &HighlightTheme::default_dark());
}

#[test]
fn language_config_builds_the_same_with_and_without_tree_sitter() {
    let config = LanguageConfig::plain("x");
    assert_eq!(config.name.as_ref(), "x");
    assert!(config.language.is_none());
    assert!(config.injection_languages.is_empty());
    assert!(config.highlights.is_empty());
    assert!(format!("{config:?}").contains("LanguageConfig"));

    let registry = LanguageRegistry::singleton();
    registry.register("highlighter-api-x", &config);
    let found = registry
        .language("highlighter-api-x")
        .expect("the registration should be found");
    assert_eq!(found.name.as_ref(), "x");
    assert_eq!(
        SyntaxHighlighter::new("highlighter-api-x")
            .language()
            .as_ref(),
        "x"
    );
}
