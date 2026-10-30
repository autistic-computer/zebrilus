use std::{env, path::PathBuf};

use penna::theme_css;

fn main() -> std::io::Result<()> {
    let out = env::args().nth(1).map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web/tokens"),
        PathBuf::from,
    );
    for path in theme_css::write_tokens(&out, &[])? {
        println!("{}", path.display());
    }
    Ok(())
}
