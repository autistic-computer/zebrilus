use ala::{FontWeight, Rems};

use super::rems_from_px;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextStyle {
    Micro,
    Caption,
    Small,
    #[default]
    Body,
    Lead,
    Heading,
    Title,
    Display,
    Mono,
    ReadH1,
    ReadH2,
    ReadH3,
    ReadH4,
    ReadH5,
    ReadBody,
}

impl TextStyle {
    pub const ALL: [Self; 15] = [
        Self::Micro,
        Self::Caption,
        Self::Small,
        Self::Body,
        Self::Lead,
        Self::Heading,
        Self::Title,
        Self::Display,
        Self::Mono,
        Self::ReadH1,
        Self::ReadH2,
        Self::ReadH3,
        Self::ReadH4,
        Self::ReadH5,
        Self::ReadBody,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Micro => "micro",
            Self::Caption => "caption",
            Self::Small => "small",
            Self::Body => "body",
            Self::Lead => "lead",
            Self::Heading => "heading",
            Self::Title => "title",
            Self::Display => "display",
            Self::Mono => "mono",
            Self::ReadH1 => "read-h1",
            Self::ReadH2 => "read-h2",
            Self::ReadH3 => "read-h3",
            Self::ReadH4 => "read-h4",
            Self::ReadH5 => "read-h5",
            Self::ReadBody => "read-body",
        }
    }

    pub const fn reading_heading(level: u8) -> Self {
        match level {
            1 => Self::ReadH1,
            2 => Self::ReadH2,
            3 => Self::ReadH3,
            4 => Self::ReadH4,
            _ => Self::ReadH5,
        }
    }

    pub const fn design_px(self) -> f32 {
        match self {
            Self::Micro => 10.0,
            Self::Caption => 11.0,
            Self::Small => 12.0,
            Self::Body | Self::Mono | Self::ReadH5 => 13.0,
            Self::Lead | Self::ReadH4 | Self::ReadBody => 14.0,
            Self::Heading | Self::ReadH3 => 16.0,
            Self::Title | Self::ReadH2 => 20.0,
            Self::Display | Self::ReadH1 => 26.0,
        }
    }

    pub const fn line_height_px(self) -> f32 {
        match self {
            Self::Micro => 14.0,
            Self::Caption | Self::Small => 16.0,
            Self::Body | Self::Mono => 18.0,
            Self::Lead | Self::ReadH5 => 20.0,
            Self::Heading | Self::ReadH4 | Self::ReadBody => 22.0,
            Self::ReadH3 => 24.0,
            Self::Title => 26.0,
            Self::ReadH2 => 28.0,
            Self::Display | Self::ReadH1 => 32.0,
        }
    }

    pub const fn size(self) -> Rems {
        rems_from_px(self.design_px())
    }

    pub const fn line_height(self) -> Rems {
        rems_from_px(self.line_height_px())
    }

    pub const fn weight(self) -> FontWeight {
        match self {
            Self::Heading
            | Self::Title
            | Self::Display
            | Self::ReadH1
            | Self::ReadH2
            | Self::ReadH3
            | Self::ReadH4
            | Self::ReadH5 => FontWeight::SEMIBOLD,
            _ => FontWeight::NORMAL,
        }
    }

    pub const fn strong_weight(self) -> FontWeight {
        match self {
            Self::Micro | Self::Caption | Self::Small | Self::Body | Self::Lead | Self::Mono => {
                FontWeight::MEDIUM
            }
            _ => FontWeight::SEMIBOLD,
        }
    }

    pub const fn is_mono(self) -> bool {
        matches!(self, Self::Mono)
    }
}
