use std::collections::HashMap;
use std::fmt;

use serde_json::{Map, Value};

use super::catalog::{Component, Dynamic, Kind, Scalar};
use crate::components::chart::MAX_SERIES;

const ROOT: &str = "root";
const BARE_SURFACE: &str = "main";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecError(String);

impl SpecError {
    pub(super) fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for SpecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for SpecError {}

#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    CreateSurface {
        surface_id: String,
        components: Vec<Component>,
        data_model: Option<Value>,
    },
    UpdateComponents {
        surface_id: String,
        components: Vec<Component>,
    },
    UpdateDataModel {
        surface_id: String,
        path: String,
        value: Value,
    },
    DeleteSurface {
        surface_id: String,
    },
}

impl Message {
    pub fn surface_id(&self) -> &str {
        match self {
            Self::CreateSurface { surface_id, .. }
            | Self::UpdateComponents { surface_id, .. }
            | Self::UpdateDataModel { surface_id, .. }
            | Self::DeleteSurface { surface_id } => surface_id,
        }
    }

    fn from_value(value: Value) -> Result<Self, SpecError> {
        let Value::Object(mut fields) = value else {
            return Err(SpecError::new("a message must be an object"));
        };
        fields.remove("version");
        if fields.contains_key("components") {
            fields
                .entry("surfaceId")
                .or_insert_with(|| Value::String(BARE_SURFACE.to_owned()));
            return Self::create(fields);
        }
        let Some((name, body)) = fields.into_iter().next() else {
            return Err(SpecError::new("a message has no content"));
        };
        let Value::Object(mut body) = body else {
            return Err(SpecError::new(format!("`{name}` must hold an object")));
        };
        match name.as_str() {
            "createSurface" => Self::create(body),
            "updateComponents" => Ok(Self::UpdateComponents {
                surface_id: surface_id(&mut body, &name)?,
                components: components(&mut body)?,
            }),
            "updateDataModel" => Ok(Self::UpdateDataModel {
                surface_id: surface_id(&mut body, &name)?,
                path: match body.remove("path") {
                    Some(Value::String(path)) => path,
                    _ => String::new(),
                },
                value: body.remove("value").unwrap_or(Value::Null),
            }),
            "deleteSurface" => Ok(Self::DeleteSurface {
                surface_id: surface_id(&mut body, &name)?,
            }),
            _ => Err(SpecError::new(format!(
                "unknown message `{name}` (createSurface, updateComponents, updateDataModel, deleteSurface)"
            ))),
        }
    }

    fn create(mut body: Map<String, Value>) -> Result<Self, SpecError> {
        Ok(Self::CreateSurface {
            surface_id: surface_id(&mut body, "createSurface")?,
            components: components(&mut body)?,
            data_model: body.remove("dataModel"),
        })
    }
}

fn surface_id(body: &mut Map<String, Value>, message: &str) -> Result<String, SpecError> {
    match body.remove("surfaceId") {
        Some(Value::String(id)) => Ok(id),
        _ => Err(SpecError::new(format!(
            "`{message}` has no string `surfaceId`"
        ))),
    }
}

fn components(body: &mut Map<String, Value>) -> Result<Vec<Component>, SpecError> {
    match body.remove("components") {
        None => Ok(Vec::new()),
        Some(Value::Array(items)) => items.into_iter().map(Component::from_value).collect(),
        Some(_) => Err(SpecError::new("`components` must be a list")),
    }
}

pub fn parse_messages(text: &str) -> Result<Vec<Message>, SpecError> {
    let mut messages = Vec::new();
    for value in serde_json::Deserializer::from_str(text).into_iter::<Value>() {
        match value.map_err(|error| SpecError::new(format!("not JSON: {error}")))? {
            Value::Array(items) => {
                for item in items {
                    messages.push(Message::from_value(item)?);
                }
            }
            other => messages.push(Message::from_value(other)?),
        }
    }
    if messages.is_empty() {
        return Err(SpecError::new("no messages"));
    }
    Ok(messages)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    components: HashMap<String, Component>,
    model: Value,
}

impl Document {
    pub fn apply(&mut self, message: Message) -> Vec<String> {
        match message {
            Message::CreateSurface {
                components,
                data_model,
                ..
            } => {
                let mut changed: Vec<String> = self.components.drain().map(|(id, _)| id).collect();
                self.model = data_model.unwrap_or(Value::Null);
                changed.extend(self.insert(components));
                changed
            }
            Message::UpdateComponents { components, .. } => self.insert(components),
            Message::UpdateDataModel { path, value, .. } => {
                self.set(&absolute("", &path), value);
                Vec::new()
            }
            Message::DeleteSurface { .. } => {
                self.model = Value::Null;
                self.components.drain().map(|(id, _)| id).collect()
            }
        }
    }

    fn insert(&mut self, components: Vec<Component>) -> Vec<String> {
        components
            .into_iter()
            .map(|component| {
                let id = component.id.clone();
                self.components.insert(id.clone(), component);
                id
            })
            .collect()
    }

    pub fn model(&self) -> &Value {
        &self.model
    }

    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if !self.components.contains_key(ROOT) {
            problems.push(format!("no component has the id `{ROOT}`"));
        }
        let mut ids: Vec<&String> = self.components.keys().collect();
        ids.sort();
        for id in ids {
            for child in self.components[id].children() {
                if !self.components.contains_key(child) {
                    problems.push(format!("`{id}` refers to `{child}`, which does not exist"));
                }
            }
            if let Kind::Chart(chart) = &self.components[id].kind
                && !(1..=MAX_SERIES).contains(&chart.series.len())
            {
                problems.push(format!(
                    "chart `{id}` has {} series, a chart takes 1 to {MAX_SERIES}",
                    chart.series.len()
                ));
            }
        }
        let mut trail = Vec::new();
        if let Some(repeated) = self.cycle(ROOT, &mut trail) {
            problems.push(format!("`{repeated}` contains itself"));
        }
        problems
    }

    fn cycle<'doc>(&'doc self, id: &'doc str, trail: &mut Vec<&'doc str>) -> Option<&'doc str> {
        if trail.contains(&id) {
            return Some(id);
        }
        let component = self.components.get(id)?;
        trail.push(id);
        let found = component
            .children()
            .into_iter()
            .find_map(|child| self.cycle(child, trail));
        trail.pop();
        found
    }

    pub fn buttons(&self) -> Vec<(String, String)> {
        let mut ids: Vec<&String> = self.components.keys().collect();
        ids.sort();
        ids.into_iter()
            .filter_map(|id| match &self.components[id].kind {
                Kind::Button(button) => {
                    let label = match &button.label {
                        Dynamic::Literal(Scalar::Text(text)) => text.clone(),
                        Dynamic::Literal(_) => String::new(),
                        Dynamic::Path { path } => self
                            .value(&absolute("", path))
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                    };
                    Some((button.action.event.name.clone(), label))
                }
                _ => None,
            })
            .collect()
    }

    pub(super) fn root(&self) -> &'static str {
        ROOT
    }

    pub(super) fn component(&self, id: &str) -> Option<&Component> {
        self.components.get(id)
    }

    pub(super) fn value(&self, pointer: &str) -> Option<&Value> {
        self.model.pointer(pointer)
    }

    pub(super) fn set(&mut self, pointer: &str, value: Value) -> bool {
        if self.value(pointer) == Some(&value) {
            return false;
        }
        let mut tokens: Vec<String> = pointer
            .split('/')
            .skip(1)
            .map(|token| token.replace("~1", "/").replace("~0", "~"))
            .collect();
        let Some(last) = tokens.pop() else {
            self.model = value;
            return true;
        };
        let mut current = &mut self.model;
        for token in tokens {
            current = match current {
                Value::Array(items) => {
                    let Some(item) = token
                        .parse::<usize>()
                        .ok()
                        .and_then(|index| items.get_mut(index))
                    else {
                        return false;
                    };
                    item
                }
                other => object(other)
                    .entry(token)
                    .or_insert_with(|| Value::Object(Map::new())),
            };
        }
        match current {
            Value::Array(items) => match last.parse::<usize>() {
                Ok(index) if index < items.len() => items[index] = value,
                Ok(index) if index == items.len() => items.push(value),
                _ => return false,
            },
            other => {
                let fields = object(other);
                if value.is_null() {
                    fields.remove(&last);
                } else {
                    fields.insert(last, value);
                }
            }
        }
        true
    }
}

fn object(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    match value {
        Value::Object(fields) => fields,
        _ => unreachable!("the value was just made an object"),
    }
}

pub(super) fn absolute(base: &str, path: &str) -> String {
    match path {
        "" => base.to_owned(),
        "/" => String::new(),
        _ if path.starts_with('/') => path.to_owned(),
        _ => format!("{base}/{path}"),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_bare_block_becomes_one_surface_and_writes_land_in_its_model() {
        let messages = parse_messages(
            r#"{"components":[
                {"id":"root","component":"Column","children":["name","go"]},
                {"id":"name","component":"TextField","label":"Name","value":{"@path":"/form/name"}},
                {"id":"go","component":"Button","label":"Go","action":{"event":{"name":"go"}}}
            ],"dataModel":{"form":{"name":"zz"},"rows":[{"n":1}]}}"#,
        )
        .expect("the block parses");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].surface_id(), "main");

        let mut document = Document::default();
        for message in messages {
            document.apply(message);
        }
        assert_eq!(document.problems(), Vec::<String>::new());
        assert!(document.set("/form/name", json!("penna")));
        assert!(!document.set("/form/name", json!("penna")));
        assert!(document.set("/rows/1", json!({"n": 2})));
        assert!(document.set("/new/deep/key", json!(3)));
        assert_eq!(
            document.model(),
            &json!({
                "form": {"name": "penna"},
                "rows": [{"n": 1}, {"n": 2}],
                "new": {"deep": {"key": 3}}
            })
        );
        assert_eq!(absolute("/rows/1", "n"), "/rows/1/n");
        assert_eq!(absolute("/rows/1", "/form/name"), "/form/name");
    }

    #[test]
    fn a_bad_spec_says_which_component_and_why() {
        let unknown_field =
            parse_messages(r#"{"components":[{"id":"root","component":"Text","txt":"hi"}]}"#)
                .expect_err("`txt` is not a Text field");
        assert_eq!(
            unknown_field.to_string(),
            "component `root` (Text): unknown field `txt`, expected one of `text`, `variant`, `tone`"
        );

        let unknown_component =
            parse_messages(r#"{"components":[{"id":"root","component":"Carousel"}]}"#)
                .expect_err("Carousel is not in the catalog");
        assert_eq!(
            unknown_component.to_string(),
            "component `root` uses `Carousel`, which is not in the catalog (Column, Row, Grid, Card, Text, Stat, Badge, Button, TextField, Select, Segmented, Slider, Progress, Table, Chart)"
        );

        let mut document = Document::default();
        for message in parse_messages(
            r#"{"components":[
                {"id":"top","component":"Column","children":["card","ghost"]},
                {"id":"card","component":"Card","child":"top"}
            ]}"#,
        )
        .expect("the block parses")
        {
            document.apply(message);
        }
        assert_eq!(
            document.problems(),
            vec![
                "no component has the id `root`".to_owned(),
                "`top` refers to `ghost`, which does not exist".to_owned(),
            ]
        );
    }
}
