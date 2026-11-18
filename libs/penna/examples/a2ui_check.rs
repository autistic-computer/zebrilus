use std::io::Read as _;
use std::process::ExitCode;

use penna::blocks::a2ui::{Document, compile_express, parse_messages};

fn main() -> ExitCode {
    let mut text = String::new();
    if let Err(error) = std::io::stdin().read_to_string(&mut text) {
        println!("could not read the spec: {error}");
        return ExitCode::FAILURE;
    }
    let parsed = if text.trim_start().starts_with(['{', '[']) {
        parse_messages(&text)
    } else {
        compile_express(&text).map(|message| vec![message])
    };
    let messages = match parsed {
        Ok(messages) => messages,
        Err(error) => {
            println!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let mut document = Document::default();
    for message in messages {
        document.apply(message);
    }
    let problems = document.problems();
    if problems.is_empty() {
        println!("ok");
        return ExitCode::SUCCESS;
    }
    for problem in problems {
        println!("{problem}");
    }
    ExitCode::FAILURE
}
