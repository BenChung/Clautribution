use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

fn run_cli(stdin_json: &str) -> (i32, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_claudtributter"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn binary");

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(stdin_json.as_bytes())
        .unwrap();

    let output = child.wait_with_output().unwrap();
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

/// Create a temp dir containing a git repo with an initial commit and return it.
/// The `TempDir` must be kept alive for the duration of the test.
fn temp_git_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();

    // Configure user identity for commits.
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Test").unwrap();
    config.set_str("user.email", "test@test.com").unwrap();

    // Create an initial commit so HEAD exists.
    let sig = repo.signature().unwrap();
    let tree_oid = repo.index().unwrap().write_tree().unwrap();
    let tree = repo.find_tree(tree_oid).unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
        .unwrap();

    dir
}

fn common(cwd: &str, transcript_path: &str) -> String {
    format!(
        r#"
    "session_id": "test-session",
    "transcript_path": "{transcript_path}",
    "cwd": "{cwd}",
    "permission_mode": "default"
"#
    )
}

/// Common fields pointing at a non-git /tmp dir (for handlers that don't call data_dir).
const COMMON_NO_GIT: &str = r#"
    "session_id": "test-session",
    "transcript_path": "/tmp/t.jsonl",
    "cwd": "/tmp",
    "permission_mode": "default"
"#;

#[test]
fn handle_session_start() {
    let repo = temp_git_repo();
    let cwd = repo.path().to_str().unwrap();
    let common = common(cwd, "/tmp/t.jsonl");
    let input = format!(
        r#"{{ {common},
            "hook_event_name": "SessionStart",
            "source": "startup",
            "model": "claude-sonnet-4-5-20250929"
        }}"#
    );
    let (code, stdout, stderr) = run_cli(&input);
    assert_eq!(code, 0);
    assert!(stderr.is_empty(), "expected no stderr, got: {stderr}");
    // Test repo is on master, so we expect a branch warning.
    let output: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(
        output["systemMessage"].as_str().unwrap().contains("feature branch"),
        "expected branch warning, got: {stdout}"
    );
    assert!(repo.path().join(".claudetributer").is_dir());
}

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

/// Helper: read a plain-text git note from a specific ref on HEAD.
fn read_note(repo_path: &std::path::Path, ref_name: &str) -> Option<String> {
    let repo = git2::Repository::open(repo_path).unwrap();
    let head_oid = repo.head().unwrap().peel_to_commit().unwrap().id();
    repo.find_note(Some(ref_name), head_oid)
        .ok()
        .and_then(|note| note.message().map(|s| s.trim().to_string()))
}

#[test]
fn handle_stop() {
    let repo = temp_git_repo();
    let cwd = repo.path().to_str().unwrap();
    // Build a small transcript with a user→assistant chain.
    let transcript = tempfile::NamedTempFile::new().unwrap();
    fs::write(transcript.path(), concat!(
        r#"{"type":"user","uuid":"u1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","message":{"role":"user","content":"hello"}}"#, "\n",
        r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","requestId":"r1","message":{"role":"assistant","content":[{"type":"text","text":"hi"}]}}"#, "\n",
    )).unwrap();
    // Write prompt.json so handle_stop finds it.
    let data_dir = repo.path().join(".claudetributer");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(
        data_dir.join("prompt-test-session.json"),
        r#"{"prompt":"hello","session_id":"s","uuid":"u1"}"#,
    ).unwrap();
    // Create a file change so this is a productive stop (has uncommitted changes).
    fs::write(repo.path().join("output.txt"), "result").unwrap();

    let common = common(cwd, transcript.path().to_str().unwrap());
    let input = format!(
        r#"{{ {common}, "hook_event_name": "Stop", "stop_hook_active": false }}"#
    );
    let (code, stdout, stderr) = run_cli(&input);
    assert_eq!(code, 0);
    assert!(stderr.is_empty(), "expected no stderr, got: {stderr}");
    let output: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let msg = output["systemMessage"].as_str().unwrap();
    assert!(msg.contains("notes"), "expected hint about notes, got: {msg}");

    // Verify per-category notes were written.
    let transcript_note = read_note(repo.path(), "refs/notes/transcript");
    assert!(transcript_note.is_some(), "expected transcript note");
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&transcript_note.unwrap()).unwrap();
    // With no prior committed tail, the transcript note walks the full chain: u1 + a1 = 2 entries.
    assert_eq!(parsed.len(), 2, "expected 2 transcript entries (full span since no prior commit tail)");

    let prompt_note = read_note(repo.path(), "refs/notes/prompt");
    assert_eq!(prompt_note.as_deref(), Some("hello"));

    let session_note = read_note(repo.path(), "refs/notes/session");
    assert_eq!(session_note.as_deref(), Some("s"));

    let tail_note = read_note(repo.path(), "refs/notes/tail");
    assert_eq!(tail_note.as_deref(), Some("a1"));

    // continuation.json must be cleared after a productive stop.
    assert!(!data_dir.join("continuation-test-session.json").exists(), "breadcrumb should be cleared after productive stop");
}

#[test]
fn handle_stop_detects_reset() {
    let repo = temp_git_repo();
    let cwd = repo.path().to_str().unwrap();

    // --- Turn 1: normal conversation u1→a1 (productive: creates a file) ---
    let transcript = tempfile::NamedTempFile::new().unwrap();
    fs::write(transcript.path(), concat!(
        r#"{"type":"user","uuid":"u1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","message":{"role":"user","content":"hello"}}"#, "\n",
        r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","requestId":"r1","message":{"role":"assistant","content":[{"type":"text","text":"hi"}]}}"#, "\n",
    )).unwrap();
    let data_dir = repo.path().join(".claudetributer");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(
        data_dir.join("prompt-test-session.json"),
        r#"{"prompt":"hello","session_id":"s","uuid":"u1"}"#,
    ).unwrap();
    // File change makes this a productive stop so refs/notes/tail gets written.
    fs::write(repo.path().join("turn1.txt"), "turn 1").unwrap();
    let common_str = common(cwd, transcript.path().to_str().unwrap());
    let input = format!(
        r#"{{ {common_str}, "hook_event_name": "Stop", "stop_hook_active": false }}"#
    );
    let (code, stdout, stderr) = run_cli(&input);
    assert_eq!(code, 0);
    assert!(stderr.is_empty(), "turn 1 stderr: {stderr}");
    let output: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let msg = output["systemMessage"].as_str().unwrap();
    assert!(!msg.contains("reset"), "turn 1 should not detect reset, got: {msg}");

    // Verify tail is "a1" after turn 1.
    assert_eq!(read_note(repo.path(), "refs/notes/tail").as_deref(), Some("a1"));

    // --- Turn 2: reset — new conversation branches from u1, not continuing from a1 ---
    // The transcript now has the old chain PLUS a new branch: u1→a1 and u1→a2
    // (user reset back to u1 and got a different assistant response).
    fs::write(transcript.path(), concat!(
        r#"{"type":"user","uuid":"u1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","message":{"role":"user","content":"hello"}}"#, "\n",
        r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","requestId":"r1","message":{"role":"assistant","content":[{"type":"text","text":"hi"}]}}"#, "\n",
        r#"{"type":"user","uuid":"u2","parentUuid":"a1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","message":{"role":"user","content":"do something"}}"#, "\n",
        r#"{"type":"assistant","uuid":"a2","parentUuid":"u2","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","requestId":"r2","message":{"role":"assistant","content":[{"type":"text","text":"ok"}]}}"#, "\n",
        r#"{"type":"user","uuid":"u3","parentUuid":"u1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","message":{"role":"user","content":"try again"}}"#, "\n",
        r#"{"type":"assistant","uuid":"a3","parentUuid":"u3","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","requestId":"r3","message":{"role":"assistant","content":[{"type":"text","text":"retrying"}]}}"#, "\n",
    )).unwrap();
    // New prompt metadata for the reset turn.
    fs::write(
        data_dir.join("prompt-test-session.json"),
        r#"{"prompt":"try again","session_id":"s","uuid":"u3"}"#,
    ).unwrap();
    let common_str = common(cwd, transcript.path().to_str().unwrap());
    let input = format!(
        r#"{{ {common_str}, "hook_event_name": "Stop", "stop_hook_active": false }}"#
    );
    let (code, stdout, stderr) = run_cli(&input);
    assert_eq!(code, 0);
    assert!(stderr.is_empty(), "turn 2 stderr: {stderr}");
    let output: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let msg = output["systemMessage"].as_str().unwrap();
    assert!(msg.contains("reset detected"), "turn 2 should detect reset, got: {msg}");
}

#[test]
fn handle_stop_normal_continuation_no_false_reset() {
    let repo = temp_git_repo();
    let cwd = repo.path().to_str().unwrap();

    // --- Turn 1: u1→a1 ---
    let transcript = tempfile::NamedTempFile::new().unwrap();
    fs::write(transcript.path(), concat!(
        r#"{"type":"user","uuid":"u1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","message":{"role":"user","content":"hello"}}"#, "\n",
        r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","requestId":"r1","message":{"role":"assistant","content":[{"type":"text","text":"hi"}]}}"#, "\n",
    )).unwrap();
    let data_dir = repo.path().join(".claudetributer");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(
        data_dir.join("prompt-test-session.json"),
        r#"{"prompt":"hello","session_id":"s","uuid":"u1"}"#,
    ).unwrap();
    let common_str = common(cwd, transcript.path().to_str().unwrap());
    let input = format!(
        r#"{{ {common_str}, "hook_event_name": "Stop", "stop_hook_active": false }}"#
    );
    let (code, _, stderr) = run_cli(&input);
    assert_eq!(code, 0);
    assert!(stderr.is_empty(), "turn 1 stderr: {stderr}");

    // --- Turn 2: normal continuation u1→a1→u2→a2 ---
    fs::write(transcript.path(), concat!(
        r#"{"type":"user","uuid":"u1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","message":{"role":"user","content":"hello"}}"#, "\n",
        r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","requestId":"r1","message":{"role":"assistant","content":[{"type":"text","text":"hi"}]}}"#, "\n",
        r#"{"type":"user","uuid":"u2","parentUuid":"a1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","message":{"role":"user","content":"do more"}}"#, "\n",
        r#"{"type":"assistant","uuid":"a2","parentUuid":"u2","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","requestId":"r2","message":{"role":"assistant","content":[{"type":"text","text":"done"}]}}"#, "\n",
    )).unwrap();
    fs::write(
        data_dir.join("prompt-test-session.json"),
        r#"{"prompt":"do more","session_id":"s","uuid":"u2"}"#,
    ).unwrap();
    let common_str = common(cwd, transcript.path().to_str().unwrap());
    let input = format!(
        r#"{{ {common_str}, "hook_event_name": "Stop", "stop_hook_active": false }}"#
    );
    let (code, stdout, stderr) = run_cli(&input);
    assert_eq!(code, 0);
    assert!(stderr.is_empty(), "turn 2 stderr: {stderr}");
    let output: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let msg = output["systemMessage"].as_str().unwrap();
    assert!(!msg.contains("reset"), "normal continuation should NOT detect reset, got: {msg}");
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

#[test]
fn unhandled_event_passes_through() {
    let input = format!(
        r#"{{ {COMMON_NO_GIT},
            "hook_event_name": "PostToolUseFailure",
            "tool_name": "Bash",
            "tool_input": {{ "command": "false" }},
            "tool_use_id": "toolu_003",
            "error": "exit code 1",
            "is_interrupt": false
        }}"#
    );
    let (code, stdout, stderr) = run_cli(&input);
    assert_eq!(code, 0);
    assert!(stdout.is_empty());
    assert!(stderr.is_empty());
}

// =================================================================
// Breadcrumb / continuation tests
// =================================================================

#[test]
fn nonproductive_stop_writes_breadcrumb_no_notes() {
    let repo = temp_git_repo();
    let cwd = repo.path().to_str().unwrap();
    let transcript = tempfile::NamedTempFile::new().unwrap();
    fs::write(transcript.path(), concat!(
        r#"{"type":"user","uuid":"u1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","message":{"role":"user","content":"hello"}}"#, "\n",
        r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","requestId":"r1","message":{"role":"assistant","content":[{"type":"text","text":"hi"}]}}"#, "\n",
    )).unwrap();
    let data_dir = repo.path().join(".claudetributer");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(
        data_dir.join("prompt-test-session.json"),
        r#"{"prompt":"hello","session_id":"s","uuid":"u1"}"#,
    ).unwrap();
    // No file changes → nonproductive stop.
    let common = common(cwd, transcript.path().to_str().unwrap());
    let input = format!(
        r#"{{ {common}, "hook_event_name": "Stop", "stop_hook_active": false }}"#
    );
    let (code, stdout, stderr) = run_cli(&input);
    assert_eq!(code, 0);
    assert!(stderr.is_empty(), "expected no stderr, got: {stderr}");
    let output: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let msg = output["systemMessage"].as_str().unwrap();
    assert!(msg.contains("nonproductive"), "expected nonproductive hint, got: {msg}");

    // No notes written on HEAD (still initial commit).
    assert!(read_note(repo.path(), "refs/notes/transcript").is_none(), "no transcript note expected");
    assert!(read_note(repo.path(), "refs/notes/tail").is_none(), "no tail note expected");

    // Breadcrumb written.
    let crumb_path = data_dir.join("continuation-test-session.json");
    assert!(crumb_path.exists(), "breadcrumb should be written");
    let crumb: serde_json::Value = serde_json::from_str(&fs::read_to_string(&crumb_path).unwrap()).unwrap();
    assert_eq!(crumb["tail_uuid"].as_str(), Some("a1"));
    assert_eq!(crumb["session_id"].as_str(), Some("s"));
}

#[test]
fn productive_stop_flushes_breadcrumb_with_expanded_transcript() {
    let repo = temp_git_repo();
    let cwd = repo.path().to_str().unwrap();
    let data_dir = repo.path().join(".claudetributer");
    fs::create_dir_all(&data_dir).unwrap();

    // --- Nonproductive turn: u1→a1, no file changes ---
    let transcript = tempfile::NamedTempFile::new().unwrap();
    fs::write(transcript.path(), concat!(
        r#"{"type":"user","uuid":"u1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","message":{"role":"user","content":"hello"}}"#, "\n",
        r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","requestId":"r1","message":{"role":"assistant","content":[{"type":"text","text":"hi"}]}}"#, "\n",
    )).unwrap();
    fs::write(
        data_dir.join("prompt-test-session.json"),
        r#"{"prompt":"hello","session_id":"s","uuid":"u1"}"#,
    ).unwrap();
    let common_str = common(cwd, transcript.path().to_str().unwrap());
    let input = format!(r#"{{ {common_str}, "hook_event_name": "Stop", "stop_hook_active": false }}"#);
    let (code, _, stderr) = run_cli(&input);
    assert_eq!(code, 0);
    assert!(stderr.is_empty(), "turn 1 stderr: {stderr}");
    // Breadcrumb should now exist with tail "a1".
    let crumb: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(data_dir.join("continuation-test-session.json")).unwrap()
    ).unwrap();
    assert_eq!(crumb["tail_uuid"].as_str(), Some("a1"));

    // --- Productive turn: extends to u2→a2, creates a file ---
    fs::write(transcript.path(), concat!(
        r#"{"type":"user","uuid":"u1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","message":{"role":"user","content":"hello"}}"#, "\n",
        r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","requestId":"r1","message":{"role":"assistant","content":[{"type":"text","text":"hi"}]}}"#, "\n",
        r#"{"type":"user","uuid":"u2","parentUuid":"a1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","message":{"role":"user","content":"do more"}}"#, "\n",
        r#"{"type":"assistant","uuid":"a2","parentUuid":"u2","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","requestId":"r2","message":{"role":"assistant","content":[{"type":"text","text":"done"}]}}"#, "\n",
    )).unwrap();
    fs::write(
        data_dir.join("prompt-test-session.json"),
        r#"{"prompt":"do more","session_id":"s","uuid":"u2"}"#,
    ).unwrap();
    // Uncommitted file change → productive stop.
    fs::write(repo.path().join("output.txt"), "done").unwrap();
    let common_str = common(cwd, transcript.path().to_str().unwrap());
    let input = format!(r#"{{ {common_str}, "hook_event_name": "Stop", "stop_hook_active": false }}"#);
    let (code, stdout, stderr) = run_cli(&input);
    assert_eq!(code, 0);
    assert!(stderr.is_empty(), "turn 2 stderr: {stderr}");
    let out: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let msg = out["systemMessage"].as_str().unwrap();
    assert!(msg.contains("notes"), "expected notes hint, got: {msg}");

    // Transcript note should span BOTH turns (a1 + a2 = 2 entries, since
    // committed_tail is None → walk to root, stopping before the stop-point which is None).
    // With no prior committed tail: turn_raw("a2", None) → [u1, a1, u2, a2] = 4 entries.
    let transcript_note = read_note(repo.path(), "refs/notes/transcript").unwrap();
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&transcript_note).unwrap();
    assert!(parsed.len() >= 2, "expanded transcript should span both turns, got {} entries", parsed.len());
    let uuids: Vec<&str> = parsed.iter().filter_map(|v| v["uuid"].as_str()).collect();
    assert!(uuids.contains(&"a1"), "a1 should be in expanded transcript: {:?}", uuids);
    assert!(uuids.contains(&"a2"), "a2 should be in expanded transcript: {:?}", uuids);

    // Breadcrumb must be cleared.
    assert!(!data_dir.join("continuation-test-session.json").exists(), "breadcrumb should be cleared after productive stop");

    // Tail note updated to "a2".
    assert_eq!(read_note(repo.path(), "refs/notes/tail").as_deref(), Some("a2"));
}

#[test]
fn reset_detected_via_breadcrumb() {
    let repo = temp_git_repo();
    let cwd = repo.path().to_str().unwrap();
    let data_dir = repo.path().join(".claudetributer");
    fs::create_dir_all(&data_dir).unwrap();

    // --- Nonproductive turn: u1→a1 ---
    let transcript = tempfile::NamedTempFile::new().unwrap();
    fs::write(transcript.path(), concat!(
        r#"{"type":"user","uuid":"u1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","message":{"role":"user","content":"hello"}}"#, "\n",
        r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","requestId":"r1","message":{"role":"assistant","content":[{"type":"text","text":"hi"}]}}"#, "\n",
    )).unwrap();
    fs::write(
        data_dir.join("prompt-test-session.json"),
        r#"{"prompt":"hello","session_id":"s","uuid":"u1"}"#,
    ).unwrap();
    let common_str = common(cwd, transcript.path().to_str().unwrap());
    let input = format!(r#"{{ {common_str}, "hook_event_name": "Stop", "stop_hook_active": false }}"#);
    let (code, _, stderr) = run_cli(&input);
    assert_eq!(code, 0);
    assert!(stderr.is_empty(), "turn 1 stderr: {stderr}");
    // Breadcrumb has tail "a1" from nonproductive stop.
    assert!(data_dir.join("continuation-test-session.json").exists());

    // --- Reset: transcript branches from u1 to a branch that doesn't include a1 ---
    // New branch: u1→u3→a3 (u3's parentUuid is u1, bypassing a1)
    fs::write(transcript.path(), concat!(
        r#"{"type":"user","uuid":"u1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","message":{"role":"user","content":"hello"}}"#, "\n",
        r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","requestId":"r1","message":{"role":"assistant","content":[{"type":"text","text":"hi"}]}}"#, "\n",
        r#"{"type":"user","uuid":"u3","parentUuid":"u1","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","message":{"role":"user","content":"try again"}}"#, "\n",
        r#"{"type":"assistant","uuid":"a3","parentUuid":"u3","isSidechain":false,"userType":"external","cwd":"/tmp","sessionId":"s","timestamp":"t","version":"v","requestId":"r3","message":{"role":"assistant","content":[{"type":"text","text":"retrying"}]}}"#, "\n",
    )).unwrap();
    fs::write(
        data_dir.join("prompt-test-session.json"),
        r#"{"prompt":"try again","session_id":"s","uuid":"u3"}"#,
    ).unwrap();
    let common_str = common(cwd, transcript.path().to_str().unwrap());
    let input = format!(r#"{{ {common_str}, "hook_event_name": "Stop", "stop_hook_active": false }}"#);
    let (code, stdout, stderr) = run_cli(&input);
    assert_eq!(code, 0);
    assert!(stderr.is_empty(), "reset turn stderr: {stderr}");
    let out: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let msg = out["systemMessage"].as_str().unwrap();
    assert!(msg.contains("reset detected"), "expected reset detected via breadcrumb, got: {msg}");
}

// =================================================================
// Error handling
// =================================================================

#[test]
fn rejects_invalid_json() {
    let (code, _, _) = run_cli("not json");
    assert_ne!(code, 0);
}

#[test]
fn rejects_unknown_event() {
    let input = format!(
        r#"{{ {COMMON_NO_GIT}, "hook_event_name": "BogusEvent" }}"#
    );
    let (code, _, _) = run_cli(&input);
    assert_ne!(code, 0);
}
