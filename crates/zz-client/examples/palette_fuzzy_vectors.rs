use std::{
    error::Error,
    io::{self, Write},
};

use serde_json::{Value, json};
use zz_client::fuzzy::fuzzy_match;

fn case(cases: &mut Vec<Value>, query: &str, label: &str) {
    cases.push(json!({
        "query": query,
        "label": label,
        "expected": fuzzy_match(query, label).map(|(score, hits)| json!({
            "score": score,
            "hits": hits.into_iter().map(|range| [range.start, range.end]).collect::<Vec<_>>()
        }))
    }));
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut lower = Vec::new();
    let mut whitespace = Vec::new();
    let mut cases = Vec::new();
    for scalar in 0..=0x10_FFFF {
        let Some(character) = char::from_u32(scalar) else {
            continue;
        };
        let folded: Vec<_> = character.to_lowercase().map(u32::from).collect();
        if folded != [scalar] {
            lower.push(json!([scalar, folded]));
            let original = character.to_string();
            let label = format!("pre/{character}/post");
            case(&mut cases, &original, &original);
            case(&mut cases, &original, &label);
            let query: String = character.to_lowercase().collect();
            case(&mut cases, &query, &label);
        }
        if character.is_whitespace() {
            whitespace.push(scalar);
            let query = format!("{character}a{character}b{character}");
            case(&mut cases, &query, "--A--B--");
            case(&mut cases, &query, "BA");
        }
    }
    let labels = [
        "",
        "a",
        "A",
        "ab",
        "BA",
        "A B C",
        "abcabc",
        "foo/bar.rs",
        "select-pane",
        "split-window",
        "東京terminal",
        "世界🌍",
        "İstanbul",
        "i\u{307}",
        "Σίσυφος",
        "Straße",
        "STRASSE",
        "\u{212A}elvin",
        "KELVIN",
        "café",
        "cafe\u{301}",
        "a\0b",
        "a\u{200b}b",
        "\u{a0}a\u{202f}b",
        "𐐀𐐨𐐁𐐩",
        "ქართული",
        "ᲥᲐᲠᲗᲣᲚᲘ",
    ];
    let queries = [
        "",
        " ",
        "a",
        "A",
        "ab",
        "ba",
        "abc",
        "ac",
        "fp",
        "sp",
        "sw",
        "tw",
        "東京",
        "世🌍",
        "İ",
        "i",
        "i\u{307}",
        "σ",
        "ς",
        "ss",
        "ß",
        "k",
        "\u{212A}",
        "é",
        "e\u{301}",
        "a\0b",
        "a\u{200b}b",
        "a\u{85}b",
        "𐐀𐐁",
        "ქართული",
    ];
    for label in labels {
        for query in queries {
            case(&mut cases, query, label);
        }
    }
    let mut seed = 0xA87C_395D_u64;
    let alphabet = ['a', 'b', 'c', 'A', 'B', 'C', ' ', '/', 'é', 'Σ', 'İ', '🌍'];
    for _ in 0..4096 {
        let mut label = String::new();
        let mut query = String::new();
        for index in 0..24 {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let character = alphabet[((seed >> 32) as usize) % alphabet.len()];
            label.push(character);
            if index % 5 == 0 {
                query.push(character);
            }
        }
        case(&mut cases, &query, &label);
    }
    let output = json!({"lowercase": lower, "whitespace": whitespace, "cases": cases});
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    serde_json::to_writer(&mut stdout, &output)?;
    stdout.write_all(b"\n")?;
    Ok(())
}
