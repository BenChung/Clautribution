mod common;

use std::fs;

use common::{common, read_note, run_cli, temp_git_repo};

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
