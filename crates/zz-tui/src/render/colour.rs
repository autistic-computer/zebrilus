use super::*;

pub(super) const RGB_COLOURS: u32 = 16_777_216;

pub(super) fn downgrade_palette(index: u8, colours: u32) -> u8 {
    if colours >= 256 || (index < 16 && colours >= 16) {
        return index;
    }
    let mapped = colour_256to16(index);
    if colours >= 16 { mapped } else { mapped & 7 }
}

fn colour_256to16(index: u8) -> u8 {
    #[rustfmt::skip]
    const TABLE: [u8; 256] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
    0, 4, 4, 4, 12, 12, 2, 6, 4, 4, 12, 12, 2, 2, 6, 4,
    12, 12, 2, 2, 2, 6, 12, 12, 10, 10, 10, 10, 14, 12, 10, 10,
    10, 10, 10, 14, 1, 5, 4, 4, 12, 12, 3, 8, 4, 4, 12, 12,
    2, 2, 6, 4, 12, 12, 2, 2, 2, 6, 12, 12, 10, 10, 10, 10,
    14, 12, 10, 10, 10, 10, 10, 14, 1, 1, 5, 4, 12, 12, 1, 1,
    5, 4, 12, 12, 3, 3, 8, 4, 12, 12, 2, 2, 2, 6, 12, 12,
    10, 10, 10, 10, 14, 12, 10, 10, 10, 10, 10, 14, 1, 1, 1, 5,
    12, 12, 1, 1, 1, 5, 12, 12, 1, 1, 1, 5, 12, 12, 3, 3,
    3, 7, 12, 12, 10, 10, 10, 10, 14, 12, 10, 10, 10, 10, 10, 14,
    9, 9, 9, 9, 13, 12, 9, 9, 9, 9, 13, 12, 9, 9, 9, 9,
    13, 12, 9, 9, 9, 9, 13, 12, 11, 11, 11, 11, 7, 12, 10, 10,
    10, 10, 10, 14, 9, 9, 9, 9, 9, 13, 9, 9, 9, 9, 9, 13,
    9, 9, 9, 9, 9, 13, 9, 9, 9, 9, 9, 13, 9, 9, 9, 9,
    9, 13, 11, 11, 11, 11, 11, 15, 0, 0, 0, 0, 0, 0, 8, 8,
    8, 8, 8, 8, 7, 7, 7, 7, 7, 7, 15, 15, 15, 15, 15, 15,
    ];
    TABLE[usize::from(index)]
}

pub(super) fn colour_find_rgb(colour: Color) -> u8 {
    const Q2C: [i32; 6] = [0x00, 0x5f, 0x87, 0xaf, 0xd7, 0xff];
    let (r, g, b) = (
        i32::from(colour.r),
        i32::from(colour.g),
        i32::from(colour.b),
    );
    let (qr, qg, qb) = (colour_to_6cube(r), colour_to_6cube(g), colour_to_6cube(b));
    let (cr, cg, cb) = (Q2C[qr as usize], Q2C[qg as usize], Q2C[qb as usize]);
    let cube = 16 + (36 * qr) + (6 * qg) + qb;
    if cr == r && cg == g && cb == b {
        return cube as u8;
    }
    let grey_average = (r + g + b) / 3;
    let grey_index = if grey_average > 238 {
        23
    } else {
        (grey_average - 3) / 10
    };
    let grey = 8 + (10 * grey_index);
    if colour_distance(grey, grey, grey, r, g, b) < colour_distance(cr, cg, cb, r, g, b) {
        (232 + grey_index) as u8
    } else {
        cube as u8
    }
}

fn colour_to_6cube(value: i32) -> i32 {
    if value < 48 {
        0
    } else if value < 114 {
        1
    } else {
        (value - 35) / 40
    }
}

fn colour_distance(red: i32, green: i32, blue: i32, r: i32, g: i32, b: i32) -> i32 {
    (red - r) * (red - r) + (green - g) * (green - g) + (blue - b) * (blue - b)
}

pub(super) const DARK_THEME_COLOURS: [&str; 10] = [
    "gray5",
    "gray90",
    "gray70",
    "gray15",
    "yellowgreen",
    "darkgoldenrod",
    "indianred",
    "skyblue3",
    "cadetblue",
    "mediumpurple",
];

pub(super) fn theme_colour(index: u8) -> Option<Color> {
    DARK_THEME_COLOURS
        .get(usize::from(index))
        .and_then(|name| zz_terminal::parse_x11_color(name))
}

pub(super) fn resolve_tmux_colour(
    colour: TmuxColour,
    fallback: Color,
    appearance: &TerminalAppearance,
) -> Color {
    match colour {
        TmuxColour::Basic(index) | TmuxColour::Indexed(index) => {
            appearance.palette[usize::from(index)]
        }
        TmuxColour::Rgb(value) => Color::from_packed(value),
        TmuxColour::Theme(index) => theme_colour(index).unwrap_or(fallback),
        TmuxColour::Default | TmuxColour::Terminal => fallback,
    }
}
