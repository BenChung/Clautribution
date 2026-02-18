//! End-to-end tests that run real Claude Code sessions against the built
//! claudtributter binary. These are disabled by default because they:
//!
//! - Require a valid `ANTHROPIC_API_KEY` (or active Claude Code auth)
//! - Make real API calls (costs money)
//! - Are non-deterministic (Claude's responses vary)
//! - Are slow (seconds per invocation)
//!
//! Run them with:
//!
//!     CLAUDE_E2E=1 cargo test --test e2e -- --ignored --nocapture
//!
//! You can also set `CLAUDE_E2E_MODEL` to control the model (default: haiku).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Skip the test unless `CLAUDE_E2E` is set.
fn require_e2e() {
    if std::env::var("CLAUDE_E2E").is_err() {
        eprintln!("skipping e2e test (set CLAUDE_E2E=1 to enable)");
        return;
    }
}

fn model() -> String {
    std::env::var("CLAUDE_E2E_MODEL").unwrap_or_else(|_| "haiku".into())
}

/// Create a temp directory with an initialized git repo, `.gitignore`, and
/// a hook settings file pointing at the built claudtributter binary.
struct TestRepo {
    dir: tempfile::TempDir,
    session_id: String,
}

impl TestRepo {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let repo = git2::Repository::init(dir.path()).expect("git init failed");

        // Configure git identity.
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "E2E Test").unwrap();
        config.set_str("user.email", "e2e@test.com").unwrap();

        // Ignore .claudetributer so the hook can create its state files.
        fs::write(dir.path().join(".gitignore"), ".claudetributer\n").unwrap();

        // Create a seed file and initial commit.
        fs::write(dir.path().join("README.md"), "# test repo\n").unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
            .unwrap();

        // Write a project settings file that registers claudtributter hooks.
        let binary = env!("CARGO_BIN_EXE_claudtributter");
        let claude_dir = dir.path().join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let settings = serde_json::json!({
            "hooks": {
                "SessionStart": [{
                    "matcher": "*",
                    "hooks": [{ "type": "command", "command": binary, "timeout": 30 }]
                }],
                "UserPromptSubmit": [{
                    "matcher": "*",
                    "hooks": [{ "type": "command", "command": binary, "timeout": 30 }]
                }],
                "Stop": [{
                    "matcher": "*",
                    "hooks": [{ "type": "command", "command": binary, "timeout": 30 }]
                }],
                "SessionEnd": [{
                    "matcher": "*",
                    "hooks": [{ "type": "command", "command": binary, "timeout": 30 }]
                }]
            },
            "permissions": {
                "allow": [
                    "Write",
                    "Read",
                    "Edit"
                ]
            }
        });
        fs::write(
            claude_dir.join("settings.local.json"),
            serde_json::to_string_pretty(&settings).unwrap(),
        )
        .unwrap();

        let session_id = uuid::Uuid::new_v4().to_string();

        Self { dir, session_id }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn data_dir(&self) -> PathBuf {
        self.dir.path().join(".claudetributer")
    }

    /// Base args shared by all claude invocations.
    fn base_args(&self) -> Vec<String> {
        vec![
            "--model".into(),
            model(),
            "--output-format".into(),
            "json".into(),
            "--allowed-tools".into(),
            "Write Read Edit".into(),
        ]
    }

    /// Run `claude -p <prompt>` in the test repo. Returns (exit_code, stdout, stderr).
    fn run_claude(&self, prompt: &str) -> (i32, String, String) {
        let mut args = vec!["-p".into(), prompt.into()];
        args.extend(self.base_args());
        args.extend(["--session-id".into(), self.session_id.clone()]);

        let output = Command::new("claude")
            .args(&args)
            .current_dir(self.dir.path())
            .env_remove("CLAUDECODE")
            .output()
            .expect("failed to run claude");

        (
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        )
    }

    /// Run `claude -p <prompt> --resume <session_id>` to continue the session.
    fn resume_claude(&self, prompt: &str) -> (i32, String, String) {
        let mut args = vec!["-p".into(), prompt.into()];
        args.extend(self.base_args());
        args.extend(["--resume".into(), self.session_id.clone()]);

        let output = Command::new("claude")
            .args(&args)
            .current_dir(self.dir.path())
            .env_remove("CLAUDECODE")
            .output()
            .expect("failed to run claude");

        (
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        )
    }

    /// Read a git note from a specific ref on the latest HEAD commit.
    fn read_note(&self, ref_name: &str) -> Option<String> {
        let repo = git2::Repository::open(self.path()).unwrap();
        let head_oid = repo.head().ok()?.peel_to_commit().ok()?.id();
        repo.find_note(Some(ref_name), head_oid)
            .ok()
            .and_then(|note| note.message().map(|s| s.trim().to_string()))
    }

    /// Count commits on the current branch.
    fn commit_count(&self) -> usize {
        let repo = git2::Repository::open(self.path()).unwrap();
        let mut revwalk = repo.revwalk().unwrap();
        revwalk.push_head().unwrap();
        revwalk.count()
    }
}

// =================================================================
// Tests — all gated behind CLAUDE_E2E=1
// =================================================================

/// Basic productive stop: Claude creates a file, claudtributter commits it
/// and attaches notes.
#[test]
#[ignore]
fn productive_stop_creates_commit_with_notes() {
    require_e2e();

    let repo = TestRepo::new();
    let (code, stdout, stderr) = repo.run_claude(
        "Create a file called hello.txt containing the text 'hello world'. \
         Do not create any other files. Do not explain, just create the file.",
    );

    eprintln!("exit={code}\nstdout={stdout}\nstderr={stderr}");
    assert_eq!(code, 0, "claude exited with code {code}\nstderr: {stderr}");

    // claudtributter should have committed the file.
    assert!(
        repo.commit_count() > 1,
        "expected more than the initial commit, got {}",
        repo.commit_count()
    );

    // The committed file should exist.
    assert!(
        repo.path().join("hello.txt").exists(),
        "hello.txt should exist"
    );

    // Git notes should be attached.
    assert!(
        repo.read_note("refs/notes/prompt").is_some(),
        "prompt note missing"
    );
    assert!(
        repo.read_note("refs/notes/transcript").is_some(),
        "transcript note missing"
    );
    assert!(
        repo.read_note("refs/notes/session").is_some(),
        "session note missing"
    );
    assert!(
        repo.read_note("refs/notes/tail").is_some(),
        "tail note missing"
    );

    // Breadcrumb should be cleaned up after productive stop.
    let crumb = repo
        .data_dir()
        .join(format!("continuation-{}.json", repo.session_id));
    assert!(
        !crumb.exists(),
        "breadcrumb should be cleared after productive stop"
    );
}

/// Nonproductive stop: Claude answers a question without creating files.
/// No commit should be created, but a breadcrumb should be written.
#[test]
#[ignore]
fn nonproductive_stop_writes_breadcrumb_only() {
    require_e2e();

    let repo = TestRepo::new();
    let initial_count = repo.commit_count();

    let (code, stdout, stderr) = repo.run_claude(
        "What is 2 + 2? Answer briefly, do not create or modify any files.",
    );

    eprintln!("exit={code}\nstdout={stdout}\nstderr={stderr}");
    assert_eq!(code, 0, "claude exited with code {code}\nstderr: {stderr}");

    // No new commits — only the initial commit.
    assert_eq!(
        repo.commit_count(),
        initial_count,
        "no new commit expected for nonproductive stop"
    );

    // Breadcrumb should exist.
    let crumb = repo
        .data_dir()
        .join(format!("continuation-{}.json", repo.session_id));
    assert!(
        crumb.exists(),
        "breadcrumb should be written for nonproductive stop"
    );
}

/// Multi-round: nonproductive stop followed by productive stop.
/// The productive commit's transcript note should span both rounds.
#[test]
#[ignore]
fn multi_round_nonproductive_then_productive() {
    require_e2e();

    let repo = TestRepo::new();

    // Round 1: nonproductive (just a question).
    let (code, _stdout, stderr) = repo.run_claude(
        "What is the capital of France? Answer briefly, do not create or modify any files.",
    );
    assert_eq!(code, 0, "round 1 failed: {stderr}");

    let crumb = repo
        .data_dir()
        .join(format!("continuation-{}.json", repo.session_id));
    assert!(
        crumb.exists(),
        "breadcrumb should exist after nonproductive round"
    );

    // Round 2: productive (creates a file), resuming the same session.
    let (code, _stdout, stderr) = repo.resume_claude(
        "Create a file called result.txt containing 'Paris'. \
         Do not create any other files. Do not explain.",
    );
    assert_eq!(code, 0, "round 2 failed: {stderr}");

    // Should have a new commit with notes.
    assert!(repo.commit_count() > 1, "expected a new commit");
    assert!(
        repo.read_note("refs/notes/transcript").is_some(),
        "transcript note missing"
    );

    // The transcript note should contain entries from both rounds.
    let transcript_json = repo.read_note("refs/notes/transcript").unwrap();
    let entries: Vec<serde_json::Value> = serde_json::from_str(&transcript_json).unwrap();
    assert!(
        entries.len() >= 4,
        "transcript should span both rounds (at least 4 entries: 2 user + 2 assistant), got {}",
        entries.len()
    );

    // Breadcrumb should be cleared after productive stop.
    assert!(
        !crumb.exists(),
        "breadcrumb should be cleared after productive stop"
    );
}
