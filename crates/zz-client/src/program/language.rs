use super::{Look, Program};

pub(super) struct Language {
    pub(super) fences: &'static [&'static str],
    pub(super) name: &'static str,
    pub(super) program: &'static str,
}

pub(super) const MARKS: &[Program] = &[
    Program {
        id: "javascript",
        name: "JavaScript",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x00F0_DB4F,
            foreground: 0x0032_3330,
        },
    },
    Program {
        id: "kotlin",
        name: "Kotlin",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x00C7_11E1,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "c",
        name: "C",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x00A9_BACD,
            foreground: 0x001B_2A41,
        },
    },
    Program {
        id: "cplusplus",
        name: "C++",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x0000_4482,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "csharp",
        name: "C#",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x0068_217A,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "fsharp",
        name: "F#",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x0037_8BBA,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "objectivec",
        name: "Objective-C",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x000B_5A9D,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "lua",
        name: "Lua",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x0000_0080,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "haskell",
        name: "Haskell",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x005E_5185,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "elixir",
        name: "Elixir",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x0088_47B9,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "erlang",
        name: "Erlang",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x00A9_0533,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "scala",
        name: "Scala",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x00DE_3423,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "clojure",
        name: "Clojure",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x0058_81D8,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "ocaml",
        name: "OCaml",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x00F1_8803,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "r",
        name: "R",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x001F_65B7,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "julia",
        name: "Julia",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x0028_A745,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "perl",
        name: "Perl",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x0021_2177,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "groovy",
        name: "Groovy",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x0061_9CBC,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "solidity",
        name: "Solidity",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x0038_3838,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "html5",
        name: "HTML",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x00E5_4D26,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "css3",
        name: "CSS",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x003D_8FC6,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "sass",
        name: "Sass",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x00CC_6699,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "vuejs",
        name: "Vue",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x0041_B883,
            foreground: 0x0035_495E,
        },
    },
    Program {
        id: "svelte",
        name: "Svelte",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x00FF_3E00,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "graphql",
        name: "GraphQL",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x00E4_34AA,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "json",
        name: "JSON",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x0050_5050,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "yaml",
        name: "YAML",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x00CB_171E,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "toml",
        name: "TOML",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x009C_4221,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "xml",
        name: "XML",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x0000_5FAD,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "markdown",
        name: "Markdown",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x0000_0000,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "latex",
        name: "LaTeX",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x0000_0000,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "powershell",
        name: "PowerShell",
        commands: &[],
        icon: None,
        look: Look::Mark {
            background: 0x001E_2A3A,
            foreground: 0x00FF_FFFF,
        },
    },
];

pub(super) const LANGUAGES: &[Language] = &[
    Language {
        fences: &["rust", "rs"],
        name: "Rust",
        program: "rust",
    },
    Language {
        fences: &["python", "py", "python3", "py3", "pyi"],
        name: "Python",
        program: "python",
    },
    Language {
        fences: &["go", "golang"],
        name: "Go",
        program: "go",
    },
    Language {
        fences: &["typescript", "ts", "tsx", "mts", "cts"],
        name: "TypeScript",
        program: "typescript",
    },
    Language {
        fences: &["javascript", "js", "jsx", "mjs", "cjs", "node"],
        name: "JavaScript",
        program: "javascript",
    },
    Language {
        fences: &["java"],
        name: "Java",
        program: "java",
    },
    Language {
        fences: &["kotlin", "kt", "kts"],
        name: "Kotlin",
        program: "kotlin",
    },
    Language {
        fences: &["swift"],
        name: "Swift",
        program: "swift",
    },
    Language {
        fences: &["dart"],
        name: "Dart",
        program: "dart",
    },
    Language {
        fences: &["zig"],
        name: "Zig",
        program: "zig",
    },
    Language {
        fences: &["c", "h"],
        name: "C",
        program: "c",
    },
    Language {
        fences: &["cpp", "c++", "cc", "cxx", "hpp", "hh", "hxx"],
        name: "C++",
        program: "cplusplus",
    },
    Language {
        fences: &["csharp", "cs", "c#"],
        name: "C#",
        program: "csharp",
    },
    Language {
        fences: &["fsharp", "fs", "f#"],
        name: "F#",
        program: "fsharp",
    },
    Language {
        fences: &["objc", "objective-c", "objectivec", "objc++"],
        name: "Objective-C",
        program: "objectivec",
    },
    Language {
        fences: &["ruby", "rb"],
        name: "Ruby",
        program: "ruby",
    },
    Language {
        fences: &["php"],
        name: "PHP",
        program: "php",
    },
    Language {
        fences: &["lua", "luau"],
        name: "Lua",
        program: "lua",
    },
    Language {
        fences: &["haskell", "hs"],
        name: "Haskell",
        program: "haskell",
    },
    Language {
        fences: &["elixir", "ex", "exs", "heex"],
        name: "Elixir",
        program: "elixir",
    },
    Language {
        fences: &["erlang", "erl"],
        name: "Erlang",
        program: "erlang",
    },
    Language {
        fences: &["scala", "sc"],
        name: "Scala",
        program: "scala",
    },
    Language {
        fences: &["clojure", "clj", "cljs", "cljc", "edn"],
        name: "Clojure",
        program: "clojure",
    },
    Language {
        fences: &["ocaml", "ml", "mli"],
        name: "OCaml",
        program: "ocaml",
    },
    Language {
        fences: &["r"],
        name: "R",
        program: "r",
    },
    Language {
        fences: &["julia", "jl"],
        name: "Julia",
        program: "julia",
    },
    Language {
        fences: &["perl", "pl", "pm"],
        name: "Perl",
        program: "perl",
    },
    Language {
        fences: &["groovy", "gradle"],
        name: "Groovy",
        program: "groovy",
    },
    Language {
        fences: &["solidity", "sol"],
        name: "Solidity",
        program: "solidity",
    },
    Language {
        fences: &["html", "htm", "xhtml"],
        name: "HTML",
        program: "html5",
    },
    Language {
        fences: &["css"],
        name: "CSS",
        program: "css3",
    },
    Language {
        fences: &["scss", "sass"],
        name: "Sass",
        program: "sass",
    },
    Language {
        fences: &["vue"],
        name: "Vue",
        program: "vuejs",
    },
    Language {
        fences: &["svelte"],
        name: "Svelte",
        program: "svelte",
    },
    Language {
        fences: &["graphql", "gql"],
        name: "GraphQL",
        program: "graphql",
    },
    Language {
        fences: &["json", "jsonc", "json5", "jsonl", "ndjson"],
        name: "JSON",
        program: "json",
    },
    Language {
        fences: &["yaml", "yml"],
        name: "YAML",
        program: "yaml",
    },
    Language {
        fences: &["toml"],
        name: "TOML",
        program: "toml",
    },
    Language {
        fences: &["xml", "svg", "plist", "xsd", "xsl"],
        name: "XML",
        program: "xml",
    },
    Language {
        fences: &["markdown", "md", "mdx"],
        name: "Markdown",
        program: "markdown",
    },
    Language {
        fences: &["latex", "tex"],
        name: "LaTeX",
        program: "latex",
    },
    Language {
        fences: &[
            "bash",
            "sh",
            "shell",
            "zsh",
            "fish",
            "ksh",
            "console",
            "shellsession",
            "sh-session",
        ],
        name: "Shell",
        program: "terminal",
    },
    Language {
        fences: &["powershell", "ps1", "pwsh", "psm1"],
        name: "PowerShell",
        program: "powershell",
    },
    Language {
        fences: &["dockerfile", "docker", "containerfile"],
        name: "Dockerfile",
        program: "docker",
    },
    Language {
        fences: &["nix"],
        name: "Nix",
        program: "nix",
    },
    Language {
        fences: &["hcl", "terraform", "tf", "tfvars"],
        name: "Terraform",
        program: "terraform",
    },
    Language {
        fences: &["sql", "psql", "pgsql", "postgres", "postgresql", "plpgsql"],
        name: "SQL",
        program: "postgres",
    },
    Language {
        fences: &["sqlite"],
        name: "SQLite",
        program: "sqlite",
    },
    Language {
        fences: &["mysql"],
        name: "MySQL",
        program: "mysql",
    },
    Language {
        fences: &["cmake"],
        name: "CMake",
        program: "cmake",
    },
    Language {
        fences: &["just", "justfile"],
        name: "just",
        program: "just",
    },
    Language {
        fences: &["d2"],
        name: "D2",
        program: "d2",
    },
    Language {
        fences: &["diff", "patch"],
        name: "Diff",
        program: "gitchanges",
    },
    Language {
        fences: &["gitconfig", "gitignore", "gitattributes"],
        name: "Git",
        program: "git",
    },
];
