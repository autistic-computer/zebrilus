use zz_protocol::{
    StyledSegment, TmuxAlign, TmuxColour, indexed_colour_rgb, parse_styled_segments,
    parse_tmux_colour,
};

#[must_use]
pub fn split_tmux_alignment(label: &str) -> [Vec<StyledSegment>; 3] {
    let mut buckets: [Vec<StyledSegment>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for segment in parse_styled_segments(label) {
        let bucket = match segment.style.align {
            Some(TmuxAlign::Centre | TmuxAlign::AbsoluteCentre) => 1,
            Some(TmuxAlign::Right) => 2,
            _ => 0,
        };
        buckets[bucket].push(segment);
    }
    buckets
}

#[must_use]
pub fn style_field<'a>(style: &'a str, key: &str) -> Option<&'a str> {
    style.rsplit([',', ' ']).find_map(|part| {
        let (name, value) = part.split_once('=')?;
        name.eq_ignore_ascii_case(key).then_some(value)
    })
}

#[must_use]
pub fn literal_rgb(colour: TmuxColour) -> Option<u32> {
    match colour {
        TmuxColour::Basic(index) | TmuxColour::Indexed(index) => Some(indexed_colour_rgb(index)),
        TmuxColour::Rgb(colour) => Some(colour),
        TmuxColour::Default | TmuxColour::Terminal | TmuxColour::Theme(_) => None,
    }
}

#[must_use]
pub fn literal_style_colour(style: &str, key: &str) -> Option<u32> {
    literal_rgb(parse_tmux_colour(style_field(style, key)?)?)
}
