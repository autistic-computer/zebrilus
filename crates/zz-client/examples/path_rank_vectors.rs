use std::error::Error;

use neo_frizbee::{CaseMatching, Config, Matcher, SortStrategy};
use serde_json::{Value, json};
use zz_client::path_rank;

fn config(query: &str) -> Config {
    Config {
        max_typos: Some(u16::try_from(query.chars().count() / 4).unwrap_or(6).min(6)),
        casing: CaseMatching::Smart,
        sort: SortStrategy::Unsorted,
        ..Config::default()
    }
}

fn case(query: &str, labels: &[String], salt: usize, limit: usize) -> Value {
    let labels = labels.iter().map(String::as_str).collect::<Vec<_>>();
    let configuration = config(query);
    let mut matcher = Matcher::new(query, &configuration);
    let scores = matcher
        .match_list(&labels)
        .into_iter()
        .map(|matched| json!([matched.index, matched.score, matched.exact]))
        .collect::<Vec<_>>();
    let indices = matcher
        .match_list_indices(&labels)
        .into_iter()
        .map(|matched| json!([matched.index, matched.score, matched.exact, matched.indices]))
        .collect::<Vec<_>>();
    let changed = (0..labels.len())
        .map(|index| (index + salt).is_multiple_of(3))
        .collect::<Vec<_>>();
    let prior = labels
        .iter()
        .zip(&changed)
        .map(|(label, changed)| path_rank::path_prior(query, label, *changed))
        .collect::<Vec<_>>();
    let order = path_rank::rank(query, &labels, |index| prior[index], limit);
    let kept = order.iter().map(|index| labels[*index]).collect::<Vec<_>>();
    let hits = path_rank::highlights(query, &kept);
    json!({"query":query,"labels":labels,"changed":changed,"prior":prior,"depth":labels.iter().map(|label|path_rank::depth(label)).collect::<Vec<_>>(),"basename":labels.iter().map(|label|path_rank::in_basename(query,label)).collect::<Vec<_>>(),"scores":scores,"indices":indices,"limit":limit,"order":order,"hits":hits.iter().map(|ranges|ranges.iter().map(|range|[range.start,range.end]).collect::<Vec<_>>()).collect::<Vec<_>>()})
}

fn unicode_data() -> Vec<[u32; 4]> {
    (0..=0x0010_ffff)
        .filter_map(char::from_u32)
        .filter_map(|character| {
            let mut lower = character.to_lowercase();
            let first_lower = lower.next().unwrap_or(character);
            let flipped = if character.is_uppercase() {
                (lower.next().is_none() && first_lower.len_utf8() == character.len_utf8())
                    .then_some(first_lower)
            } else if character.is_lowercase() {
                let mut upper = character.to_uppercase();
                let first = upper.next().unwrap_or(character);
                (upper.next().is_none() && first.len_utf8() == character.len_utf8())
                    .then_some(first)
            } else {
                None
            }
            .unwrap_or(character);
            (character.is_uppercase() || flipped != character || first_lower != character)
                .then_some([
                    character as u32,
                    u32::from(character.is_uppercase()),
                    flipped as u32,
                    first_lower as u32,
                ])
        })
        .collect()
}

fn main() -> Result<(), Box<dyn Error>> {
    let labels = [
        "",
        "a",
        "foo",
        "x/foo/a",
        "x/a/foo",
        "src/main.rs",
        "src/m_a_i_n.rs",
        "fooBar",
        "foo_bar",
        "foo--bar",
        "/abc/",
        "a//b",
        "dir/wörld",
        "Ö/öö",
        "λ/κόσμος",
        "世界/文書",
        "e\u{301}",
        "é",
        "İ",
        "ß",
        "ſ",
        "src/README.md",
        "readme",
        "foo bar",
        "foo\tbar",
        "foo\0bar",
        "a-b-c",
        "𐐀/𐐨",
        "🦀/file.rs",
    ]
    .map(str::to_owned);
    let mut cases = Vec::new();
    for (index, query) in [
        "", "a", "f", "fo", "foo", "fob", "fb", "fbr", "main", "man", "mian", "mainr", "mair",
        "foobar", "foobxr", "foo b", "foo/", "a/foo", "README", "readme", "REA", "Rea", "ö", "Ö",
        "öö", "İ", "i", "ß", "s", "ſ", "κόσ", "世界", "文", "𐐨", "𐐀", "🦀", "e\u{301}", "é", "\0",
        "foo\0bar",
    ]
    .into_iter()
    .enumerate()
    {
        for limit in [0, 1, 3, 200] {
            cases.push(case(query, &labels, index, limit));
        }
    }
    let mut random = 0x7a90_13ce_u64;
    let alphabet = b"aabccFOO_Bar/.- 019";
    for index in 0..4096 {
        let mut words = Vec::new();
        for word in 0..5 {
            random = random
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let length =
                (random as usize % if word == 0 { 19 } else { 65 }) + usize::from(word == 0);
            let mut text = String::new();
            for _ in 0..length {
                random = random
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                text.push(alphabet[(random >> 32) as usize % alphabet.len()] as char);
            }
            words.push(text);
        }
        cases.push(case(&words[0], &words[1..], index, 200));
    }
    let alphabet = [
        "a", "ö", "é", "İ", "ı", "ß", "ſ", "Ω", "ω", "Д", "д", "世界", "🦀", "\u{301}", "\0", " ",
    ];
    for index in 0..1024 {
        let mut words = Vec::new();
        for word in 0..5 {
            random = random
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let length = random as usize % if word == 0 { 11 } else { 25 };
            let mut text = String::new();
            for _ in 0..length {
                random = random
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                text.push_str(alphabet[(random >> 32) as usize % alphabet.len()]);
            }
            words.push(text);
        }
        cases.push(case(&words[0], &words[1..], index, 200));
    }
    for length in [
        31_usize, 32, 33, 63, 64, 65, 127, 128, 129, 1023, 1024, 1025, 2048, 4096,
    ] {
        let long = vec![
            format!("f{}oo", "x".repeat(length.saturating_sub(3))),
            format!("Ö{}Ö", "x".repeat(length.saturating_sub(4))),
            format!("{}foo", "x".repeat(length.saturating_sub(3))),
            "a".repeat(length),
        ];
        for query in ["foo", "foox", "ö", "Ö", "aaaa"] {
            cases.push(case(query, &long, length, 200));
        }
    }
    for length in [19, 20, 21, 64, 100, 199, 200, 201, 600] {
        let duplicates = (0..length)
            .map(|index| ["foo/a", "foo/b", "foo/a", "foo/c"][index % 4].to_owned())
            .collect::<Vec<_>>();
        cases.push(case("foo", &duplicates, length, 200));
    }
    for query in [
        "a".repeat(1024),
        "a".repeat(1030),
        "a".repeat(4096),
        "é".repeat(512),
    ] {
        let extended = if query.len() < 4096 {
            format!("{query}x")
        } else {
            format!("{}b", "a".repeat(4095))
        };
        cases.push(case(
            &query,
            &[query.clone(), extended, "a".repeat(4096)],
            1,
            200,
        ));
    }
    let browse = labels
        .iter()
        .flat_map(|left| {
            labels.iter().flat_map(move |right| {
                [false, true].into_iter().flat_map(move |left_dir| {
                    [false, true].into_iter().map(move |right_dir| {
                        json!([
                            left,
                            left_dir,
                            right,
                            right_dir,
                            path_rank::browse_order((left, left_dir), (right, right_dir)) as i8
                        ])
                    })
                })
            })
        })
        .collect::<Vec<_>>();
    println!(
        "{}",
        serde_json::to_string(
            &json!({"cases":cases,"browse":browse,"unicode":unicode_data(),"scope":"actual public path_rank and pinned default typo matcher scores/indices"})
        )?
    );
    Ok(())
}
