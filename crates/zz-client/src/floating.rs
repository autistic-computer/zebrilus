use zz_protocol::{
    CommandInvocation, FloatingPaneSnapshot, MuxSnapshot, PaneBorderLines, PaneBorderStatus,
    PaneId, TmuxAlign,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FloatCells {
    pub xoff: i32,
    pub yoff: i32,
    pub sx: u16,
    pub sy: u16,
}

impl From<&FloatingPaneSnapshot> for FloatCells {
    fn from(float: &FloatingPaneSnapshot) -> Self {
        Self {
            xoff: float.xoff,
            yoff: float.yoff,
            sx: float.sx,
            sy: float.sy,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PixelRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl PixelRect {
    fn clipped(self, width: f32, height: f32) -> Option<Self> {
        let left = self.x.max(0.0);
        let top = self.y.max(0.0);
        let right = (self.x + self.width).min(width);
        let bottom = (self.y + self.height).min(height);
        (right > left && bottom > top).then_some(Self {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloatPixels {
    pub frame: PixelRect,
    pub content: Option<PixelRect>,
}

#[must_use]
#[allow(
    clippy::cast_precision_loss,
    reason = "window cell offsets are far below f32's exact integer range"
)]
pub fn float_pixels(
    float: FloatCells,
    bordered: bool,
    cell: (f32, f32),
    canvas: (f32, f32),
) -> Option<FloatPixels> {
    let pad = if bordered { 1.0 } else { 0.0 };
    let content = PixelRect {
        x: float.xoff as f32 * cell.0,
        y: float.yoff as f32 * cell.1,
        width: f32::from(float.sx) * cell.0,
        height: f32::from(float.sy) * cell.1,
    };
    let frame = PixelRect {
        x: content.x - pad * cell.0,
        y: content.y - pad * cell.1,
        width: content.width + 2.0 * pad * cell.0,
        height: content.height + 2.0 * pad * cell.1,
    };
    Some(FloatPixels {
        frame: frame.clipped(canvas.0, canvas.1)?,
        content: content.clipped(canvas.0, canvas.1),
    })
}

#[must_use]
#[allow(
    clippy::cast_precision_loss,
    reason = "window cell offsets are far below f32's exact integer range"
)]
pub fn float_placement(
    float: &FloatingPaneSnapshot,
    cell: (f32, f32),
    canvas: (f32, f32),
) -> Option<(PixelRect, PixelRect)> {
    let bordered = float.border_lines != PaneBorderLines::None;
    if canvas.0 > 0.0
        && canvas.1 > 0.0
        && float_pixels(FloatCells::from(float), bordered, cell, canvas).is_none()
    {
        return None;
    }
    let pad = i32::from(bordered);
    let rect = |x: i32, y: i32, width: i32, height: i32| PixelRect {
        x: x as f32 * cell.0,
        y: y as f32 * cell.1,
        width: width as f32 * cell.0,
        height: height as f32 * cell.1,
    };
    Some((
        rect(
            float.xoff - pad,
            float.yoff - pad,
            i32::from(float.sx) + 2 * pad,
            i32::from(float.sy) + 2 * pad,
        ),
        rect(
            float.xoff,
            float.yoff,
            i32::from(float.sx),
            i32::from(float.sy),
        ),
    ))
}

pub const PIP_MARGIN: f32 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PipGeometry {
    pub window: (u16, u16),
    pub cell: (f32, f32),
    pub canvas: (f32, f32),
    pub lines: PaneBorderLines,
    pub header: f32,
}

impl PipGeometry {
    fn bordered(&self) -> bool {
        self.lines != PaneBorderLines::None
    }

    #[must_use]
    pub fn size(&self, cells: FloatCells) -> (f32, f32) {
        let (width, height) = (
            self.cell.0 * f32::from(cells.sx),
            self.cell.1 * f32::from(cells.sy),
        );
        if self.bordered() {
            (
                width + self.cell.0 * 2.0,
                height + self.header.max(self.cell.1) + self.cell.1,
            )
        } else {
            (width, height)
        }
    }

    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "window cell offsets are far below f32's exact integer range"
    )]
    pub fn frame(&self, cells: FloatCells) -> PixelRect {
        let size = self.size(cells);
        let area = self.area();
        let pad = i32::from(self.bordered());
        let axis = |offset: i32, extent: u16, window: u16, frame: f32, canvas: f32, cell: f32| {
            if window == 0 {
                return cell * (offset - pad) as f32;
            }
            let free = canvas - frame;
            if free <= 0.0 {
                return 0.0;
            }
            let room = f32::from(window) - f32::from(extent) - 2.0 * pad as f32;
            let along = if room > 0.0 {
                ((offset - pad) as f32 / room).clamp(0.0, 1.0)
            } else {
                0.5
            };
            let margin = PIP_MARGIN.min(free / 2.0);
            margin + (free - margin * 2.0) * along
        };
        PixelRect {
            x: axis(
                cells.xoff,
                cells.sx,
                self.window.0,
                size.0,
                area.0,
                self.cell.0,
            ),
            y: axis(
                cells.yoff,
                cells.sy,
                self.window.1,
                size.1,
                area.1,
                self.cell.1,
            ),
            width: size.0,
            height: size.1,
        }
    }

    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the cell count is floored and clamped to u16 before the cast"
    )]
    pub fn max_cells(&self) -> (u16, u16) {
        let area = self.area();
        let chrome = self.size(FloatCells {
            xoff: 0,
            yoff: 0,
            sx: 0,
            sy: 0,
        });
        let pad = if self.bordered() { 2 } else { 0 };
        let fit = |room: f32, chrome: f32, cell: f32, window: u16| {
            let fits = if cell > 0.0 {
                ((room - PIP_MARGIN * 2.0 - chrome) / cell)
                    .floor()
                    .clamp(1.0, f32::from(u16::MAX)) as u16
            } else {
                u16::MAX
            };
            if window == 0 {
                fits
            } else {
                fits.min(window.saturating_sub(pad).max(1))
            }
        };
        (
            fit(area.0, chrome.0, self.cell.0, self.window.0),
            fit(area.1, chrome.1, self.cell.1, self.window.1),
        )
    }

    #[must_use]
    pub fn fill_rows(&self) -> u16 {
        self.max_cells().1 + if self.bordered() { 2 } else { 0 }
    }

    #[must_use]
    pub fn area(&self) -> (f32, f32) {
        if self.canvas.0 > 0.0 && self.canvas.1 > 0.0 {
            self.canvas
        } else {
            (
                self.cell.0 * f32::from(self.window.0),
                self.cell.1 * f32::from(self.window.1),
            )
        }
    }

    #[must_use]
    pub fn place_at(&self, centre: (f32, f32)) -> FloatPlace {
        let area = self.area();
        let fraction = |at: f32, extent: f32, cells: u16| {
            if extent > 0.0 {
                at / extent * f32::from(cells)
            } else {
                0.0
            }
        };
        FloatPlace::zone(
            (
                fraction(centre.0, area.0, self.window.0),
                fraction(centre.1, area.1, self.window.1),
            ),
            self.window,
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FloatTitle {
    pub left: String,
    pub right: String,
}

#[must_use]
pub fn float_title(border_status_text: &str) -> FloatTitle {
    let mut title = FloatTitle::default();
    for segment in zz_protocol::parse_styled_segments(border_status_text) {
        if segment.style.align == Some(TmuxAlign::Right) {
            title.right.push_str(&segment.text);
        } else {
            title.left.push_str(&segment.text);
        }
    }
    title.left.truncate(title.left.trim_end().len());
    title.right = title.right.trim().to_owned();
    title
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FloatGrip {
    pub left: bool,
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
}

impl FloatGrip {
    pub const MOVE: Self = Self {
        left: false,
        top: false,
        right: false,
        bottom: false,
    };

    #[must_use]
    pub const fn is_move(self) -> bool {
        !(self.left || self.top || self.right || self.bottom)
    }

    #[must_use]
    pub const fn moves_origin(self) -> bool {
        self.is_move() || self.left || self.top
    }
}

#[must_use]
pub fn float_drag_preview(start: FloatCells, grip: FloatGrip, delta: (i32, i32)) -> FloatCells {
    if grip.is_move() {
        return FloatCells {
            xoff: start.xoff + delta.0,
            yoff: start.yoff + delta.1,
            ..start
        };
    }
    let (xoff, sx) = drag_axis(start.xoff, start.sx, grip.left, grip.right, delta.0);
    let (yoff, sy) = drag_axis(start.yoff, start.sy, grip.top, grip.bottom, delta.1);
    FloatCells { xoff, yoff, sx, sy }
}

#[must_use]
pub fn float_drag_within(target: FloatCells, grip: FloatGrip, max: (u16, u16)) -> FloatCells {
    if grip.is_move() {
        return target;
    }
    let axis = |offset: i32, size: u16, near: bool, max: u16| {
        let fitted = size.min(max.max(1));
        let offset = if near {
            offset + i32::from(size - fitted)
        } else {
            offset
        };
        (offset, fitted)
    };
    let (xoff, sx) = axis(target.xoff, target.sx, grip.left, max.0);
    let (yoff, sy) = axis(target.yoff, target.sy, grip.top, max.1);
    FloatCells { xoff, yoff, sx, sy }
}

fn drag_axis(offset: i32, size: u16, near: bool, far: bool, delta: i32) -> (i32, u16) {
    let size = i32::from(size);
    let (offset, size) = if near {
        let delta = delta.min(size - 1);
        (offset + delta, size - delta)
    } else if far {
        (offset, (size + delta).max(1))
    } else {
        (offset, size)
    };
    (offset, u16::try_from(size).unwrap_or(u16::MAX))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FloatWindow {
    pub border_status: PaneBorderStatus,
    pub rows: u16,
}

#[must_use]
pub fn float_drag_command(
    pane: PaneId,
    grip: FloatGrip,
    start: FloatCells,
    target: FloatCells,
    lines: PaneBorderLines,
    window: FloatWindow,
) -> CommandInvocation {
    let pad = i32::from(lines != PaneBorderLines::None);
    let move_pane = format!(
        "move-pane -t {pane} -X {} -Y {}",
        target.xoff - pad,
        target.yoff - pad
    );
    if grip.is_move() {
        return CommandInvocation::new(
            "move-pane",
            move_pane.split(' ').skip(1).map(str::to_owned),
        );
    }
    let status_row = match window.border_status {
        PaneBorderStatus::Top => start.yoff == 1,
        PaneBorderStatus::Bottom => start.yoff + i32::from(start.sy) == i32::from(window.rows) - 1,
        PaneBorderStatus::Off => false,
    };
    let height = i32::from(target.sy) + 2 * pad - i32::from(status_row);
    let resize_pane = format!(
        "resize-pane -t {pane} -x {} -y {height}",
        i32::from(target.sx) + 2 * pad
    );
    if grip.moves_origin() {
        return CommandInvocation::new(
            "run-shell",
            ["-C".to_owned(), format!("{resize_pane} ; {move_pane}")],
        );
    }
    CommandInvocation::new(
        "resize-pane",
        resize_pane.split(' ').skip(1).map(str::to_owned),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FloatPlace {
    TopLeft,
    TopCentre,
    TopRight,
    CentreLeft,
    Centre,
    CentreRight,
    BottomLeft,
    BottomCentre,
    BottomRight,
}

impl FloatPlace {
    pub const ALL: [Self; 9] = [
        Self::TopLeft,
        Self::TopCentre,
        Self::TopRight,
        Self::CentreLeft,
        Self::Centre,
        Self::CentreRight,
        Self::BottomLeft,
        Self::BottomCentre,
        Self::BottomRight,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::TopLeft => "top-left",
            Self::TopCentre => "top-centre",
            Self::TopRight => "top-right",
            Self::CentreLeft => "centre-left",
            Self::Centre => "centre",
            Self::CentreRight => "centre-right",
            Self::BottomLeft => "bottom-left",
            Self::BottomCentre => "bottom-centre",
            Self::BottomRight => "bottom-right",
        }
    }

    #[must_use]
    pub fn cells(
        self,
        float: FloatCells,
        window: (u16, u16),
        lines: PaneBorderLines,
    ) -> FloatCells {
        let border = i32::from(lines != PaneBorderLines::None);
        let (wx, wy) = (i32::from(window.0), i32::from(window.1));
        let (px, py) = (i32::from(float.sx), i32::from(float.sy));
        let left = border;
        let centre_x = (wx - px) / 2;
        let right = wx - px - border;
        let top = border;
        let centre_y = (wy - py) / 2;
        let bottom = wy - py - border;
        let (xoff, yoff) = match self {
            Self::TopLeft => (left, top),
            Self::TopCentre => (centre_x, top),
            Self::TopRight => (right, top),
            Self::CentreLeft => (left, centre_y),
            Self::Centre => (centre_x, centre_y),
            Self::CentreRight => (right, centre_y),
            Self::BottomLeft => (left, bottom),
            Self::BottomCentre => (centre_x, bottom),
            Self::BottomRight => (right, bottom),
        };
        FloatCells {
            xoff,
            yoff,
            ..float
        }
    }

    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the third is clamped to 0..=2 before it indexes"
    )]
    pub fn zone(point: (f32, f32), window: (u16, u16)) -> Self {
        let third = |at: f32, size: u16| {
            (at / f32::from(size.max(1)) * 3.0).floor().clamp(0.0, 2.0) as usize
        };
        Self::ALL[third(point.1, window.1) * 3 + third(point.0, window.0)]
    }

    #[must_use]
    pub fn command(self, pane: PaneId) -> CommandInvocation {
        CommandInvocation::new(
            "move-pane",
            [
                "-t".to_owned(),
                pane.to_string(),
                "-P".to_owned(),
                self.name().to_owned(),
            ],
        )
    }
}

impl FloatCells {
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "window cell offsets are far below f32's exact integer range"
    )]
    pub fn centre(self) -> (f32, f32) {
        (
            self.xoff as f32 + f32::from(self.sx) / 2.0,
            self.yoff as f32 + f32::from(self.sy) / 2.0,
        )
    }
}

#[must_use]
pub fn tile_pane_command(pane: PaneId) -> CommandInvocation {
    CommandInvocation::new(
        "join-pane",
        [
            "-s".to_owned(),
            pane.to_string(),
            "-t".to_owned(),
            pane.to_string(),
        ],
    )
}

#[must_use]
pub fn zoom_pane_command(pane: PaneId) -> CommandInvocation {
    CommandInvocation::new(
        "resize-pane",
        ["-Z".to_owned(), "-t".to_owned(), pane.to_string()],
    )
}

#[must_use]
pub fn float_pane_commands(pane: PaneId, rows: Option<u16>) -> [CommandInvocation; 2] {
    [
        CommandInvocation::new(
            "break-pane",
            [
                "-W".to_owned(),
                "-x".to_owned(),
                "33%".to_owned(),
                "-y".to_owned(),
                rows.map_or_else(|| "100%".to_owned(), |rows| rows.to_string()),
                "-s".to_owned(),
                pane.to_string(),
            ],
        ),
        FloatPlace::BottomRight.command(pane),
    ]
}

#[must_use]
pub fn pane_floats(snapshot: &MuxSnapshot, pane: PaneId) -> bool {
    snapshot
        .sessions
        .iter()
        .flat_map(|session| &session.windows)
        .any(|window| window.floating.iter().any(|float| float.pane == pane))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlayFrame {
    pub bounds: PixelRect,
    pub inset_x: f32,
    pub inset_y: f32,
}

#[must_use]
pub fn overlay_frame(
    (left, top, width, height): (u16, u16, u16, u16),
    (client_columns, client_rows): (u16, u16),
    (cell_width_px, cell_height_px): (u32, u32),
    bordered: bool,
    (canvas_width, canvas_height): (f32, f32),
    scale: f32,
) -> OverlayFrame {
    let cell_width = f32::from(u16::try_from(cell_width_px).unwrap_or(u16::MAX)) / scale;
    let cell_height = f32::from(u16::try_from(cell_height_px).unwrap_or(u16::MAX)) / scale;
    let grid_left = ((canvas_width - cell_width * f32::from(client_columns)) / 2.0).max(0.0);
    let grid_top = ((canvas_height - cell_height * f32::from(client_rows)) / 2.0).max(0.0);
    OverlayFrame {
        bounds: PixelRect {
            x: grid_left + cell_width * f32::from(left),
            y: grid_top + cell_height * f32::from(top),
            width: cell_width * f32::from(width),
            height: cell_height * f32::from(height),
        },
        inset_x: if bordered { cell_width } else { 0.0 },
        inset_y: if bordered { cell_height } else { 0.0 },
    }
}

#[must_use]
#[allow(
    clippy::cast_precision_loss,
    reason = "a prompt's display width is far below f32's exact integer range"
)]
pub fn confirm_size(prompt: &str, canvas_width: f32) -> (f32, f32) {
    let width = (zz_protocol::display_width(prompt)
        .saturating_mul(8)
        .saturating_add(32)) as f32;
    (width.max(180.0).min((canvas_width - 24.0).max(180.0)), 48.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLOAT: FloatCells = FloatCells {
        xoff: 10,
        yoff: 4,
        sx: 20,
        sy: 6,
    };
    const PLAIN: FloatWindow = FloatWindow {
        border_status: PaneBorderStatus::Off,
        rows: 24,
    };

    fn words(command: &CommandInvocation) -> String {
        std::iter::once(command.name.clone())
            .chain(command.args.iter().map(ToString::to_string))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn list(command: &CommandInvocation) -> Vec<Vec<String>> {
        if command.name == "run-shell" {
            return command.args[1]
                .split(" ; ")
                .map(|part| part.split(' ').map(str::to_owned).collect())
                .collect();
        }
        vec![words(command).split(' ').map(str::to_owned).collect()]
    }

    fn apply(start: FloatCells, window: FloatWindow, command: &CommandInvocation) -> FloatCells {
        let mut float = start;
        for words in list(command) {
            let value = |flag: &str| {
                words
                    .iter()
                    .position(|word| word == flag)
                    .and_then(|index| words.get(index + 1))
                    .and_then(|value| value.parse::<i32>().ok())
            };
            match words[0].as_str() {
                "resize-pane" => {
                    let status_row = match window.border_status {
                        PaneBorderStatus::Top => float.yoff == 1,
                        PaneBorderStatus::Bottom => {
                            float.yoff + i32::from(float.sy) == i32::from(window.rows) - 1
                        }
                        PaneBorderStatus::Off => false,
                    };
                    float.sx = u16::try_from(value("-x").unwrap() - 2).unwrap();
                    float.sy =
                        u16::try_from(value("-y").unwrap() + i32::from(status_row) - 2).unwrap();
                }
                "move-pane" => {
                    float.xoff = value("-X").unwrap() + 1;
                    float.yoff = value("-Y").unwrap() + 1;
                }
                other => panic!("unexpected {other}"),
            }
        }
        float
    }

    fn grip(edges: &str) -> FloatGrip {
        FloatGrip {
            left: edges.contains('l'),
            top: edges.contains('t'),
            right: edges.contains('r'),
            bottom: edges.contains('b'),
        }
    }

    #[test]
    fn cells_become_canvas_pixels_with_the_border_outside_the_content() {
        let placed = float_pixels(FLOAT, true, (8.0, 16.0), (800.0, 600.0)).unwrap();
        assert_eq!(
            placed.frame,
            PixelRect {
                x: 72.0,
                y: 48.0,
                width: 176.0,
                height: 128.0,
            }
        );
        assert_eq!(
            placed.content,
            Some(PixelRect {
                x: 80.0,
                y: 64.0,
                width: 160.0,
                height: 96.0,
            })
        );
        let bare = float_pixels(FLOAT, false, (8.0, 16.0), (800.0, 600.0)).unwrap();
        assert_eq!(bare.frame, bare.content.unwrap());
    }

    #[test]
    fn a_float_past_the_canvas_edges_is_clipped_and_one_off_it_is_gone() {
        let hanging = FloatCells {
            xoff: -3,
            yoff: 30,
            sx: 10,
            sy: 10,
        };
        let placed = float_pixels(hanging, true, (8.0, 16.0), (400.0, 560.0)).unwrap();
        assert_eq!(
            placed.frame,
            PixelRect {
                x: 0.0,
                y: 464.0,
                width: 64.0,
                height: 96.0,
            }
        );
        assert_eq!(
            placed.content,
            Some(PixelRect {
                x: 0.0,
                y: 480.0,
                width: 56.0,
                height: 80.0,
            })
        );
        let gone = FloatCells { xoff: 60, ..FLOAT };
        assert_eq!(float_pixels(gone, true, (8.0, 16.0), (400.0, 560.0)), None);
    }

    #[test]
    fn a_move_sends_move_pane_with_the_outer_corner() {
        let target = float_drag_preview(FLOAT, FloatGrip::MOVE, (5, -2));
        let command = float_drag_command(
            PaneId(3),
            FloatGrip::MOVE,
            FLOAT,
            target,
            PaneBorderLines::Single,
            PLAIN,
        );
        assert_eq!(words(&command), "move-pane -t %3 -X 14 -Y 1");
        assert_eq!(apply(FLOAT, PLAIN, &command), target);
        let bare = float_drag_command(
            PaneId(3),
            FloatGrip::MOVE,
            FLOAT,
            target,
            PaneBorderLines::None,
            PLAIN,
        );
        assert_eq!(words(&bare), "move-pane -t %3 -X 15 -Y 2");
    }

    #[test]
    fn a_right_or_bottom_edge_sends_only_resize_pane() {
        for (grip, delta) in [
            (grip("r"), (4, 0)),
            (grip("b"), (0, 3)),
            (grip("rb"), (-2, -1)),
        ] {
            let target = float_drag_preview(FLOAT, grip, delta);
            let command = float_drag_command(
                PaneId(3),
                grip,
                FLOAT,
                target,
                PaneBorderLines::Single,
                PLAIN,
            );
            assert_eq!(command.name, "resize-pane");
            assert_eq!(apply(FLOAT, PLAIN, &command), target);
            assert_eq!((target.xoff, target.yoff), (FLOAT.xoff, FLOAT.yoff));
        }
        let right = grip("r");
        let target = float_drag_preview(FLOAT, right, (4, 0));
        assert_eq!(
            words(&float_drag_command(
                PaneId(3),
                right,
                FLOAT,
                target,
                PaneBorderLines::Single,
                PLAIN
            )),
            "resize-pane -t %3 -x 26 -y 8"
        );
    }

    #[test]
    fn a_left_top_or_top_left_resize_is_one_command_list_landing_on_the_preview() {
        for (grip, delta) in [
            (grip("l"), (-4, 0)),
            (grip("t"), (0, 2)),
            (grip("lt"), (3, -2)),
        ] {
            let target = float_drag_preview(FLOAT, grip, delta);
            let command = float_drag_command(
                PaneId(7),
                grip,
                FLOAT,
                target,
                PaneBorderLines::Single,
                PLAIN,
            );
            assert_eq!(command.name, "run-shell");
            let names: Vec<String> = list(&command)
                .iter()
                .map(|words| words[0].clone())
                .collect();
            assert_eq!(names, ["resize-pane", "move-pane"]);
            assert_eq!(apply(FLOAT, PLAIN, &command), target);
            assert_eq!(
                target.xoff + i32::from(target.sx),
                FLOAT.xoff + i32::from(FLOAT.sx)
            );
            assert_eq!(
                target.yoff + i32::from(target.sy),
                FLOAT.yoff + i32::from(FLOAT.sy)
            );
        }
        let corner = grip("lt");
        let target = float_drag_preview(FLOAT, corner, (3, -2));
        assert_eq!(
            words(&float_drag_command(
                PaneId(7),
                corner,
                FLOAT,
                target,
                PaneBorderLines::Single,
                PLAIN
            )),
            "run-shell -C resize-pane -t %7 -x 19 -y 10 ; move-pane -t %7 -X 12 -Y 1"
        );
    }

    #[test]
    fn a_resize_on_the_row_under_a_pane_status_takes_the_status_row_back() {
        let top = FloatWindow {
            border_status: PaneBorderStatus::Top,
            rows: 24,
        };
        let under_status = FloatCells { yoff: 1, ..FLOAT };
        for (grip, delta) in [
            (grip("r"), (4, 0)),
            (grip("b"), (0, 3)),
            (grip("lt"), (2, 1)),
        ] {
            let target = float_drag_preview(under_status, grip, delta);
            let command = float_drag_command(
                PaneId(2),
                grip,
                under_status,
                target,
                PaneBorderLines::Single,
                top,
            );
            assert_eq!(apply(under_status, top, &command), target);
        }
        let bottom = FloatWindow {
            border_status: PaneBorderStatus::Bottom,
            rows: 24,
        };
        let over_status = FloatCells { yoff: 17, ..FLOAT };
        let right = grip("r");
        let target = float_drag_preview(over_status, right, (4, 0));
        let command = float_drag_command(
            PaneId(2),
            right,
            over_status,
            target,
            PaneBorderLines::Single,
            bottom,
        );
        assert_eq!(words(&command), "resize-pane -t %2 -x 26 -y 7");
        assert_eq!(apply(over_status, bottom, &command), target);
    }

    #[test]
    fn places_match_move_pane_p_on_an_80_by_24_window() {
        let window = (80, 24);
        let at = |place: FloatPlace, lines| {
            let cells = place.cells(FLOAT, window, lines);
            (cells.xoff, cells.yoff)
        };
        let single = PaneBorderLines::Single;
        assert_eq!(at(FloatPlace::TopLeft, single), (1, 1));
        assert_eq!(at(FloatPlace::TopCentre, single), (30, 1));
        assert_eq!(at(FloatPlace::TopRight, single), (59, 1));
        assert_eq!(at(FloatPlace::CentreLeft, single), (1, 9));
        assert_eq!(at(FloatPlace::Centre, single), (30, 9));
        assert_eq!(at(FloatPlace::CentreRight, single), (59, 9));
        assert_eq!(at(FloatPlace::BottomLeft, single), (1, 17));
        assert_eq!(at(FloatPlace::BottomCentre, single), (30, 17));
        assert_eq!(at(FloatPlace::BottomRight, single), (59, 17));
        assert_eq!(at(FloatPlace::BottomRight, PaneBorderLines::None), (60, 18));
        assert_eq!(FloatPlace::Centre.cells(FLOAT, window, single).sx, FLOAT.sx);
    }

    #[test]
    fn the_window_splits_into_nine_zones_and_a_place_lands_with_one_move_pane() {
        let window = (90, 30);
        assert_eq!(
            FloatPlace::zone((89.0, 29.0), window),
            FloatPlace::BottomRight
        );
        assert_eq!(FloatPlace::zone((45.0, 15.0), window), FloatPlace::Centre);
        assert_eq!(
            FloatPlace::zone((29.9, 10.0), window),
            FloatPlace::CentreLeft
        );
        assert_eq!(FloatPlace::zone((30.0, 9.9), window), FloatPlace::TopCentre);
        assert_eq!(FloatPlace::zone((-200.0, 3.0), window), FloatPlace::TopLeft);
        assert_eq!(
            FloatPlace::zone((500.0, 500.0), window),
            FloatPlace::BottomRight
        );
        assert_eq!(
            words(&FloatPlace::TopCentre.command(PaneId(4))),
            "move-pane -t %4 -P top-centre"
        );
    }

    #[test]
    fn float_asks_for_a_third_of_the_width_and_the_height_that_fits() {
        let [float, place] = float_pane_commands(PaneId(7), Some(40));
        assert_eq!(words(&float), "break-pane -W -x 33% -y 40 -s %7");
        assert_eq!(words(&place), "move-pane -t %7 -P bottom-right");
        let [float, _] = float_pane_commands(PaneId(7), None);
        assert_eq!(words(&float), "break-pane -W -x 33% -y 100% -s %7");
    }

    #[test]
    fn a_resize_never_collapses_the_content() {
        let target = float_drag_preview(FLOAT, grip("l"), (50, 0));
        assert_eq!(target.sx, 1);
        assert_eq!(target.xoff, FLOAT.xoff + i32::from(FLOAT.sx) - 1);
    }

    #[test]
    fn a_resize_stops_at_the_largest_float_that_fits_and_keeps_the_far_edge() {
        let grown = float_drag_preview(FLOAT, grip("lb"), (-30, 40));
        let fitted = float_drag_within(grown, grip("lb"), (40, 16));
        assert_eq!((fitted.sx, fitted.sy), (40, 16));
        assert_eq!(
            fitted.xoff + i32::from(fitted.sx),
            FLOAT.xoff + i32::from(FLOAT.sx)
        );
        assert_eq!(fitted.yoff, FLOAT.yoff);
        let small = float_drag_preview(FLOAT, grip("r"), (2, 0));
        assert_eq!(float_drag_within(small, grip("r"), (40, 16)), small);
    }

    #[test]
    fn a_float_title_drops_tmux_markup_and_sets_the_right_aligned_controls_apart() {
        let title = |left: &str, right: &str| FloatTitle {
            left: left.to_owned(),
            right: right.to_owned(),
        };
        assert_eq!(
            float_title(concat!(
                "#[reverse]0#[default] \"zsh\"#[align=right]",
                "#[range=control|7][t]#[norange]",
                "#[range=control|8][z]#[norange]",
                "#[range=control|9][x]#[norange]",
            )),
            title("0 \"zsh\"", "[t][z][x]")
        );
        assert_eq!(float_title("1#[default] \"vim\""), title("1 \"vim\"", ""));
        assert_eq!(float_title("#[fg=red]modal"), title("modal", ""));
        assert_eq!(float_title("100##[x]"), title("100#[x]", ""));
        assert_eq!(float_title(""), title("", ""));
    }
}
