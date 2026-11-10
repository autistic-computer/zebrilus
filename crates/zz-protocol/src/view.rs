use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

use crate::PaneId;

pub const MAX_VIEW_TEXT_BYTES: usize = 64 * 1024;
pub const MAX_VIEW_LABEL_BYTES: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ViewDescriptor {
    pub origin: Option<PaneId>,
    pub title: String,
    pub content: ViewContent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewContent {
    Diagram {
        language: String,
        source: String,
    },
    Code {
        language: Option<String>,
        text: String,
    },
    Markdown {
        text: String,
    },
    Image {
        item: String,
        index: u32,
    },
    Subagent {
        item: String,
    },
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ViewDescriptorError {
    #[error("view {field} cannot exceed {limit} bytes")]
    TooLong { field: &'static str, limit: usize },
    #[error("view {field} cannot contain control characters")]
    ControlCharacter { field: &'static str },
    #[error("a view of a chat image or subagent needs the pane it comes from")]
    NoOrigin,
}

impl ViewContent {
    #[must_use]
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::Diagram { .. } => "Diagram",
            Self::Code { .. } => "Code",
            Self::Markdown { .. } => "Markdown",
            Self::Image { .. } => "Image",
            Self::Subagent { .. } => "Subagent",
        }
    }

    #[must_use]
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Diagram { source, .. } => Some(source),
            Self::Code { text, .. } | Self::Markdown { text } => Some(text),
            Self::Image { .. } | Self::Subagent { .. } => None,
        }
    }

    fn label(&self) -> Option<&str> {
        match self {
            Self::Diagram { language, .. } => Some(language),
            Self::Code { language, .. } => language.as_deref(),
            Self::Image { item, .. } | Self::Subagent { item } => Some(item),
            Self::Markdown { .. } => None,
        }
    }

    const fn points_into_chat(&self) -> bool {
        matches!(self, Self::Image { .. } | Self::Subagent { .. })
    }
}

impl ViewDescriptor {
    pub fn validate(&self) -> Result<(), ViewDescriptorError> {
        label("title", &self.title)?;
        if let Some(value) = self.content.label() {
            label("label", value)?;
        }
        if self
            .content
            .text()
            .is_some_and(|text| text.len() > MAX_VIEW_TEXT_BYTES)
        {
            return Err(ViewDescriptorError::TooLong {
                field: "text",
                limit: MAX_VIEW_TEXT_BYTES,
            });
        }
        if self.content.points_into_chat() && self.origin.is_none() {
            return Err(ViewDescriptorError::NoOrigin);
        }
        Ok(())
    }
}

fn label(field: &'static str, value: &str) -> Result<(), ViewDescriptorError> {
    if value.len() > MAX_VIEW_LABEL_BYTES {
        return Err(ViewDescriptorError::TooLong {
            field,
            limit: MAX_VIEW_LABEL_BYTES,
        });
    }
    if value.chars().any(char::is_control) {
        return Err(ViewDescriptorError::ControlCharacter { field });
    }
    Ok(())
}

#[derive(Deserialize)]
struct ViewDescriptorWire {
    origin: Option<PaneId>,
    title: String,
    content: ViewContent,
}

impl<'de> Deserialize<'de> for ViewDescriptor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ViewDescriptorWire::deserialize(deserializer)?;
        let descriptor = Self {
            origin: wire.origin,
            title: wire.title,
            content: wire.content,
        };
        descriptor.validate().map_err(serde::de::Error::custom)?;
        Ok(descriptor)
    }
}
