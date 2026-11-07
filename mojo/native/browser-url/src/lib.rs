use std::{ptr, slice, str};

#[path = "../../../../crates/zz-client/src/url_input.rs"]
#[allow(dead_code)]
mod url_input;

const MAX_INPUT_BYTES: usize = 64 * 1024;

fn canonical(operation: u32, provider: u32, input: &str) -> Result<String, i64> {
    let result = match operation {
        0 => url_input::normalize_url(input),
        1 => {
            let provider = match provider {
                0 => url_input::SearchProvider::Google,
                1 => url_input::SearchProvider::DuckDuckGo,
                2 => url_input::SearchProvider::Brave,
                _ => return Err(-102),
            };
            url_input::resolve_address(input, provider)
        }
        2 => return Ok(url_input::diagnostic_url(input)),
        _ => return Err(-103),
    };
    result.map_err(|error| match error {
        url_input::UrlInputError::Empty => -1,
        url_input::UrlInputError::UnsupportedScheme => -2,
        url_input::UrlInputError::Invalid => -3,
    })
}

#[unsafe(no_mangle)]
unsafe extern "C" fn zz_browser_url(
    operation: u32,
    provider: u32,
    input: *const u8,
    input_len: usize,
    output: *mut u8,
    capacity: usize,
) -> i64 {
    if input_len > MAX_INPUT_BYTES
        || input.is_null() && input_len != 0
        || output.is_null() && capacity != 0
    {
        return -100;
    }
    let input = if input_len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(input, input_len) }
    };
    let Ok(input) = str::from_utf8(input) else {
        return -101;
    };
    let result = match canonical(operation, provider, input) {
        Ok(result) => result,
        Err(error) => return error,
    };
    let Ok(length) = i64::try_from(result.len()) else {
        return -100;
    };
    if capacity >= result.len() && !output.is_null() {
        unsafe { ptr::copy_nonoverlapping(result.as_ptr(), output, result.len()) };
    }
    length
}
