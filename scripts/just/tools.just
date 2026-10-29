import "settings.just"

set working-directory := "../.."

[doc("Writes the zz-workspace skill from this tree's guide; the CLI answers tools itself.")]
skill:
    @cargo build -q -p zz-cli --bin zz_cli && target/debug/zz_cli tools --skill > .agents/skills/zz-workspace/SKILL.md
