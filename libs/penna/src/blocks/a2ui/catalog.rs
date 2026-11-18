use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use super::document::SpecError;
use crate::Tone;

pub const CATALOG_ID: &str = "penna/0";

pub const CATALOG: &str = include_str!("catalog.md");

pub const EXPRESS: &str = include_str!("express.md");

pub(super) const NAMES: &str = "Column, Row, Grid, Card, Text, Stat, Badge, Button, TextField, Select, Segmented, Slider, Progress, Table, Chart";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Param {
    Plain,
    Children,
    Child,
    Action,
    Rows,
    Columns,
}

pub(super) fn signature(component: &str) -> Option<&'static [(&'static str, Param)]> {
    use Param::{Action, Child, Children, Columns, Plain, Rows};
    Some(match component {
        "Column" | "Row" => &[("children", Children), ("justify", Plain), ("align", Plain)],
        "Grid" => &[("children", Children), ("columns", Plain)],
        "Card" => &[("child", Child)],
        "Text" => &[("text", Plain), ("variant", Plain), ("tone", Plain)],
        "Stat" => &[("label", Plain), ("value", Plain)],
        "Badge" => &[("label", Plain), ("tone", Plain)],
        "Button" => &[("label", Plain), ("action", Action), ("variant", Plain)],
        "TextField" => &[("value", Plain), ("label", Plain), ("placeholder", Plain)],
        "Select" => &[
            ("options", Plain),
            ("value", Plain),
            ("label", Plain),
            ("placeholder", Plain),
        ],
        "Segmented" => &[("options", Plain), ("value", Plain)],
        "Slider" => &[
            ("value", Plain),
            ("max", Plain),
            ("min", Plain),
            ("step", Plain),
            ("label", Plain),
        ],
        "Progress" => &[("value", Plain), ("label", Plain)],
        "Table" => &[("columns", Columns), ("rows", Rows)],
        "Chart" => &[
            ("rows", Rows),
            ("x", Rows),
            ("series", Columns),
            ("kind", Plain),
        ],
        _ => return None,
    })
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub(super) enum Dynamic<T> {
    Path {
        #[serde(rename = "@path")]
        path: String,
    },
    Literal(T),
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub(super) enum Scalar {
    Text(String),
    Number(f64),
    Flag(bool),
}

pub(super) type Words = Dynamic<Scalar>;

#[derive(Debug, Clone, PartialEq)]
pub struct Component {
    pub(super) id: String,
    pub(super) kind: Kind,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Kind {
    Column(Flex),
    Row(Flex),
    Grid(Grid),
    Card(Card),
    Text(Text),
    Stat(Stat),
    Badge(Badge),
    Button(Button),
    TextField(TextField),
    Select(Select),
    Segmented(Segmented),
    Slider(Slider),
    Progress(Progress),
    Table(Table),
    Chart(Chart),
}

impl Component {
    pub(super) fn from_value(value: Value) -> Result<Self, SpecError> {
        let Value::Object(mut fields) = value else {
            return Err(SpecError::new("a component must be an object"));
        };
        let Some(Value::String(id)) = fields.remove("id") else {
            return Err(SpecError::new("a component has no string `id`"));
        };
        let Some(Value::String(name)) = fields.remove("component") else {
            return Err(SpecError::new(format!(
                "component `{id}` has no `component` name"
            )));
        };
        fields.retain(|field, _| !field.starts_with('@') && field != "weight");
        let kind = match name.as_str() {
            "Column" => Kind::Column(props(&id, &name, fields)?),
            "Row" => Kind::Row(props(&id, &name, fields)?),
            "Grid" => Kind::Grid(props(&id, &name, fields)?),
            "Card" => Kind::Card(props(&id, &name, fields)?),
            "Text" => Kind::Text(props(&id, &name, fields)?),
            "Stat" => Kind::Stat(props(&id, &name, fields)?),
            "Badge" => Kind::Badge(props(&id, &name, fields)?),
            "Button" => Kind::Button(props(&id, &name, fields)?),
            "TextField" => Kind::TextField(props(&id, &name, fields)?),
            "Select" => Kind::Select(props(&id, &name, fields)?),
            "Segmented" => Kind::Segmented(props(&id, &name, fields)?),
            "Slider" => Kind::Slider(props(&id, &name, fields)?),
            "Progress" => Kind::Progress(props(&id, &name, fields)?),
            "Table" => Kind::Table(props(&id, &name, fields)?),
            "Chart" => Kind::Chart(props(&id, &name, fields)?),
            _ => {
                return Err(SpecError::new(format!(
                    "component `{id}` uses `{name}`, which is not in the catalog ({NAMES})"
                )));
            }
        };
        Ok(Self { id, kind })
    }

    pub(super) fn children(&self) -> Vec<&str> {
        match &self.kind {
            Kind::Column(flex) | Kind::Row(flex) => flex.children.ids(),
            Kind::Grid(grid) => grid.children.ids(),
            Kind::Card(card) => vec![card.child.as_str()],
            _ => Vec::new(),
        }
    }
}

fn props<T: DeserializeOwned>(
    id: &str,
    name: &str,
    fields: Map<String, Value>,
) -> Result<T, SpecError> {
    serde_json::from_value(Value::Object(fields))
        .map_err(|error| SpecError::new(format!("component `{id}` ({name}): {error}")))
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub(super) enum Children {
    Ids(Vec<String>),
    Template {
        #[serde(rename = "componentId")]
        component_id: String,
        path: String,
    },
}

impl Children {
    fn ids(&self) -> Vec<&str> {
        match self {
            Self::Ids(ids) => ids.iter().map(String::as_str).collect(),
            Self::Template { component_id, .. } => vec![component_id.as_str()],
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Justify {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Align {
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum ToneName {
    #[default]
    Neutral,
    Primary,
    Danger,
    Warning,
    Success,
    Accent,
}

impl From<ToneName> for Tone {
    fn from(name: ToneName) -> Self {
        match name {
            ToneName::Neutral => Self::Neutral,
            ToneName::Primary => Self::Primary,
            ToneName::Danger => Self::Danger,
            ToneName::Warning => Self::Warning,
            ToneName::Success => Self::Success,
            ToneName::Accent => Self::Accent,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Flex {
    pub(super) children: Children,
    #[serde(default)]
    pub(super) justify: Justify,
    #[serde(default)]
    pub(super) align: Option<Align>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Grid {
    pub(super) children: Children,
    #[serde(default = "three")]
    pub(super) columns: u16,
}

const fn three() -> u16 {
    3
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Card {
    pub(super) child: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum TextVariant {
    #[default]
    Body,
    Caption,
    Heading,
    Title,
    Mono,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Text {
    pub(super) text: Words,
    #[serde(default)]
    pub(super) variant: TextVariant,
    #[serde(default)]
    pub(super) tone: Option<ToneName>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Stat {
    pub(super) label: Words,
    pub(super) value: Words,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Badge {
    pub(super) label: Words,
    #[serde(default)]
    pub(super) tone: ToneName,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum ButtonVariant {
    #[default]
    Default,
    Primary,
    #[serde(alias = "borderless")]
    Ghost,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Button {
    pub(super) label: Words,
    #[serde(default)]
    pub(super) variant: ButtonVariant,
    pub(super) action: ActionSpec,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ActionSpec {
    pub(super) event: EventSpec,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct EventSpec {
    pub(super) name: String,
    #[serde(default)]
    pub(super) context: Map<String, Value>,
    #[serde(default)]
    pub(super) order: Vec<String>,
    #[serde(default)]
    pub(super) user_message: Option<Words>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TextField {
    #[serde(default)]
    pub(super) label: Option<Words>,
    pub(super) value: Words,
    #[serde(default)]
    pub(super) placeholder: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub(super) enum Choice {
    Plain(String),
    Labeled { label: String, value: String },
}

impl Choice {
    pub(super) fn label(&self) -> &str {
        match self {
            Self::Plain(value) => value,
            Self::Labeled { label, .. } => label,
        }
    }

    pub(super) fn key(&self) -> &String {
        match self {
            Self::Plain(value) | Self::Labeled { value, .. } => value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Select {
    #[serde(default)]
    pub(super) label: Option<Words>,
    pub(super) options: Vec<Choice>,
    pub(super) value: Words,
    #[serde(default)]
    pub(super) placeholder: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Segmented {
    pub(super) options: Vec<Choice>,
    pub(super) value: Words,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Slider {
    #[serde(default)]
    pub(super) label: Option<Words>,
    pub(super) value: Dynamic<f64>,
    #[serde(default)]
    pub(super) min: f64,
    pub(super) max: f64,
    #[serde(default)]
    pub(super) step: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Progress {
    pub(super) value: Dynamic<f64>,
    #[serde(default)]
    pub(super) label: Option<Words>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Table {
    pub(super) columns: Vec<TableColumn>,
    pub(super) rows: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TableColumn {
    pub(super) title: String,
    pub(super) path: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum ChartKind {
    #[default]
    Bar,
    Line,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Chart {
    pub(super) rows: String,
    pub(super) x: String,
    pub(super) series: Vec<ChartSeries>,
    #[serde(default)]
    pub(super) kind: ChartKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub(super) enum ChartSeries {
    Plain(String),
    Bound {
        #[serde(rename = "@path")]
        path: String,
    },
    Titled {
        title: String,
        path: String,
    },
}

impl ChartSeries {
    pub(super) fn title(&self) -> &str {
        match self {
            Self::Plain(path) | Self::Bound { path } => path,
            Self::Titled { title, .. } => title,
        }
    }

    pub(super) fn path(&self) -> &str {
        match self {
            Self::Plain(path) | Self::Bound { path } | Self::Titled { path, .. } => path,
        }
    }
}
