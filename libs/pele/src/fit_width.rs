use crate::{PeleError, RenderOptions, RenderResult};

pub fn title_room(options: &RenderOptions) -> f64 {
    options
        .max_width
        .filter(|available| *available > 0.0)
        .map_or(4000.0, |available| {
            (available - 2.0 * options.padding).max(80.0)
        })
}

pub fn cap_width(options: &RenderOptions, natural: f64, least: f64, around: f64) -> f64 {
    options
        .max_width
        .filter(|available| *available > 0.0)
        .map_or(natural, |available| {
            least.min(natural).max(natural.min(available - around))
        })
}

pub fn fit_width(
    options: &RenderOptions,
    natural: f64,
    least: f64,
    mut draw: impl FnMut(f64, bool) -> Result<RenderResult, PeleError>,
) -> Result<RenderResult, PeleError> {
    let mut result = draw(natural, false)?;
    let Some(available) = options.max_width.filter(|available| *available > 0.0) else {
        return Ok(result);
    };
    if result.width <= available {
        return Ok(result);
    }
    let floor = least.min(natural);
    let mut plot = natural;
    result = draw(plot, true)?;
    for _ in 0..3 {
        if result.width <= available || plot <= floor {
            break;
        }
        plot = floor.max(plot - (result.width - available));
        result = draw(plot, true)?;
    }
    Ok(result)
}

pub fn turn_to_fit(
    options: &RenderOptions,
    mut draw: impl FnMut(bool) -> Result<RenderResult, PeleError>,
) -> Result<RenderResult, PeleError> {
    let across = draw(false)?;
    let Some(available) = options.max_width.filter(|available| *available > 0.0) else {
        return Ok(across);
    };
    if !options.auto_direction || across.width <= available {
        return Ok(across);
    }
    let down = draw(true)?;
    let narrow = available < options.direction_breakpoint;
    Ok(
        if down.width < across.width && (narrow || down.width <= available) {
            down
        } else {
            across
        },
    )
}

pub fn runs_across(direction: &str) -> bool {
    matches!(direction, "LR" | "RL")
}

pub fn tighten(
    options: &RenderOptions,
    mut draw: impl FnMut(bool) -> Result<RenderResult, PeleError>,
) -> Result<RenderResult, PeleError> {
    let loose = draw(false)?;
    let Some(available) = options.max_width.filter(|available| *available > 0.0) else {
        return Ok(loose);
    };
    if loose.width <= available {
        return Ok(loose);
    }
    let tight = draw(true)?;
    Ok(if tight.width < loose.width {
        tight
    } else {
        loose
    })
}
