mod common;

use std::fs;

use common::{common, run_cli, temp_git_repo};

#[test]
fn handle_user_prompt_submit() {
    let repo = temp_git_repo();
    let cwd = repo.path().to_str().unwrap();
    let transcript = tempfile::NamedTempFile::new().unwrap();
    fs::write(transcript.path(), "{\"type\":\"system\",\"uuid\":\"a\",\"subtype\":\"turn_duration\",\"isSidechain\":false,\"userType\":\"external\",\"cwd\":\"/tmp\",\"sessionId\":\"s\",\"timestamp\":\"t\",\"version\":\"v\",\"durationMs\":100,\"isMeta\":false}\n").unwrap();
    let common = common(cwd, transcript.path().to_str().unwrap());
    let input = format!(
        r#"{{ {common}, "hook_event_name": "UserPromptSubmit", "prompt": "hello world" }}"#
    );
    let (code, stdout, stderr) = run_cli(&input);
    assert_eq!(code, 0);
    assert!(stderr.is_empty(), "expected no stderr, got: {stderr}");
    let output: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(
        output["systemMessage"].as_str().unwrap().contains("tracking prompt"),
        "expected hint about tracking prompt, got: {stdout}"
    );
}

#[test]
fn handle_user_prompt_submit_blocks_on_uncommitted_changes() {
    let repo = temp_git_repo();
    let cwd = repo.path().to_str().unwrap();
    // Create an uncommitted file so has_uncommitted_changes returns true.
    fs::write(repo.path().join("dirty.txt"), "uncommitted content").unwrap();
    let common = common(cwd, "/tmp/t.jsonl");
    let input = format!(
        r#"{{ {common}, "hook_event_name": "UserPromptSubmit", "prompt": "hello world" }}"#
    );
    let (code, stdout, stderr) = run_cli(&input);
    assert_eq!(code, 0);
    assert!(stderr.is_empty(), "expected no stderr, got: {stderr}");
    let output: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        output["decision"].as_str(),
        Some("block"),
        "expected block decision, got: {stdout}"
    );
    assert!(
        output["reason"].as_str().unwrap().contains("uncommitted"),
        "expected reason about uncommitted changes, got: {stdout}"
    );
}
