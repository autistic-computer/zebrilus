use std::{env, fs, path::PathBuf};

use penna::ThemeMode;
use zz_ui::{interface_theme::builtin_themes, theme_css};

fn main() -> std::io::Result<()> {
    let out = env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("design/kit/tokens"), PathBuf::from);
    fs::create_dir_all(&out)?;
    for (id, theme) in builtin_themes() {
        let icons = out.join(format!("{id}-icons.css"));
        fs::write(
            &icons,
            theme_css::icons(&theme_css::kit_theme(theme, ThemeMode::Dark)),
        )?;
        println!("{}", icons.display());
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let kit = theme_css::kit_theme(theme, mode);
            let path = out.join(format!("{id}-{}.css", mode.name()));
            fs::write(&path, theme_css::stylesheet(&theme.name, &kit))?;
            println!("{}", path.display());
        }
    }
    Ok(())
}
