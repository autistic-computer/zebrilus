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
}

impl TextStyle {
    pub const ALL: [Self; 9] = [
        Self::Micro,
        Self::Caption,
        Self::Small,
        Self::Body,
        Self::Lead,
        Self::Heading,
        Self::Title,
        Self::Display,
        Self::Mono,
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
        }
    }

    pub const fn design_px(self) -> f32 {
        match self {
            Self::Micro => 10.0,
            Self::Caption => 11.0,
            Self::Small => 12.0,
            Self::Body | Self::Mono => 13.0,
            Self::Lead => 14.0,
            Self::Heading => 16.0,
            Self::Title => 20.0,
            Self::Display => 26.0,
        }
    }

    pub const fn line_height_px(self) -> f32 {
        match self {
            Self::Micro => 14.0,
            Self::Caption | Self::Small => 16.0,
            Self::Body | Self::Mono => 18.0,
            Self::Lead => 20.0,
            Self::Heading => 22.0,
            Self::Title => 26.0,
            Self::Display => 32.0,
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
            Self::Heading | Self::Title | Self::Display => FontWeight::SEMIBOLD,
            _ => FontWeight::NORMAL,
        }
    }

    pub const fn strong_weight(self) -> FontWeight {
        match self {
            Self::Heading | Self::Title | Self::Display => FontWeight::SEMIBOLD,
            _ => FontWeight::MEDIUM,
        }
    }

    pub const fn is_mono(self) -> bool {
        matches!(self, Self::Mono)
    }
}
