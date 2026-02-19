mod common;

use common::{run_cli, COMMON_NO_GIT};

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
