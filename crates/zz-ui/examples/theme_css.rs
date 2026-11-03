use std::{env, path::PathBuf};

use penna::foundation::theme_css;

fn main() -> std::io::Result<()> {
    let out = env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("design/tokens"), PathBuf::from);
    for path in theme_css::write_tokens(&out, &penna::foundation::roles::TONED)? {
        println!("{}", path.display());
    }
    Ok(())
}
