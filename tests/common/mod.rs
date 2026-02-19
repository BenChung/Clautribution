use std::io::Write;
use std::process::{Command, Stdio};

pub fn run_cli(stdin_json: &str) -> (i32, String, String) {
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
pub fn temp_git_repo() -> tempfile::TempDir {
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

pub fn common(cwd: &str, transcript_path: &str) -> String {
    format!(
        r#"
    "session_id": "test-session",
    "transcript_path": "{transcript_path}",
    "cwd": "{cwd}",
    "permission_mode": "default"
"#
    )
}

/// Helper: read a plain-text git note from a specific ref on HEAD.
pub fn read_note(repo_path: &std::path::Path, ref_name: &str) -> Option<String> {
    let repo = git2::Repository::open(repo_path).unwrap();
    let head_oid = repo.head().unwrap().peel_to_commit().unwrap().id();
    repo.find_note(Some(ref_name), head_oid)
        .ok()
        .and_then(|note| note.message().map(|s| s.trim().to_string()))
}

/// Common fields pointing at a non-git /tmp dir (for handlers that don't call data_dir).
pub const COMMON_NO_GIT: &str = r#"
    "session_id": "test-session",
    "transcript_path": "/tmp/t.jsonl",
    "cwd": "/tmp",
    "permission_mode": "default"
"#;
