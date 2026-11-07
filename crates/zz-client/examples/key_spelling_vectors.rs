use std::{
    error::Error,
    fs::File,
    io::{BufWriter, Write},
};

use serde_json::json;
use zz_protocol::canonical_key;

fn main() -> Result<(), Box<dyn Error>> {
    let mut values = Vec::new();
    for point in (0..=255).chain([
        0x1680, 0x2000, 0x2001, 0x2002, 0x2003, 0x2004, 0x2005, 0x2006, 0x2007, 0x2008, 0x2009,
        0x200a, 0x2028, 0x2029, 0x202f, 0x205f, 0x3000,
    ]) {
        let character = char::from_u32(point).expect("fixture point is a scalar");
        for raw in [
            format!("{character}C-a{character}"),
            format!("C-{character}"),
            format!("Ctrl-Alt-{character}"),
            character.to_string(),
        ] {
            values.push(json!({"value":canonical_key(&raw),"raw":raw}));
        }
    }
    for raw in [
        "Space",
        "none",
        "NONE",
        "nOnE",
        "Ctrl-C-Alt-M-a",
        "C- ",
        "Ctrl-",
        "M-C- ",
    ] {
        values.push(json!({"value":canonical_key(raw),"raw":raw}));
    }
    let path = std::env::args_os().nth(1).ok_or("expected output path")?;
    let mut output = BufWriter::new(File::create(path)?);
    serde_json::to_writer(&mut output, &values)?;
    output.write_all(b"\n")?;
    output.flush()?;
    Ok(())
}
