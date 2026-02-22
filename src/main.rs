mod decision;
mod metadata;
mod preferences;
mod session;
mod transcript;
mod types;

use anyhow::Result;
use session::Session;
use std::io::{self, Read};
use std::process;
use types::{HookInput, HookOutput};

fn read_stdin() -> Result<String> {
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    Ok(buffer)
}

fn main() {
    let input = read_stdin().expect("Failed to read stdin");
    let hook_input: HookInput =
        serde_json::from_str(&input).expect("Failed to parse hook input");

    let result: Result<Option<HookOutput>> = match &hook_input {
        HookInput::SessionStart(e) => Session::open(&e.common.cwd, &e.common.session_id)
            .and_then(|s| s.handle_session_start(e)),
        HookInput::UserPromptSubmit(e) => Session::open(&e.common.cwd, &e.common.session_id)
            .and_then(|s| s.handle_user_prompt_submit(e)),
        HookInput::Stop(e) => Session::open(&e.common.cwd, &e.common.session_id)
            .and_then(|s| s.handle_stop(e)),
        HookInput::SessionEnd(e) => Session::open(&e.common.cwd, &e.common.session_id)
            .and_then(|s| s.handle_session_end(e)),
        _ => Ok(None),
    };

    match result {
        Ok(Some(output)) => {
            println!(
                "{}",
                serde_json::to_string(&output).expect("Failed to serialize output")
            );
        }
        Ok(None) => {}
        Err(err) => {
            eprintln!("claudtributter: {err:#}");
            process::exit(2);
        }
    }
}
