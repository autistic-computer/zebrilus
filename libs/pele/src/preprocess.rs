use crate::PeleError;
use serde_json::{Map, Value};

pub(crate) struct Prepared {
    pub text: String,
    pub title: Option<String>,
    pub config: Value,
    pub source_lines: Vec<usize>,
    pub source_positions: Vec<Vec<(usize, usize)>>,
}

pub(crate) fn merge_config(target: &mut Value, source: &Value) {
    let Value::Object(source) = source else {
        return;
    };
    if !target.is_object() {
        *target = Value::Object(Map::new());
    }
    let target = target.as_object_mut().expect("object config");
    for (key, value) in source {
        if matches!(key.as_str(), "__proto__" | "constructor" | "prototype") {
            continue;
        }
        if value.is_object() {
            merge_config(target.entry(key.clone()).or_insert(Value::Null), value);
        } else {
            target.insert(key.clone(), value.clone());
        }
    }
}

fn yaml_string(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), String::from)
}

fn frontmatter(source: &str) -> Option<(String, usize, usize)> {
    let first_end = source.find('\n')?;
    let first = &source[..first_end];
    let trimmed = first.trim_start_matches(|c: char| c.is_whitespace() && c != '\n');
    if trimmed.trim_end() != "---" {
        return None;
    }
    let indent = &first[..first.len() - trimmed.len()];
    let fence = format!("{indent}---");
    let mut offset = first_end + 1;
    for (index, part) in source[offset..].split_inclusive('\n').enumerate() {
        let line = part.trim_end_matches('\n');
        if line.trim_end() == fence && part.ends_with('\n') {
            let body = source[first_end + 1..offset]
                .split('\n')
                .map(|line| line.strip_prefix(indent).unwrap_or(line))
                .collect::<Vec<_>>()
                .join("\n");
            return Some((body, offset + part.len(), index + 2));
        }
        offset += part.len();
    }
    None
}

pub(crate) fn preprocess(source: &str) -> Result<Prepared, PeleError> {
    let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
    let mut remaining = normalized.as_str();
    let mut line_offset = 0;
    let mut config = Value::Object(Map::new());
    let mut title = None;
    if let Some((yaml, consumed, lines)) = frontmatter(&normalized) {
        let parsed: Value = serde_yaml::from_str(&yaml).map_err(|error| {
            let position = error.location();
            PeleError::syntax(
                format!("Front matter is not valid YAML. {error}"),
                position.as_ref().map_or(0, |p| p.line() + 1),
                position.as_ref().map_or(0, |p| p.column()),
            )
        })?;
        if let Value::Object(parsed) = parsed {
            title = parsed
                .get("title")
                .filter(|v| !v.is_null() && **v != false && **v != 0 && **v != "")
                .map(yaml_string);
            if let Some(extra) = parsed.get("config") {
                merge_config(&mut config, extra);
            }
            if let Some(mode) = parsed.get("displayMode").filter(|v| !v.is_null()) {
                if !config["gantt"].is_object() {
                    config["gantt"] = Value::Object(Map::new());
                }
                config["gantt"]["displayMode"] = yaml_string(mode).into();
            }
        }
        remaining = &normalized[consumed..];
        line_offset = lines;
    }
    let mut chars: Vec<(char, usize, usize)> = Vec::with_capacity(remaining.len());
    let mut original_line = line_offset + 1;
    let mut original_column = 1;
    let mut cursor = 0;
    while cursor < remaining.len() {
        if remaining[cursor..].starts_with("%%{") {
            let directive_start = cursor + 3;
            if let Some(end) = remaining[directive_start..].find("}%%") {
                let end = directive_start + end;
                let body = remaining[directive_start..end].trim();
                let (kind, arguments) = body.split_once(':').unwrap_or((body, ""));
                if kind.trim() == "wrap" {
                    config["wrap"] = true.into();
                } else if matches!(kind.trim(), "init" | "initialize") {
                    let arguments = arguments.replace('\'', "\"");
                    if let Ok(value) = serde_json::from_str::<Value>(arguments.trim()) {
                        merge_config(&mut config, &value);
                    }
                }
                for ch in remaining[cursor..end + 3].chars() {
                    if ch == '\n' {
                        original_line += 1;
                        original_column = 1;
                    } else {
                        original_column += ch.len_utf16();
                    }
                }
                cursor = end + 3;
                continue;
            }
            return Err(PeleError::syntax(
                "Unterminated Mermaid directive.",
                original_line,
                1,
            ));
        }
        let ch = remaining[cursor..]
            .chars()
            .next()
            .expect("character boundary");
        chars.push((ch, original_line, original_column));
        if ch == '\n' {
            original_line += 1;
            original_column = 1;
        } else {
            original_column += ch.len_utf16();
        }
        cursor += ch.len_utf8();
    }
    let mut kept = Vec::with_capacity(chars.len());
    let mut start = 0;
    while start < chars.len() {
        let end = chars[start..]
            .iter()
            .position(|(ch, _, _)| *ch == '\n')
            .map_or(chars.len(), |p| start + p + 1);
        let line: String = chars[start..end].iter().map(|(ch, _, _)| ch).collect();
        if !line.trim_start().starts_with("%%") {
            kept.extend_from_slice(&chars[start..end]);
        }
        start = end;
    }
    let first = kept
        .iter()
        .position(|(ch, _, _)| !ch.is_whitespace() && *ch != '\u{feff}')
        .unwrap_or(kept.len());
    let kept = &kept[first..];
    let mut text = String::new();
    let mut source_lines = Vec::new();
    let mut source_positions: Vec<Vec<(usize, usize)>> = Vec::new();
    let mut start_of_line = true;
    for (ch, line, column) in kept {
        if start_of_line {
            source_lines.push(*line);
            source_positions.push(Vec::new());
        }
        for unit in 0..ch.len_utf16() {
            source_positions
                .last_mut()
                .expect("source line")
                .push((*line, column + unit));
        }
        text.push(*ch);
        start_of_line = *ch == '\n';
    }
    if start_of_line {
        source_positions.push(Vec::new());
    }
    source_positions
        .last_mut()
        .expect("source EOF line")
        .push((original_line, original_column));
    Ok(Prepared {
        text,
        title,
        config,
        source_lines,
        source_positions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn frontmatter_and_directives_merge_nested_config_and_map_lines() {
        let source = "---\r\ntitle: Example\r\nconfig:\r\n  pie:\r\n    donutHole: 0.4\r\n---\r\n%% a comment\r\n%%{init: {'pie': {'textPosition': 0.7}}}%%\r\npie\r\n  \"A\" : 3\r\n";
        let prepared = preprocess(source).unwrap();
        assert_eq!(prepared.title.as_deref(), Some("Example"));
        assert_eq!(
            prepared.config,
            json!({"pie":{"donutHole":0.4,"textPosition":0.7}})
        );
        assert_eq!(prepared.text, "pie\n  \"A\" : 3\n");
        assert_eq!(prepared.source_lines, vec![9, 10]);
    }

    #[test]
    fn config_drops_unsafe_keys_recursively() {
        let mut target = json!({"pie":{"width":400}});
        merge_config(
            &mut target,
            &json!({"__proto__":{"polluted":true},"pie":{"constructor":4,"height":300}}),
        );
        assert_eq!(target, json!({"pie":{"width":400,"height":300}}));
    }

    #[test]
    fn invalid_yaml_reports_original_line() {
        let error = preprocess("---\nconfig: [\n---\npie\n\"A\": 1")
            .err()
            .unwrap();
        assert_eq!(error.code, "syntax");
        assert!(error.line >= 2);
    }

    #[test]
    fn commented_directive_does_not_delete_following_diagram() {
        let prepared = preprocess("%%{wrap}%%\n%% ignored\nflowchart LR\nA --> B").unwrap();
        assert_eq!(prepared.config["wrap"], true);
        assert!(prepared.text.starts_with("flowchart"));
        assert_eq!(prepared.source_lines, vec![3, 4]);
    }
}
