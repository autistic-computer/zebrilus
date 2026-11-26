use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientMessageKind {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatusPosition {
    Top,
    #[default]
    Bottom,
}

impl StatusPosition {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusLine {
    #[serde(deserialize_with = "deserialize_status_text")]
    pub left: String,
    #[serde(deserialize_with = "deserialize_status_text")]
    pub right: String,
    #[serde(deserialize_with = "deserialize_status_text")]
    pub title: String,
    #[serde(deserialize_with = "deserialize_status_text")]
    pub base_style: String,
    #[serde(deserialize_with = "deserialize_status_rows")]
    pub rows: Vec<String>,
    pub position: StatusPosition,
    pub message_line: u8,
    pub customized: bool,
    pub theme: crate::ThemeColours,
    #[serde(deserialize_with = "deserialize_status_text")]
    pub message_style: String,
    #[serde(deserialize_with = "deserialize_status_text")]
    pub message_command_style: String,
    #[serde(deserialize_with = "deserialize_mode_presentations")]
    pub modes: Vec<ModePresentation>,
    #[serde(deserialize_with = "deserialize_pane_border_presentations")]
    pub pane_borders: Vec<PaneBorderPresentation>,
}

pub const MAX_MODE_PRESENTATIONS: usize = 2;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModePresentation {
    pub pane: PaneId,
    pub view: bool,
    #[serde(deserialize_with = "deserialize_status_text")]
    pub position: String,
    #[serde(deserialize_with = "deserialize_status_text")]
    pub position_style: String,
    #[serde(deserialize_with = "deserialize_status_text")]
    pub selection_style: String,
    pub vi_keys: bool,
    #[serde(deserialize_with = "deserialize_status_text")]
    pub match_style: String,
    #[serde(deserialize_with = "deserialize_status_text")]
    pub current_match_style: String,
    pub line_numbers: u8,
    #[serde(deserialize_with = "deserialize_status_text")]
    pub line_number_style: String,
    #[serde(deserialize_with = "deserialize_status_text")]
    pub current_line_number_style: String,
    #[serde(deserialize_with = "deserialize_status_text")]
    pub current_line_style: String,
}

fn deserialize_mode_presentations<'de, D>(
    deserializer: D,
) -> Result<Vec<ModePresentation>, D::Error>
where
    D: Deserializer<'de>,
{
    let modes = Vec::<ModePresentation>::deserialize(deserializer)?;
    if modes.len() > MAX_MODE_PRESENTATIONS {
        return Err(serde::de::Error::invalid_length(
            modes.len(),
            &"at most two mode presentations",
        ));
    }
    Ok(modes)
}

pub const MAX_PANE_BORDER_PRESENTATIONS: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneBorderPresentation {
    pub pane: PaneId,
    #[serde(deserialize_with = "deserialize_status_text")]
    pub style: String,
    #[serde(default, deserialize_with = "deserialize_status_text")]
    pub window_style: String,
    #[serde(default, deserialize_with = "deserialize_status_text")]
    pub window_active_style: String,
}

fn deserialize_pane_border_presentations<'de, D>(
    deserializer: D,
) -> Result<Vec<PaneBorderPresentation>, D::Error>
where
    D: Deserializer<'de>,
{
    let borders = Vec::<PaneBorderPresentation>::deserialize(deserializer)?;
    if borders.len() > MAX_PANE_BORDER_PRESENTATIONS {
        return Err(serde::de::Error::invalid_length(
            borders.len(),
            &"at most 256 pane border presentations",
        ));
    }
    Ok(borders)
}

fn style_or_empty_parses(value: &str) -> bool {
    value.is_empty() || crate::parse_style(value).is_some()
}

impl StatusLine {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.left.len() > MAX_STATUS_TEXT_BYTES
            || self.right.len() > MAX_STATUS_TEXT_BYTES
            || self.title.len() > MAX_STATUS_TEXT_BYTES
            || self.base_style.len() > MAX_STATUS_TEXT_BYTES
        {
            return Err("status text exceeds the wire byte limit");
        }
        if self.rows.len() > MAX_STATUS_ROWS {
            return Err("status rows exceed the wire row limit");
        }
        if self
            .rows
            .iter()
            .any(|row| row.len() > MAX_STATUS_TEXT_BYTES)
        {
            return Err("status row exceeds the wire byte limit");
        }
        if crate::parse_style(&self.base_style).is_none() {
            return Err("status base style does not parse as a style");
        }
        if self.rows.is_empty() {
            if self.message_line != 0 {
                return Err("status message line names a row while no rows are published");
            }
        } else if usize::from(self.message_line) >= self.rows.len() {
            return Err("status message line names a row outside the published rows");
        }
        if self.theme.is_circular() {
            return Err("status theme slot carries a theme colour");
        }
        if self.message_style.len() > MAX_STATUS_TEXT_BYTES
            || self.message_command_style.len() > MAX_STATUS_TEXT_BYTES
        {
            return Err("status message style exceeds the wire byte limit");
        }
        if !style_or_empty_parses(&self.message_style)
            || !style_or_empty_parses(&self.message_command_style)
        {
            return Err("status message style does not parse as a style");
        }
        if self.modes.len() > MAX_MODE_PRESENTATIONS {
            return Err("status mode presentations exceed the wire limit");
        }
        for mode in &self.modes {
            if mode.position.len() > MAX_STATUS_TEXT_BYTES
                || mode.position_style.len() > MAX_STATUS_TEXT_BYTES
                || mode.selection_style.len() > MAX_STATUS_TEXT_BYTES
                || mode.match_style.len() > MAX_STATUS_TEXT_BYTES
                || mode.current_match_style.len() > MAX_STATUS_TEXT_BYTES
            {
                return Err("status mode presentation exceeds the wire byte limit");
            }
            if !style_or_empty_parses(&mode.position_style)
                || !style_or_empty_parses(&mode.selection_style)
                || !style_or_empty_parses(&mode.match_style)
                || !style_or_empty_parses(&mode.current_match_style)
            {
                return Err("status mode style does not parse as a style");
            }
        }
        if self.pane_borders.len() > MAX_PANE_BORDER_PRESENTATIONS {
            return Err("status pane border presentations exceed the wire limit");
        }
        for border in &self.pane_borders {
            let styles = [
                &border.style,
                &border.window_style,
                &border.window_active_style,
            ];
            if styles
                .iter()
                .any(|style| style.len() > MAX_STATUS_TEXT_BYTES)
            {
                return Err("status pane border style exceeds the wire byte limit");
            }
            if !styles.iter().all(|style| style_or_empty_parses(style)) {
                return Err("status pane border style does not parse as a style");
            }
        }
        Ok(())
    }
}

fn deserialize_status_text<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_STATUS_TEXT_BYTES)
}

struct BoundedStatusRow(String);

impl<'de> Deserialize<'de> for BoundedStatusRow {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StatusRowVisitor;

        impl Visitor<'_> for StatusRowVisitor {
            type Value = BoundedStatusRow;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(
                    formatter,
                    "a status row no longer than {MAX_STATUS_TEXT_BYTES} bytes"
                )
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if value.len() > MAX_STATUS_TEXT_BYTES {
                    return Err(E::invalid_length(value.len(), &self));
                }
                Ok(BoundedStatusRow(value.to_owned()))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if value.len() > MAX_STATUS_TEXT_BYTES {
                    return Err(E::invalid_length(value.len(), &self));
                }
                Ok(BoundedStatusRow(value))
            }
        }

        deserializer.deserialize_str(StatusRowVisitor)
    }
}

fn deserialize_status_rows<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct StatusRowsVisitor;

    impl<'de> Visitor<'de> for StatusRowsVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "at most {MAX_STATUS_ROWS} bounded status rows")
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let hint = sequence.size_hint();
            if hint.is_some_and(|length| length > MAX_STATUS_ROWS) {
                return Err(A::Error::invalid_length(
                    hint.unwrap_or(MAX_STATUS_ROWS.saturating_add(1)),
                    &self,
                ));
            }
            let mut rows = Vec::with_capacity(hint.unwrap_or(0).min(MAX_STATUS_ROWS));
            while rows.len() < MAX_STATUS_ROWS {
                let Some(row) = sequence.next_element::<BoundedStatusRow>()? else {
                    return Ok(rows);
                };
                rows.push(row.0);
            }
            if sequence.next_element::<BoundedStatusRow>()?.is_some() {
                return Err(A::Error::invalid_length(
                    MAX_STATUS_ROWS.saturating_add(1),
                    &self,
                ));
            }
            Ok(rows)
        }
    }

    deserializer.deserialize_seq(StatusRowsVisitor)
}
