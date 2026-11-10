use std::{
    collections::HashMap,
    fmt,
    sync::{LazyLock, Mutex, MutexGuard, PoisonError},
};

use ala::SharedString;
use tree_sitter_language::LanguageFn;

#[cfg(feature = "tree-sitter")]
use super::Language;

#[derive(Clone)]
#[non_exhaustive]
pub struct LanguageConfig {
    pub name: SharedString,
    pub language: Option<LanguageFn>,
    pub injection_languages: Vec<SharedString>,
    pub highlights: SharedString,
    pub injections: SharedString,
    pub locals: SharedString,
}

impl LanguageConfig {
    pub fn new(
        name: impl Into<SharedString>,
        language: LanguageFn,
        injection_languages: Vec<SharedString>,
        highlights: &str,
        injections: &str,
        locals: &str,
    ) -> Self {
        Self {
            name: name.into(),
            language: Some(language),
            injection_languages,
            highlights: highlights.to_string().into(),
            injections: injections.to_string().into(),
            locals: locals.to_string().into(),
        }
    }

    #[cfg(feature = "tree-sitter")]
    pub fn grammar(&self) -> Option<tree_sitter::Language> {
        self.language.map(tree_sitter::Language::new)
    }

    pub fn plain(name: impl Into<SharedString>) -> Self {
        Self {
            name: name.into(),
            language: None,
            injection_languages: Vec::new(),
            highlights: SharedString::default(),
            injections: SharedString::default(),
            locals: SharedString::default(),
        }
    }
}

impl fmt::Debug for LanguageConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LanguageConfig")
            .field("name", &self.name)
            .field("grammar", &self.language.is_some())
            .finish_non_exhaustive()
    }
}

pub struct LanguageRegistry {
    languages: Mutex<HashMap<SharedString, LanguageConfig>>,
}

impl LanguageRegistry {
    pub fn singleton() -> &'static Self {
        static INSTANCE: LazyLock<LanguageRegistry> = LazyLock::new(|| LanguageRegistry {
            languages: Mutex::new(HashMap::new()),
        });
        &INSTANCE
    }

    pub fn register(&self, lang: &str, config: &LanguageConfig) {
        self.entries().insert(lang.into(), config.clone());
    }

    pub fn language(&self, name: &str) -> Option<LanguageConfig> {
        let mut entries = self.entries();
        if let Some(config) = entries.get(name) {
            return Some(config.clone());
        }
        let (bundled, config) = bundled(name)?;
        Some(entries.entry(bundled.into()).or_insert_with(config).clone())
    }

    fn entries(&self) -> MutexGuard<'_, HashMap<SharedString, LanguageConfig>> {
        self.languages
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(feature = "tree-sitter")]
fn bundled(name: &str) -> Option<(&'static str, impl FnOnce() -> LanguageConfig)> {
    let language = Language::from_name(name)?;
    Some((language.name(), move || language.config()))
}

#[cfg(not(feature = "tree-sitter"))]
fn bundled(_name: &str) -> Option<(&'static str, fn() -> LanguageConfig)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_round_trips_a_registration() {
        let registry = LanguageRegistry::singleton();
        assert!(registry.language("registry-round-trip").is_none());

        registry.register(
            "registry-round-trip",
            &LanguageConfig::plain("Registry round trip"),
        );

        let config = registry
            .language("registry-round-trip")
            .expect("the registration should be found");
        assert_eq!(config.name.as_ref(), "Registry round trip");
        assert!(config.language.is_none());
        assert!(registry.language("zig").is_none());
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn registry_contains_the_supported_grammar_set() {
        let registry = LanguageRegistry::singleton();
        for language in ["json", "markdown", "rust", "toml"] {
            assert!(registry.language(language).is_some(), "{language}");
        }
        assert!(registry.language("sql").is_none());
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn aliases_resolve_to_canonical_languages() {
        let registry = LanguageRegistry::singleton();
        assert_eq!(registry.language("rs").unwrap().name.as_ref(), "rust");
        assert_eq!(registry.language("md").unwrap().name.as_ref(), "markdown");
    }
}
