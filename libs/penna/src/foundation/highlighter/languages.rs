use std::borrow::Cow;

use ala::SharedString;
use tree_sitter_language::LanguageFn;

use super::LanguageConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Plain,
    Bash,
    C,
    Css,
    Diff,
    Go,
    Html,
    JavaScript,
    Json,
    Markdown,
    MarkdownInline,
    Python,
    Rust,
    Toml,
    Tsx,
    TypeScript,
    Yaml,
}

impl From<Language> for SharedString {
    fn from(language: Language) -> Self {
        language.name().into()
    }
}

impl Language {
    pub fn all() -> impl Iterator<Item = Self> {
        [
            Self::Plain,
            Self::Bash,
            Self::C,
            Self::Css,
            Self::Diff,
            Self::Go,
            Self::Html,
            Self::JavaScript,
            Self::Json,
            Self::Markdown,
            Self::MarkdownInline,
            Self::Python,
            Self::Rust,
            Self::Toml,
            Self::Tsx,
            Self::TypeScript,
            Self::Yaml,
        ]
        .into_iter()
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Plain => "text",
            Self::Bash => "bash",
            Self::C => "c",
            Self::Css => "css",
            Self::Diff => "diff",
            Self::Go => "go",
            Self::Html => "html",
            Self::JavaScript => "javascript",
            Self::Json => "json",
            Self::Markdown => "markdown",
            Self::MarkdownInline => "markdown_inline",
            Self::Python => "python",
            Self::Rust => "rust",
            Self::Toml => "toml",
            Self::Tsx => "tsx",
            Self::TypeScript => "typescript",
            Self::Yaml => "yaml",
        }
    }

    pub fn from_str(name: &str) -> Self {
        Self::from_name(name).unwrap_or(Self::Plain)
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "text" | "plain" | "plaintext" => Some(Self::Plain),
            "bash" | "sh" | "shell" | "shellscript" | "zsh" => Some(Self::Bash),
            "c" | "h" => Some(Self::C),
            "css" => Some(Self::Css),
            "diff" | "patch" | "udiff" => Some(Self::Diff),
            "go" | "golang" => Some(Self::Go),
            "html" | "htm" => Some(Self::Html),
            "javascript" | "js" | "jsx" | "mjs" | "cjs" => Some(Self::JavaScript),
            "json" | "jsonc" => Some(Self::Json),
            "markdown" | "md" | "mdx" => Some(Self::Markdown),
            "markdown_inline" | "markdown-inline" => Some(Self::MarkdownInline),
            "python" | "py" | "python3" => Some(Self::Python),
            "rust" | "rs" => Some(Self::Rust),
            "toml" => Some(Self::Toml),
            "tsx" => Some(Self::Tsx),
            "typescript" | "ts" | "mts" | "cts" => Some(Self::TypeScript),
            "yaml" | "yml" => Some(Self::Yaml),
            _ => None,
        }
    }

    fn injection_languages(self) -> Vec<SharedString> {
        match self {
            Self::Markdown => ["markdown_inline", "rust", "json", "toml", "html"]
                .into_iter()
                .map(Into::into)
                .collect(),
            Self::Rust => vec!["rust".into()],
            _ => Vec::new(),
        }
    }

    pub(super) fn config(self) -> LanguageConfig {
        let (language, highlights, injections) = match self {
            Self::Plain => return LanguageConfig::plain(self.name()),
            Self::Bash => upstream(
                tree_sitter_bash::LANGUAGE,
                &[tree_sitter_bash::HIGHLIGHT_QUERY],
            ),
            Self::C => upstream(tree_sitter_c::LANGUAGE, &[tree_sitter_c::HIGHLIGHT_QUERY]),
            Self::Css => upstream(
                tree_sitter_css::LANGUAGE,
                &[tree_sitter_css::HIGHLIGHTS_QUERY],
            ),
            Self::Diff => (
                tree_sitter_diff::LANGUAGE,
                include_str!("languages/diff/highlights.scm").into(),
                "",
            ),
            Self::Go => upstream(
                tree_sitter_go::LANGUAGE,
                &[tree_sitter_go::HIGHLIGHTS_QUERY],
            ),
            Self::Html => (
                tree_sitter_html::LANGUAGE,
                tree_sitter_html::HIGHLIGHTS_QUERY.into(),
                "",
            ),
            Self::JavaScript => upstream(
                tree_sitter_javascript::LANGUAGE,
                &[
                    tree_sitter_javascript::HIGHLIGHT_QUERY,
                    tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
                ],
            ),
            Self::Json => (
                tree_sitter_json::LANGUAGE,
                include_str!("languages/json/highlights.scm").into(),
                "",
            ),
            Self::Markdown => (
                tree_sitter_md::LANGUAGE,
                include_str!("languages/markdown/highlights.scm").into(),
                include_str!("languages/markdown/injections.scm"),
            ),
            Self::MarkdownInline => (
                tree_sitter_md::INLINE_LANGUAGE,
                include_str!("languages/markdown_inline/highlights.scm").into(),
                "",
            ),
            Self::Python => upstream(
                tree_sitter_python::LANGUAGE,
                &[tree_sitter_python::HIGHLIGHTS_QUERY],
            ),
            Self::Rust => (
                tree_sitter_rust::LANGUAGE,
                include_str!("languages/rust/highlights.scm").into(),
                include_str!("languages/rust/injections.scm"),
            ),
            Self::Toml => (
                tree_sitter_toml_ng::LANGUAGE,
                tree_sitter_toml_ng::HIGHLIGHTS_QUERY.into(),
                "",
            ),
            Self::Tsx => upstream(
                tree_sitter_typescript::LANGUAGE_TSX,
                &[
                    tree_sitter_javascript::HIGHLIGHT_QUERY,
                    tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
                    tree_sitter_typescript::HIGHLIGHTS_QUERY,
                ],
            ),
            Self::TypeScript => upstream(
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
                &[
                    tree_sitter_javascript::HIGHLIGHT_QUERY,
                    tree_sitter_typescript::HIGHLIGHTS_QUERY,
                ],
            ),
            Self::Yaml => upstream(
                tree_sitter_yaml::LANGUAGE,
                &[tree_sitter_yaml::HIGHLIGHTS_QUERY],
            ),
        };

        LanguageConfig::new(
            self.name(),
            language,
            self.injection_languages(),
            &highlights,
            injections,
            "",
        )
    }
}

fn upstream(
    language: LanguageFn,
    queries: &[&str],
) -> (LanguageFn, Cow<'static, str>, &'static str) {
    let source = queries.concat();
    let Ok(query) = tree_sitter::Query::new(&tree_sitter::Language::new(language), &source) else {
        return (language, source.into(), "");
    };
    let mut bounds: Vec<usize> = (0..query.pattern_count())
        .map(|pattern| query.start_byte_for_pattern(pattern))
        .collect();
    bounds.push(source.len());
    let patterns: Vec<&str> = bounds
        .windows(2)
        .rev()
        .map(|bound| &source[bound[0]..bound[1]])
        .collect();
    (language, patterns.join("\n").into(), "")
}

#[cfg(test)]
mod tests {
    use super::super::SyntaxHighlighter;
    use super::*;
    use ropey::Rope;

    #[test]
    fn aliases_and_unknown_language_are_stable() {
        assert_eq!(Language::from_str("rs"), Language::Rust);
        assert_eq!(Language::from_str("md"), Language::Markdown);
        assert_eq!(Language::from_str("unknown"), Language::Plain);
    }

    #[test]
    fn each_added_grammar_highlights_a_keyword_and_a_literal() {
        let cases = [
            (
                "python",
                "def f(): return \"hi\"",
                [("def", "keyword"), ("\"hi\"", "string")],
            ),
            (
                "javascript",
                "const a = \"hi\";",
                [("const", "keyword"), ("\"hi\"", "string")],
            ),
            (
                "typescript",
                "let a: number = 1;",
                [("let", "keyword"), ("1", "number")],
            ),
            (
                "tsx",
                "const a = <b>{1}</b>;",
                [("const", "keyword"), ("1", "number")],
            ),
            (
                "sh",
                "if true; then echo \"hi\"; fi",
                [("if", "keyword"), ("\"hi\"", "string")],
            ),
            ("yml", "n: 1", [("n", "property"), ("1", "number")]),
            (
                "diff",
                "-a\n+b",
                [("-a", "diff.minus"), ("+b", "diff.plus")],
            ),
            (
                "go",
                "func f() string { return \"hi\" }",
                [("func", "keyword"), ("\"hi\"", "string")],
            ),
            (
                "c",
                "int f(void) { return 0; }",
                [("return", "keyword"), ("0", "number")],
            ),
            (
                "css",
                "@media screen { a { width: 1px; } }",
                [("@media", "keyword"), ("1px", "number")],
            ),
        ];
        for (language, sample, expected) in cases {
            let mut highlighter = SyntaxHighlighter::new(language);
            highlighter.update(None, &Rope::from(sample), None);
            let found: Vec<(&str, String)> = highlighter
                .highlights(0..sample.len())
                .into_iter()
                .map(|(range, name)| (&sample[range], name.to_string()))
                .collect();
            for (text, name) in expected {
                assert!(
                    found.contains(&(text, name.to_owned())),
                    "{language}: {text:?} as {name} in {found:?}"
                );
            }
        }
    }
}
