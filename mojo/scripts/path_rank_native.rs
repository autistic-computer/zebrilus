use std::{
    error::Error,
    fs::File,
    io::{BufReader, BufWriter, Read, Write},
};

fn uint(input: &mut impl Read) -> Result<u32, Box<dyn Error>> {
    let mut bytes = [0; 4];
    input.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn text(input: &mut impl Read, maximum: u32) -> Result<String, Box<dyn Error>> {
    let size = uint(input)?;
    if size > maximum {
        return Err("native path reference text exceeds its bound".into());
    }
    let mut bytes = vec![0; size as usize];
    input.read_exact(&mut bytes)?;
    Ok(String::from_utf8(bytes)?)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 4 {
        return Err("usage: path-native-reference INPUT OUTPUT WIDTH".into());
    }
    let width: u32 = args[3].parse()?;
    let mut input = BufReader::new(File::open(&args[1])?);
    let mut output = BufWriter::new(File::create(&args[2])?);
    let count = uint(&mut input)?;
    if count > 100_000 {
        return Err("too many native path reference pairs".into());
    }
    output.write_all(&count.to_le_bytes())?;
    output.write_all(&neo_frizbee::zz_path_vector_bytes().to_le_bytes())?;
    for _ in 0..count {
        let query = text(&mut input, 65536)?;
        let label = text(&mut input, 4096)?;
        let (accepted, start, end, score, exact, indices) =
            neo_frizbee::zz_path_debug_at(width, &query, &label);
        output.write_all(&[u8::from(accepted)])?;
        output.write_all(&u32::try_from(start)?.to_le_bytes())?;
        output.write_all(&u32::try_from(end)?.to_le_bytes())?;
        output.write_all(&score.to_le_bytes())?;
        output.write_all(&[u8::from(exact)])?;
        output.write_all(&u32::try_from(indices.len())?.to_le_bytes())?;
        for index in indices {
            output.write_all(&index.to_le_bytes())?;
        }
    }
    output.flush()?;
    Ok(())
}
