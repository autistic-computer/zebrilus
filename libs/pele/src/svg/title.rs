use crate::RenderOptions;
use crate::svg::{label_with_options, num};
use crate::text::{BOLD, Label, layout_label};

pub fn title_label(
    title: Option<&str>,
    options: &RenderOptions,
    size: f64,
    max_width: f64,
) -> Label {
    title.map_or_else(Label::default, |title| {
        layout_label(title, false, options.text_measurer(), size, max_width, BOLD)
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct Titled {
    pub width: f64,
    pub height: f64,
    pub content: String,
}

pub fn with_title(
    title: Option<&str>,
    options: &RenderOptions,
    size: f64,
    pad: f64,
    width: f64,
    height: f64,
    content: String,
) -> Titled {
    let label = title_label(title, options, size, 4000.0);
    if label.height == 0.0 {
        return Titled {
            width,
            height,
            content,
        };
    }
    let shift = label.height + 12.0;
    let full = width.max((label.width + 2.0 * pad).ceil());
    Titled {
        width: full,
        height: (height + shift).ceil(),
        content: format!(
            "{}<g transform=\"translate({},{})\">{content}</g>",
            label_with_options(
                &label,
                full / 2.0,
                pad + label.height / 2.0,
                " class=\"pele-title\" font-weight=\"bold\"",
                options,
            ),
            num((full - width) / 2.0),
            num(shift),
        ),
    }
}
