mod common;

use common::{common, run_cli, temp_git_repo};

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
