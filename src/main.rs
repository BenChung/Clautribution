mod decision;
mod metadata;
mod preferences;
mod session;
mod transcript;
mod types;

use anyhow::{Context, Result};
use decision::{decide_stop, StopDecision};
use session::Session;
use std::io::{self, Read};
use std::process;
use types::{HookInput, HookOutput};

fn read_stdin() -> Result<String> {
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    Ok(buffer)
}

/// Open a session by discovering the active session ID.  Tries prompt
/// metadata files first, then falls back to the most recent transcript.
fn open_active_session(cwd: &str) -> Result<(Session, String)> {
    let probe = Session::open(cwd, "")?;
    // Try prompt metadata files first (most precise).
    if let Some(sid) = probe.active_session_id()? {
        let session = Session::open(cwd, &sid)?;
        if let Some((_, transcript_path)) = session.active_transcript()? {
            return Ok((session, transcript_path));
        }
    }
    // Fall back to most recent transcript file.
    let (session_id, transcript_path) = probe
        .active_transcript()?
        .context("no active session (no transcript found)")?;
    let session = Session::open(cwd, &session_id)?;
    Ok((session, transcript_path))
}

fn run_preview(cwd: &str) -> Result<()> {
    let (session, transcript_path) = open_active_session(cwd)?;
    let mut owned = session.build_stop_context(&transcript_path)?;
    // Force the productive path so we always render a commit message,
    // even when there are no uncommitted changes yet.
    owned.has_uncommitted_changes = true;
    let ctx = owned.as_ref();
    let decision = decide_stop(&ctx).map_err(|e| anyhow::anyhow!("{e}"))?;
    match decision {
        StopDecision::NoMetadata => {
            println!("No prompt metadata — nothing to preview.");
        }
        StopDecision::NoTail => {
            println!("No transcript tail — nothing to preview.");
        }
        StopDecision::Productive { commit_message, .. } => {
            println!("{commit_message}");
        }
        StopDecision::Nonproductive { .. } => {
            // Shouldn't happen with has_uncommitted_changes forced true.
            println!("No preview available.");
        }
    }
    Ok(())
}

fn run_drop(cwd: &str) -> Result<()> {
    let (session, transcript_path) = open_active_session(cwd)?;
    let transcript = session::read_transcript(&transcript_path)?;
    if let Some(tail) = transcript.conversation_tail() {
        session.write_drop_marker(tail)?;
    }
    session.drop_accumulated()?;
    println!("Accumulated state dropped. Future commits will start from this point.");
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Subcommand dispatch: `claudtributter preview <cwd>`
    //                      `claudtributter drop <cwd>`
    if args.len() >= 2 {
        let result = match args[1].as_str() {
            "preview" => {
                if args.len() < 3 {
                    eprintln!("usage: claudtributter preview <cwd>");
                    process::exit(1);
                }
                run_preview(&args[2])
            }
            "drop" => {
                if args.len() < 3 {
                    eprintln!("usage: claudtributter drop <cwd>");
                    process::exit(1);
                }
                run_drop(&args[2])
            }
            _ => {
                // Not a recognized subcommand — fall through to hook path.
                run_hook()
            }
        };
        match result {
            Ok(()) => {}
            Err(err) => {
                eprintln!("claudtributter: {err:#}");
                process::exit(2);
            }
        }
        return;
    }

    // No args: hook path (reads JSON from stdin).
    match run_hook() {
        Ok(()) => {}
        Err(err) => {
            eprintln!("claudtributter: {err:#}");
            process::exit(2);
        }
    }
}

fn run_hook() -> Result<()> {
    let input = read_stdin()?;
    let hook_input: HookInput = serde_json::from_str(&input)?;

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
        Err(err) => return Err(err),
    }
    Ok(())
}
