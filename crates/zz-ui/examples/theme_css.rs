use std::{env, path::PathBuf};

use penna::theme_css;

fn main() -> std::io::Result<()> {
    let out = env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("design/tokens"), PathBuf::from);
    zz_ui::register();
    for path in theme_css::write_tokens(&out, &zz_ui::roles::TONED)? {
        println!("{}", path.display());
    }
    Ok(())
}
