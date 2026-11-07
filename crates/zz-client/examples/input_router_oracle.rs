use std::{
    error::Error,
    fs::File,
    io::{self, BufWriter, Write},
};

fn main() -> Result<(), Box<dyn Error>> {
    let vectors = zz_client::input_router_vectors();
    if let Some(path) = std::env::args_os().nth(1) {
        let mut output = BufWriter::new(File::create(path)?);
        serde_json::to_writer(&mut output, &vectors)?;
        output.write_all(b"\n")?;
        output.flush()?;
    } else {
        let mut output = io::stdout().lock();
        serde_json::to_writer(&mut output, &vectors)?;
        output.write_all(b"\n")?;
    }
    Ok(())
}
