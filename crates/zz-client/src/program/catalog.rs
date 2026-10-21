use super::{Look, Program};

pub(super) const PROGRAMS: &[Program] = &[
    Program {
        id: "amp",
        name: "Amp",
        commands: &["amp"],
        icon: None,
        look: Look::Mark {
            background: 0x00DF_DFC1,
            foreground: 0x0009_1C1E,
        },
    },
    Program {
        id: "antigravity",
        name: "Antigravity",
        commands: &["antigravity", "agy"],
        icon: None,
        look: Look::Mark {
            background: 0x00F9_F9F9,
            foreground: 0x0031_87FE,
        },
    },
    Program {
        id: "apple-container",
        name: "Apple Container",
        commands: &["apple-container", "container"],
        icon: None,
        look: Look::Mark {
            background: 0x0076_7B91,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "azure",
        name: "Azure",
        commands: &["azure", "az"],
        icon: None,
        look: Look::Mark {
            background: 0x0000_CEF9,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "bat",
        name: "Bat",
        commands: &["bat"],
        icon: None,
        look: Look::Mark {
            background: 0x0030_36A4,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "boundary",
        name: "Boundary",
        commands: &["boundary"],
        icon: None,
        look: Look::Mark {
            background: 0x00F2_4C53,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "brew",
        name: "Homebrew",
        commands: &["brew"],
        icon: None,
        look: Look::Mark {
            background: 0x00FF_AC0B,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "browser",
        name: "Browser",
        commands: &["browser"],
        icon: Some("globe"),
        look: Look::Mark {
            background: 0x0000_C4FD,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "btop",
        name: "btop",
        commands: &["btop"],
        icon: None,
        look: Look::Mark {
            background: 0x0027_0301,
            foreground: 0x00FF_5147,
        },
    },
    Program {
        id: "buck",
        name: "Buck",
        commands: &["buck", "buck2"],
        icon: None,
        look: Look::Mark {
            background: 0x00BD_4731,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "buildkite",
        name: "Buildkite",
        commands: &["buildkite", "bk"],
        icon: None,
        look: Look::Mark {
            background: 0x0000_D078,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "bun",
        name: "Bun",
        commands: &["bun", "bunx"],
        icon: None,
        look: Look::Mark {
            background: 0x00FB_F0DF,
            foreground: 0x0014_151A,
        },
    },
    Program {
        id: "butler",
        name: "Git Butler",
        commands: &["butler", "but"],
        icon: None,
        look: Look::Mark {
            background: 0x00BA_E8E8,
            foreground: 0x0000_0000,
        },
    },
    Program {
        id: "caffeinate",
        name: "caffeinate",
        commands: &["caffeinate"],
        icon: Some("cup"),
        look: Look::Mark {
            background: 0x002B_2D31,
            foreground: 0x007E_E787,
        },
    },
    Program {
        id: "clang",
        name: "Clang",
        commands: &["clang", "clang++", "clang-cpp", "clang-cl"],
        icon: None,
        look: Look::Mark {
            background: 0x00CB_F0FF,
            foreground: 0x0000_2C38,
        },
    },
    Program {
        id: "claude",
        name: "Claude Code",
        commands: &["claude"],
        icon: None,
        look: Look::Mark {
            background: 0x00E6_683D,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "cloudflare",
        name: "Cloudflare",
        commands: &["cloudflare", "wrangler"],
        icon: None,
        look: Look::Mark {
            background: 0x00F6_821E,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "coder",
        name: "Coder",
        commands: &["coder"],
        icon: None,
        look: Look::Mark {
            background: 0x00FF_FFFF,
            foreground: 0x0000_0000,
        },
    },
    Program {
        id: "codex",
        name: "Codex",
        commands: &["codex"],
        icon: None,
        look: Look::Mark {
            background: 0x00F8_F8F8,
            foreground: 0x0000_0000,
        },
    },
    Program {
        id: "consul",
        name: "Consul",
        commands: &["consul"],
        icon: None,
        look: Look::Mark {
            background: 0x00E0_3875,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "copilot",
        name: "Copilot",
        commands: &["copilot", "copilot/npm-loader.js"],
        icon: None,
        look: Look::Mark {
            background: 0x00FA_FAFA,
            foreground: 0x002A_56FE,
        },
    },
    Program {
        id: "crush",
        name: "Crush",
        commands: &["crush"],
        icon: None,
        look: Look::Mark {
            background: 0x00AB_5AFA,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "cursor",
        name: "Cursor",
        commands: &["cursor", "agent"],
        icon: None,
        look: Look::Mark {
            background: 0x00F7_F7F4,
            foreground: 0x0026_251D,
        },
    },
    Program {
        id: "d2",
        name: "D2",
        commands: &["d2"],
        icon: None,
        look: Look::Mark {
            background: 0x00F3_F5FF,
            foreground: 0x0024_5DF5,
        },
    },
    Program {
        id: "dart",
        name: "Dart",
        commands: &["dart", "pub"],
        icon: None,
        look: Look::Mark {
            background: 0x0000_BAFC,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "devin",
        name: "Devin",
        commands: &["devin"],
        icon: None,
        look: Look::Mark {
            background: 0x00FC_FCFC,
            foreground: 0x0000_0000,
        },
    },
    Program {
        id: "docker",
        name: "Docker",
        commands: &["docker"],
        icon: None,
        look: Look::Mark {
            background: 0x0000_62FF,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "droid",
        name: "Droid",
        commands: &["droid"],
        icon: None,
        look: Look::Mark {
            background: 0x0000_0000,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "emacs",
        name: "Emacs",
        commands: &["emacs"],
        icon: None,
        look: Look::Mark {
            background: 0x007E_55B3,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "flutter",
        name: "Flutter",
        commands: &["flutter"],
        icon: None,
        look: Look::Mark {
            background: 0x005F_C8F9,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "fx",
        name: "fx",
        commands: &["fx"],
        icon: None,
        look: Look::Mark {
            background: 0x0011_1111,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "gemini",
        name: "Gemini",
        commands: &["gemini"],
        icon: None,
        look: Look::Mark {
            background: 0x00F9_F9F9,
            foreground: 0x003A_88FE,
        },
    },
    Program {
        id: "git",
        name: "Git",
        commands: &["git"],
        icon: None,
        look: Look::Mark {
            background: 0x00F0_EFE6,
            foreground: 0x00F4_4D27,
        },
    },
    Program {
        id: "gitchanges",
        name: "Git changes",
        commands: &["gitchanges", "lazygit"],
        icon: Some("git-branch"),
        look: Look::Mark {
            background: 0x003F_B950,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "github",
        name: "GitHub",
        commands: &["github"],
        icon: None,
        look: Look::Mark {
            background: 0x00F9_F9F9,
            foreground: 0x0000_0000,
        },
    },
    Program {
        id: "gnu",
        name: "Gnu",
        commands: &[
            "gnu",
            "make",
            "gmake",
            "configure",
            "libtool",
            "gcc",
            "g++",
            "cpp",
        ],
        icon: None,
        look: Look::Mark {
            background: 0x00FF_FFFF,
            foreground: 0x0000_0000,
        },
    },
    Program {
        id: "go",
        name: "Go",
        commands: &["go"],
        icon: None,
        look: Look::Mark {
            background: 0x0000_AFDC,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "grok",
        name: "Grok",
        commands: &["grok"],
        icon: None,
        look: Look::Mark {
            background: 0x0000_0000,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "heroku",
        name: "Heroku",
        commands: &["heroku"],
        icon: None,
        look: Look::Mark {
            background: 0x0074_32B6,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "homey",
        name: "Homey",
        commands: &["homey"],
        icon: None,
        look: Look::Mark {
            background: 0x0000_90FE,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "htop",
        name: "htop",
        commands: &["htop"],
        icon: None,
        look: Look::Mark {
            background: 0x002C_312D,
            foreground: 0x002C_C52D,
        },
    },
    Program {
        id: "hugging-face",
        name: "Hugging Face",
        commands: &["hugging-face", "hf"],
        icon: None,
        look: Look::Mark {
            background: 0x00FF_D000,
            foreground: 0x0056_2800,
        },
    },
    Program {
        id: "hunk",
        name: "Hunk",
        commands: &["hunk"],
        icon: None,
        look: Look::Mark {
            background: 0x00F3_F1EA,
            foreground: 0x001E_7A35,
        },
    },
    Program {
        id: "hx",
        name: "Helix",
        commands: &["hx"],
        icon: None,
        look: Look::Mark {
            background: 0x002C_1376,
            foreground: 0x00D4_E3FE,
        },
    },
    Program {
        id: "inspector",
        name: "Inspector",
        commands: &["inspector"],
        icon: Some("inspector"),
        look: Look::Mark {
            background: 0x0036_6FFF,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "jj",
        name: "JJ",
        commands: &["jj"],
        icon: None,
        look: Look::Mark {
            background: 0x0000_A2E5,
            foreground: 0x0004_2652,
        },
    },
    Program {
        id: "jupyter",
        name: "Jupyter",
        commands: &[
            "jupyter",
            "jupyter-notebook",
            "jupyter-lab",
            "jupyter-console",
            "jupyter-qtconsole",
        ],
        icon: None,
        look: Look::Mark {
            background: 0x00F3_7626,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "k9s",
        name: "K9s",
        commands: &["k9s"],
        icon: None,
        look: Look::Mark {
            background: 0x0016_56FD,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "kakoune",
        name: "Kakoune",
        commands: &["kakoune", "kak"],
        icon: None,
        look: Look::Mark {
            background: 0x00E1_785D,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "kiro",
        name: "Kiro",
        commands: &["kiro", "kiro-cli"],
        icon: None,
        look: Look::Mark {
            background: 0x00A6_37FE,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "kube",
        name: "Kubernetes",
        commands: &["kube", "kubectl", "kubectx", "kubens"],
        icon: None,
        look: Look::Mark {
            background: 0x0032_6CE5,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "llama-cpp",
        name: "Llama",
        commands: &["llama-cpp", "llama"],
        icon: None,
        look: Look::Mark {
            background: 0x00FF_FFFF,
            foreground: 0x0000_0000,
        },
    },
    Program {
        id: "log",
        name: "log",
        commands: &["log"],
        icon: Some("list"),
        look: Look::Mark {
            background: 0x002B_2D31,
            foreground: 0x007E_E787,
        },
    },
    Program {
        id: "mastra",
        name: "Mastra",
        commands: &["mastra"],
        icon: None,
        look: Look::Mark {
            background: 0x0017_1717,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "meta",
        name: "Meta",
        commands: &["meta", "muse"],
        icon: None,
        look: Look::Mark {
            background: 0x00FF_FFFF,
            foreground: 0x0000_72FF,
        },
    },
    Program {
        id: "mistral",
        name: "Mistral",
        commands: &["mistral", "vibe", "vibe cli"],
        icon: None,
        look: Look::Mark {
            background: 0x00FF_7900,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "modal",
        name: "Modal",
        commands: &["modal"],
        icon: None,
        look: Look::Mark {
            background: 0x0018_1818,
            foreground: 0x0094_F17E,
        },
    },
    Program {
        id: "mole",
        name: "Mole",
        commands: &["mole", "mo"],
        icon: None,
        look: Look::Mark {
            background: 0x003B_573E,
            foreground: 0x00FA_F4E6,
        },
    },
    Program {
        id: "moon",
        name: "Moon",
        commands: &["moon"],
        icon: None,
        look: Look::Mark {
            background: 0x0002_2949,
            foreground: 0x0068_D3E8,
        },
    },
    Program {
        id: "neovim",
        name: "Neovim",
        commands: &["neovim", "nvim"],
        icon: None,
        look: Look::Mark {
            background: 0x0000_A3D7,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "nix",
        name: "Nix",
        commands: &[
            "nix",
            "nh",
            "nom",
            "nix-build",
            "nix-env",
            "nix-shell",
            "nix-store",
        ],
        icon: None,
        look: Look::Mark {
            background: 0x0042_70BD,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "node",
        name: "Node",
        commands: &["node"],
        icon: None,
        look: Look::Mark {
            background: 0x0040_873F,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "nomad",
        name: "Nomad",
        commands: &["nomad"],
        icon: None,
        look: Look::Mark {
            background: 0x0000_CA8E,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "ollama",
        name: "Ollama",
        commands: &["ollama"],
        icon: None,
        look: Look::Mark {
            background: 0x00F9_F9F9,
            foreground: 0x0011_1111,
        },
    },
    Program {
        id: "opencode",
        name: "OpenCode",
        commands: &["opencode"],
        icon: None,
        look: Look::Mark {
            background: 0x0033_3333,
            foreground: 0x00F9_F9F9,
        },
    },
    Program {
        id: "oxide",
        name: "Oxide",
        commands: &["oxide"],
        icon: None,
        look: Look::Mark {
            background: 0x0020_4D3B,
            foreground: 0x0000_D992,
        },
    },
    Program {
        id: "packer",
        name: "Packer",
        commands: &["packer"],
        icon: None,
        look: Look::Mark {
            background: 0x0002_A8EF,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "pi",
        name: "Pi",
        commands: &[
            "pi",
            "pi-coding-agent/dist/cli.js",
            "pi-coding-agent/dist/bundle/cli.js",
        ],
        icon: None,
        look: Look::Mark {
            background: 0x00EC_E7E4,
            foreground: 0x0000_0000,
        },
    },
    Program {
        id: "pnpm",
        name: "pnpm",
        commands: &["pnpm"],
        icon: None,
        look: Look::Mark {
            background: 0x00FF_A900,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "podman",
        name: "Podman",
        commands: &["podman"],
        icon: None,
        look: Look::Mark {
            background: 0x00CF_BDDD,
            foreground: 0x0050_3E68,
        },
    },
    Program {
        id: "poolside-icon",
        name: "Poolside",
        commands: &["poolside-icon", "pool"],
        icon: None,
        look: Look::Mark {
            background: 0x0041_37FF,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "postgres",
        name: "Postgres",
        commands: &["postgres", "psql"],
        icon: None,
        look: Look::Mark {
            background: 0x001A_6995,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "prime-agent",
        name: "Prime Agent",
        commands: &["prime-agent"],
        icon: None,
        look: Look::Mark {
            background: 0x0015_1515,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "proto",
        name: "Proto",
        commands: &["proto"],
        icon: None,
        look: Look::Mark {
            background: 0x0001_0F1C,
            foreground: 0x009A_72FF,
        },
    },
    Program {
        id: "proton-drive",
        name: "Proton Drive",
        commands: &["proton-drive", "./proton-drive"],
        icon: None,
        look: Look::Mark {
            background: 0x00EB_4FCC,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "proton-pass",
        name: "Proton Pass",
        commands: &["proton-pass", "pass-cli"],
        icon: None,
        look: Look::Mark {
            background: 0x00EF_BC9D,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "python",
        name: "Python",
        commands: &[
            "python",
            "python2",
            "python2.7",
            "python3",
            "python3.10",
            "python3.11",
            "python3.12",
            "python3.13",
            "python3.14",
            "python3.15",
        ],
        icon: None,
        look: Look::Mark {
            background: 0x0044_83B5,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "railway",
        name: "Railway",
        commands: &["railway"],
        icon: None,
        look: Look::Mark {
            background: 0x0013_111D,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "render",
        name: "Render",
        commands: &["render"],
        icon: None,
        look: Look::Mark {
            background: 0x00FF_FFFF,
            foreground: 0x0000_0000,
        },
    },
    Program {
        id: "ruby",
        name: "Ruby",
        commands: &["ruby"],
        icon: None,
        look: Look::Mark {
            background: 0x00EA_0000,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "rust",
        name: "Rust",
        commands: &["rust", "rustc", "cargo"],
        icon: None,
        look: Look::Mark {
            background: 0x00EB_EBEB,
            foreground: 0x0000_0000,
        },
    },
    Program {
        id: "shopify",
        name: "Shopify",
        commands: &["shopify"],
        icon: None,
        look: Look::Mark {
            background: 0x0089_C129,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "sleep",
        name: "sleep",
        commands: &["sleep"],
        icon: Some("moon"),
        look: Look::Mark {
            background: 0x002B_2D31,
            foreground: 0x007E_E787,
        },
    },
    Program {
        id: "sqlite",
        name: "SQLite",
        commands: &["sqlite", "sqlite3"],
        icon: None,
        look: Look::Mark {
            background: 0x0000_82D2,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "stripe",
        name: "Stripe",
        commands: &["stripe"],
        icon: None,
        look: Look::Mark {
            background: 0x0057_39FF,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "swift",
        name: "Swift",
        commands: &["swift"],
        icon: None,
        look: Look::Mark {
            background: 0x00FF_3F18,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "tailscale",
        name: "Tailscale",
        commands: &["tailscale"],
        icon: None,
        look: Look::Mark {
            background: 0x001F_1E1E,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "terminal",
        name: "Terminal",
        commands: &["terminal"],
        icon: Some("terminal"),
        look: Look::Mark {
            background: 0x002B_2D31,
            foreground: 0x007E_E787,
        },
    },
    Program {
        id: "terraform",
        name: "Terraform",
        commands: &["terraform"],
        icon: None,
        look: Look::Mark {
            background: 0x00A0_67DA,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "top",
        name: "top",
        commands: &["top"],
        icon: Some("cpu"),
        look: Look::Mark {
            background: 0x002B_2D31,
            foreground: 0x007E_E787,
        },
    },
    Program {
        id: "uv",
        name: "uv",
        commands: &["uv", "uvx"],
        icon: None,
        look: Look::Mark {
            background: 0x00DE_5FE9,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "vagrant",
        name: "Vagrant",
        commands: &["vagrant"],
        icon: None,
        look: Look::Mark {
            background: 0x0032_66E9,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "vault",
        name: "Vault",
        commands: &["vault"],
        icon: None,
        look: Look::Mark {
            background: 0x00FF_CF25,
            foreground: 0x0000_0000,
        },
    },
    Program {
        id: "vercel",
        name: "Vercel",
        commands: &["vercel", "vc"],
        icon: None,
        look: Look::Mark {
            background: 0x0011_1111,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "vim",
        name: "Vim",
        commands: &["vim"],
        icon: None,
        look: Look::Mark {
            background: 0x0019_953F,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "vite",
        name: "Vite",
        commands: &["vite"],
        icon: None,
        look: Look::Mark {
            background: 0x0091_35FF,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "waypoint",
        name: "Waypoint",
        commands: &["waypoint"],
        icon: None,
        look: Look::Mark {
            background: 0x0014_C6CB,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "yazi",
        name: "Yazi",
        commands: &["yazi"],
        icon: None,
        look: Look::Mark {
            background: 0x00F5_8B37,
            foreground: 0x00FF_F79E,
        },
    },
    Program {
        id: "zig",
        name: "Zig",
        commands: &["zig", "maker"],
        icon: None,
        look: Look::Mark {
            background: 0x00F7_A41E,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "ansible",
        name: "Ansible",
        commands: &[
            "ansible",
            "ansible-playbook",
            "ansible-galaxy",
            "ansible-vault",
        ],
        icon: None,
        look: Look::Mark {
            background: 0x00EE_0000,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "argocd",
        name: "Argo CD",
        commands: &["argocd"],
        icon: None,
        look: Look::Mark {
            background: 0x00EF_7B4D,
            foreground: 0x0011_1111,
        },
    },
    Program {
        id: "aws",
        name: "AWS CLI",
        commands: &["aws"],
        icon: None,
        look: Look::Art {
            background: 0x00F9_F9F9,
        },
    },
    Program {
        id: "biome",
        name: "Biome",
        commands: &["biome"],
        icon: None,
        look: Look::Art {
            background: 0x00F9_F9F9,
        },
    },
    Program {
        id: "bottom",
        name: "bottom",
        commands: &["btm"],
        icon: None,
        look: Look::Art {
            background: 0x00F9_F9F9,
        },
    },
    Program {
        id: "clickhouse",
        name: "ClickHouse CLI",
        commands: &["clickhouse", "clickhouse-client", "clickhouse-local"],
        icon: None,
        look: Look::Mark {
            background: 0x00FF_CC01,
            foreground: 0x0011_1111,
        },
    },
    Program {
        id: "cmake",
        name: "CMake",
        commands: &["cmake", "ctest", "cpack"],
        icon: None,
        look: Look::Mark {
            background: 0x0006_4F8C,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "composer",
        name: "Composer",
        commands: &["composer"],
        icon: None,
        look: Look::Mark {
            background: 0x0088_5630,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "curl",
        name: "curl",
        commands: &["curl"],
        icon: None,
        look: Look::Art {
            background: 0x00F9_F9F9,
        },
    },
    Program {
        id: "deno",
        name: "Deno",
        commands: &["deno"],
        icon: None,
        look: Look::Art {
            background: 0x00F9_F9F9,
        },
    },
    Program {
        id: "dotnet",
        name: ".NET",
        commands: &["dotnet"],
        icon: None,
        look: Look::Mark {
            background: 0x0051_2BD4,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "duckdb",
        name: "DuckDB CLI",
        commands: &["duckdb"],
        icon: None,
        look: Look::Mark {
            background: 0x00FF_F000,
            foreground: 0x0011_1111,
        },
    },
    Program {
        id: "eslint",
        name: "ESLint",
        commands: &["eslint"],
        icon: None,
        look: Look::Mark {
            background: 0x004B_32C3,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "fastfetch",
        name: "Fastfetch",
        commands: &["fastfetch"],
        icon: None,
        look: Look::Art {
            background: 0x00F9_F9F9,
        },
    },
    Program {
        id: "ffmpeg",
        name: "FFmpeg",
        commands: &["ffmpeg", "ffprobe", "ffplay"],
        icon: None,
        look: Look::Mark {
            background: 0x0000_7808,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "firebase",
        name: "Firebase CLI",
        commands: &["firebase"],
        icon: None,
        look: Look::Mark {
            background: 0x00DD_2C00,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "flyio",
        name: "Fly.io CLI",
        commands: &["fly", "flyctl"],
        icon: None,
        look: Look::Mark {
            background: 0x0024_175B,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "gitlab",
        name: "GitLab CLI",
        commands: &["glab"],
        icon: None,
        look: Look::Mark {
            background: 0x00FC_6D26,
            foreground: 0x0011_1111,
        },
    },
    Program {
        id: "googlecloud",
        name: "Google Cloud CLI",
        commands: &["gcloud", "bq", "gsutil"],
        icon: None,
        look: Look::Mark {
            background: 0x0042_85F4,
            foreground: 0x0011_1111,
        },
    },
    Program {
        id: "gradle",
        name: "Gradle",
        commands: &["gradle", "gradlew"],
        icon: None,
        look: Look::Mark {
            background: 0x0002_303A,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "helm",
        name: "Helm",
        commands: &["helm"],
        icon: None,
        look: Look::Mark {
            background: 0x000F_1689,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "java",
        name: "Java",
        commands: &["java", "javac", "jshell"],
        icon: None,
        look: Look::Art {
            background: 0x00F9_F9F9,
        },
    },
    Program {
        id: "jest",
        name: "Jest",
        commands: &["jest"],
        icon: None,
        look: Look::Mark {
            background: 0x00C2_1325,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "jq",
        name: "jq",
        commands: &["jq"],
        icon: None,
        look: Look::Art {
            background: 0x00F9_F9F9,
        },
    },
    Program {
        id: "just",
        name: "Just",
        commands: &["just"],
        icon: None,
        look: Look::Mark {
            background: 0x0000_0000,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "kind",
        name: "kind",
        commands: &["kind"],
        icon: None,
        look: Look::Art {
            background: 0x00F9_F9F9,
        },
    },
    Program {
        id: "maven",
        name: "Apache Maven",
        commands: &["mvn", "mvnw"],
        icon: None,
        look: Look::Mark {
            background: 0x00C7_1A36,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "minikube",
        name: "minikube",
        commands: &["minikube"],
        icon: None,
        look: Look::Art {
            background: 0x00F9_F9F9,
        },
    },
    Program {
        id: "mise",
        name: "mise",
        commands: &["mise"],
        icon: None,
        look: Look::Art {
            background: 0x00F9_F9F9,
        },
    },
    Program {
        id: "mongodb",
        name: "MongoDB Shell",
        commands: &["mongosh"],
        icon: None,
        look: Look::Mark {
            background: 0x0047_A248,
            foreground: 0x0011_1111,
        },
    },
    Program {
        id: "mysql",
        name: "MySQL CLI",
        commands: &["mysql", "mysqladmin", "mysqldump"],
        icon: None,
        look: Look::Mark {
            background: 0x0044_79A1,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "netlify",
        name: "Netlify CLI",
        commands: &["netlify", "ntl"],
        icon: None,
        look: Look::Mark {
            background: 0x0000_C7B7,
            foreground: 0x0011_1111,
        },
    },
    Program {
        id: "npm",
        name: "npm",
        commands: &["npm", "npx"],
        icon: None,
        look: Look::Mark {
            background: 0x00CB_3837,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "nx",
        name: "Nx",
        commands: &["nx"],
        icon: None,
        look: Look::Mark {
            background: 0x0014_3055,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "opentofu",
        name: "OpenTofu",
        commands: &["tofu"],
        icon: None,
        look: Look::Mark {
            background: 0x00FF_DA18,
            foreground: 0x0011_1111,
        },
    },
    Program {
        id: "php",
        name: "PHP",
        commands: &["php"],
        icon: None,
        look: Look::Mark {
            background: 0x0077_7BB4,
            foreground: 0x0011_1111,
        },
    },
    Program {
        id: "playwright",
        name: "Playwright",
        commands: &["playwright"],
        icon: None,
        look: Look::Art {
            background: 0x00F9_F9F9,
        },
    },
    Program {
        id: "poetry",
        name: "Poetry",
        commands: &["poetry"],
        icon: None,
        look: Look::Mark {
            background: 0x0060_A5FA,
            foreground: 0x0011_1111,
        },
    },
    Program {
        id: "prettier",
        name: "Prettier",
        commands: &["prettier"],
        icon: None,
        look: Look::Art {
            background: 0x00F9_F9F9,
        },
    },
    Program {
        id: "pulumi",
        name: "Pulumi",
        commands: &["pulumi"],
        icon: None,
        look: Look::Mark {
            background: 0x008A_3391,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "pytest",
        name: "Pytest",
        commands: &["pytest", "py.test"],
        icon: None,
        look: Look::Art {
            background: 0x00F9_F9F9,
        },
    },
    Program {
        id: "rclone",
        name: "Rclone",
        commands: &["rclone"],
        icon: None,
        look: Look::Art {
            background: 0x00F9_F9F9,
        },
    },
    Program {
        id: "redis",
        name: "Redis CLI",
        commands: &["redis-cli"],
        icon: None,
        look: Look::Mark {
            background: 0x00FF_4438,
            foreground: 0x0011_1111,
        },
    },
    Program {
        id: "ruff",
        name: "Ruff",
        commands: &["ruff"],
        icon: None,
        look: Look::Mark {
            background: 0x00D7_FF64,
            foreground: 0x0011_1111,
        },
    },
    Program {
        id: "skaffold",
        name: "Skaffold",
        commands: &["skaffold"],
        icon: None,
        look: Look::Mark {
            background: 0x002A_A2D6,
            foreground: 0x0011_1111,
        },
    },
    Program {
        id: "starship",
        name: "Starship",
        commands: &["starship"],
        icon: None,
        look: Look::Art {
            background: 0x00F9_F9F9,
        },
    },
    Program {
        id: "supabase",
        name: "Supabase CLI",
        commands: &["supabase"],
        icon: None,
        look: Look::Mark {
            background: 0x003F_CF8E,
            foreground: 0x0011_1111,
        },
    },
    Program {
        id: "terragrunt",
        name: "Terragrunt",
        commands: &["terragrunt"],
        icon: None,
        look: Look::Art {
            background: 0x00F9_F9F9,
        },
    },
    Program {
        id: "tmux",
        name: "tmux",
        commands: &["tmux"],
        icon: None,
        look: Look::Art {
            background: 0x00F9_F9F9,
        },
    },
    Program {
        id: "turborepo",
        name: "Turborepo",
        commands: &["turbo"],
        icon: None,
        look: Look::Mark {
            background: 0x00FF_1E56,
            foreground: 0x0011_1111,
        },
    },
    Program {
        id: "typescript",
        name: "TypeScript",
        commands: &["tsc"],
        icon: None,
        look: Look::Mark {
            background: 0x0031_78C6,
            foreground: 0x00FF_FFFF,
        },
    },
    Program {
        id: "vitest",
        name: "Vitest",
        commands: &["vitest"],
        icon: None,
        look: Look::Art {
            background: 0x00F9_F9F9,
        },
    },
    Program {
        id: "yarn",
        name: "Yarn",
        commands: &["yarn", "yarnpkg"],
        icon: None,
        look: Look::Art {
            background: 0x00F9_F9F9,
        },
    },
    Program {
        id: "zellij",
        name: "Zellij",
        commands: &["zellij"],
        icon: None,
        look: Look::Art {
            background: 0x00F9_F9F9,
        },
    },
    Program {
        id: "zoxide",
        name: "zoxide",
        commands: &["zoxide"],
        icon: None,
        look: Look::Art {
            background: 0x00F9_F9F9,
        },
    },
];
