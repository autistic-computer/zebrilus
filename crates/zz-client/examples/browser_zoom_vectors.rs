use std::{error::Error, fs};

#[path = "../../../libs/ala-cef/src/page_zoom.rs"]
mod page_zoom;

fn main() -> Result<(), Box<dyn Error>> {
    let mut factors = vec![
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -0.0,
        0.0,
        -1.0,
        1.0,
        5.0,
        0.25,
        655.35,
        f64::MAX,
        f64::MIN,
    ];
    for index in 0..=1000 {
        let factor = f64::from(index) / 200.0;
        factors.extend([factor, factor.next_up(), factor.next_down()]);
    }
    for factor in [
        0.25_f64, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0,
        5.0,
    ] {
        for boundary in [factor - 0.000_001, factor + 0.000_001] {
            factors.extend([boundary, boundary.next_up(), boundary.next_down()]);
        }
    }
    let mut output = Vec::new();
    output.extend_from_slice(&u32::try_from(factors.len())?.to_le_bytes());
    for factor in factors {
        output.extend_from_slice(&factor.to_bits().to_le_bytes());
        let sanitized = page_zoom::sanitized_page_zoom_factor(factor);
        output.extend_from_slice(&sanitized.to_bits().to_le_bytes());
        output.extend_from_slice(&page_zoom::page_zoom_percent(factor).to_le_bytes());
        output.extend_from_slice(
            &page_zoom::chromium_zoom_level(sanitized)
                .to_bits()
                .to_le_bytes(),
        );
        for direction in [-1, 0, 1] {
            output.extend_from_slice(
                &page_zoom::next_page_zoom_factor(factor, direction)
                    .to_bits()
                    .to_le_bytes(),
            );
        }
    }
    fs::write(
        std::env::args().nth(1).ok_or("missing output path")?,
        output,
    )?;
    Ok(())
}
