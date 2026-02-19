use super::*;
use serde_json::json;

// Helper to build the common fields every hook input needs.
fn common_fields() -> serde_json::Value {
    json!({
        "session_id": "sess-1",
        "transcript_path": "/tmp/transcript.jsonl",
        "cwd": "/home/user/project",
        "permission_mode": "default"
    })
}

fn merge(base: serde_json::Value, extra: serde_json::Value) -> serde_json::Value {
    let mut map = base.as_object().unwrap().clone();
    map.extend(extra.as_object().unwrap().clone());
    serde_json::Value::Object(map)
}

// =================================================================
// UserPromptSubmit (prompt hook) input deserialization
// =================================================================

#[test]
fn deserialize_user_prompt_submit() {
    let input = merge(
        common_fields(),
        json!({
            "hook_event_name": "UserPromptSubmit",
            "prompt": "Write a factorial function"
        }),
    );

    let hook: HookInput = serde_json::from_value(input).unwrap();
    match &hook {
        HookInput::UserPromptSubmit(e) => {
            assert_eq!(e.common.session_id, "sess-1");
            assert_eq!(e.common.permission_mode, Some(PermissionMode::Default));
            assert_eq!(e.prompt, "Write a factorial function");
        }
        other => panic!("Expected UserPromptSubmit, got {:?}", other),
    }
}

#[test]
fn deserialize_user_prompt_submit_all_permission_modes() {
    for (mode_str, expected) in [
        ("default", PermissionMode::Default),
        ("plan", PermissionMode::Plan),
        ("acceptEdits", PermissionMode::AcceptEdits),
        ("dontAsk", PermissionMode::DontAsk),
        ("bypassPermissions", PermissionMode::BypassPermissions),
    ] {
        let mut input = common_fields();
        input["permission_mode"] = json!(mode_str);
        let input = merge(
            input,
            json!({
                "hook_event_name": "UserPromptSubmit",
                "prompt": "test"
            }),
        );
        let hook: HookInput = serde_json::from_value(input).unwrap();
        assert_eq!(hook.common().permission_mode, Some(expected));
    }
}

// =================================================================
// PostToolUse input deserialization
// =================================================================

#[test]
fn deserialize_post_tool_use_write() {
    let input = merge(
        common_fields(),
        json!({
            "hook_event_name": "PostToolUse",
            "tool_name": "Write",
            "tool_input": {
                "file_path": "/tmp/out.txt",
                "content": "hello world"
            },
            "tool_response": {
                "filePath": "/tmp/out.txt",
                "success": true
            },
            "tool_use_id": "toolu_abc"
        }),
    );

    let hook: HookInput = serde_json::from_value(input).unwrap();
    match &hook {
        HookInput::PostToolUse(e) => {
            assert_eq!(e.tool_name, "Write");
            assert_eq!(e.tool_use_id, "toolu_abc");

            let ti: WriteToolInput = serde_json::from_value(e.tool_input.clone()).unwrap();
            assert_eq!(ti.file_path, "/tmp/out.txt");
            assert_eq!(ti.content, "hello world");

            assert_eq!(e.tool_response["success"], true);
        }
        other => panic!("Expected PostToolUse, got {:?}", other),
    }
}

#[test]
fn deserialize_post_tool_use_bash() {
    let input = merge(
        common_fields(),
        json!({
            "hook_event_name": "PostToolUse",
            "tool_name": "Bash",
            "tool_input": {
                "command": "npm test",
                "description": "Run tests",
                "timeout": 30000
            },
            "tool_response": { "stdout": "OK" },
            "tool_use_id": "toolu_xyz"
        }),
    );

    let hook: HookInput = serde_json::from_value(input).unwrap();
    if let HookInput::PostToolUse(e) = &hook {
        let bash: BashToolInput = serde_json::from_value(e.tool_input.clone()).unwrap();
        assert_eq!(bash.command, "npm test");
        assert_eq!(bash.description.as_deref(), Some("Run tests"));
        assert_eq!(bash.timeout, Some(30000));
        assert_eq!(bash.run_in_background, None);
    } else {
        panic!("wrong variant");
    }
}

// =================================================================
// ToolCall::parse and PreToolUseInput::tool_call
// =================================================================

#[test]
fn tool_call_parse_bash() {
    let tc = ToolCall::parse("Bash", &json!({"command": "npm test", "timeout": 5000})).unwrap();
    match tc {
        ToolCall::Bash(b) => {
            assert_eq!(b.command, "npm test");
            assert_eq!(b.timeout, Some(5000));
        }
        other => panic!("Expected Bash, got {:?}", other),
    }
}

#[test]
fn tool_call_parse_write() {
    let tc =
        ToolCall::parse("Write", &json!({"file_path": "/tmp/f", "content": "hi"})).unwrap();
    match tc {
        ToolCall::Write(w) => {
            assert_eq!(w.file_path, "/tmp/f");
            assert_eq!(w.content, "hi");
        }
        other => panic!("Expected Write, got {:?}", other),
    }
}

#[test]
fn tool_call_parse_all_builtins() {
    // Verify every built-in tool name parses without error
    let cases: Vec<(&str, serde_json::Value)> = vec![
        ("Bash", json!({"command": "ls"})),
        ("Write", json!({"file_path": "/f", "content": "c"})),
        (
            "Edit",
            json!({"file_path": "/f", "old_string": "a", "new_string": "b"}),
        ),
        ("Read", json!({"file_path": "/f"})),
        ("Glob", json!({"pattern": "*.rs"})),
        ("Grep", json!({"pattern": "TODO"})),
        (
            "WebFetch",
            json!({"url": "https://example.com", "prompt": "summarize"}),
        ),
        ("WebSearch", json!({"query": "rust serde"})),
        (
            "Task",
            json!({"prompt": "do it", "description": "d", "subagent_type": "Explore"}),
        ),
    ];
    for (name, input) in cases {
        let tc = ToolCall::parse(name, &input)
            .unwrap_or_else(|e| panic!("Failed to parse {name}: {e}"));
        assert!(
            !matches!(tc, ToolCall::Other { .. }),
            "{name} should not be Other"
        );
    }
}

#[test]
fn tool_call_parse_mcp_tool_is_other() {
    let tc =
        ToolCall::parse("mcp__memory__create_entities", &json!({"entities": []})).unwrap();
    match tc {
        ToolCall::Other {
            tool_name,
            tool_input,
        } => {
            assert_eq!(tool_name, "mcp__memory__create_entities");
            assert_eq!(tool_input, json!({"entities": []}));
        }
        other => panic!("Expected Other, got {:?}", other),
    }
}

#[test]
fn pre_tool_use_input_tool_call() {
    let input = merge(
        common_fields(),
        json!({
            "hook_event_name": "PreToolUse",
            "tool_name": "Edit",
            "tool_input": {"file_path": "/f", "old_string": "x", "new_string": "y"},
            "tool_use_id": "toolu_1"
        }),
    );
    let hook: HookInput = serde_json::from_value(input).unwrap();
    if let HookInput::PreToolUse(e) = &hook {
        let tc = e.tool_call().unwrap();
        match tc {
            ToolCall::Edit(edit) => {
                assert_eq!(edit.old_string, "x");
                assert_eq!(edit.new_string, "y");
            }
            other => panic!("Expected Edit, got {:?}", other),
        }
    } else {
        panic!("wrong variant");
    }
}

// =================================================================
// Tool-specific input structs
// =================================================================

#[test]
fn deserialize_edit_tool_input() {
    let v = json!({
        "file_path": "/src/main.rs",
        "old_string": "foo",
        "new_string": "bar",
        "replace_all": true
    });
    let ti: EditToolInput = serde_json::from_value(v).unwrap();
    assert_eq!(ti.file_path, "/src/main.rs");
    assert_eq!(ti.old_string, "foo");
    assert_eq!(ti.new_string, "bar");
    assert_eq!(ti.replace_all, Some(true));
}

#[test]
fn deserialize_grep_tool_input_with_dash_i() {
    let v = json!({
        "pattern": "TODO",
        "path": "/src",
        "-i": true,
        "multiline": false
    });
    let ti: GrepToolInput = serde_json::from_value(v).unwrap();
    assert_eq!(ti.pattern, "TODO");
    assert_eq!(ti.case_insensitive, Some(true));
    assert_eq!(ti.multiline, Some(false));
}

// =================================================================
// common() accessor
// =================================================================

#[test]
fn common_accessor_works_across_variants() {
    let stop = merge(
        common_fields(),
        json!({
            "hook_event_name": "Stop",
            "stop_hook_active": false
        }),
    );
    let hook: HookInput = serde_json::from_value(stop).unwrap();
    assert_eq!(hook.common().cwd, "/home/user/project");
}

// =================================================================
// Output serialization – UserPromptSubmit
// =================================================================

#[test]
fn serialize_user_prompt_submit_block_output() {
    let output = HookOutput {
        decision: Some("block".into()),
        reason: Some("Prompt rejected".into()),
        hook_specific_output: Some(HookSpecificOutput::UserPromptSubmit(
            UserPromptSubmitOutput {
                additional_context: Some("context here".into()),
            },
        )),
        ..Default::default()
    };

    let v = serde_json::to_value(&output).unwrap();
    assert_eq!(v["decision"], "block");
    assert_eq!(v["reason"], "Prompt rejected");
    assert_eq!(v["hookSpecificOutput"]["hookEventName"], "UserPromptSubmit");
    assert_eq!(
        v["hookSpecificOutput"]["additionalContext"],
        "context here"
    );
    // Fields that are None should be absent
    assert!(v.get("continue").is_none());
    assert!(v.get("stopReason").is_none());
}

// =================================================================
// Output serialization – PostToolUse
// =================================================================

#[test]
fn serialize_post_tool_use_block_output() {
    let output = HookOutput {
        decision: Some("block".into()),
        reason: Some("Lint failed".into()),
        hook_specific_output: Some(HookSpecificOutput::PostToolUse(PostToolUseOutput {
            additional_context: Some("Fix lint errors first".into()),
            updated_mcp_tool_output: None,
        })),
        ..Default::default()
    };

    let v = serde_json::to_value(&output).unwrap();
    assert_eq!(v["decision"], "block");
    assert_eq!(v["reason"], "Lint failed");
    assert_eq!(v["hookSpecificOutput"]["hookEventName"], "PostToolUse");
    assert_eq!(
        v["hookSpecificOutput"]["additionalContext"],
        "Fix lint errors first"
    );
    // updatedMCPToolOutput is None → absent
    assert!(v["hookSpecificOutput"].get("updatedMCPToolOutput").is_none());
}

#[test]
fn serialize_post_tool_use_with_mcp_override() {
    let output = HookOutput {
        hook_specific_output: Some(HookSpecificOutput::PostToolUse(PostToolUseOutput {
            additional_context: None,
            updated_mcp_tool_output: Some(json!({"result": "overridden"})),
        })),
        ..Default::default()
    };

    let v = serde_json::to_value(&output).unwrap();
    assert_eq!(
        v["hookSpecificOutput"]["updatedMCPToolOutput"]["result"],
        "overridden"
    );
}

// =================================================================
// Output serialization – continue: false
// =================================================================

#[test]
fn serialize_stop_processing_output() {
    let output = HookOutput {
        continue_processing: Some(false),
        stop_reason: Some("Build failed".into()),
        ..Default::default()
    };

    let v = serde_json::to_value(&output).unwrap();
    assert_eq!(v["continue"], false);
    assert_eq!(v["stopReason"], "Build failed");
}

// =================================================================
// Output serialization – PreToolUse (for completeness)
// =================================================================

#[test]
fn serialize_pre_tool_use_deny_output() {
    let output = HookOutput {
        hook_specific_output: Some(HookSpecificOutput::PreToolUse(PreToolUseOutput {
            permission_decision: Some(PreToolUsePermissionDecision::Deny),
            permission_decision_reason: Some("rm -rf blocked".into()),
            updated_input: None,
            additional_context: None,
        })),
        ..Default::default()
    };

    let v = serde_json::to_value(&output).unwrap();
    assert_eq!(v["hookSpecificOutput"]["hookEventName"], "PreToolUse");
    assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "deny");
    assert_eq!(
        v["hookSpecificOutput"]["permissionDecisionReason"],
        "rm -rf blocked"
    );
}

// =================================================================
// Round-trip: serialize then deserialize output
// =================================================================

#[test]
fn output_round_trip() {
    let original = HookOutput {
        continue_processing: Some(true),
        suppress_output: Some(true),
        system_message: Some("warning".into()),
        decision: Some("block".into()),
        reason: Some("test".into()),
        hook_specific_output: Some(HookSpecificOutput::PostToolUse(PostToolUseOutput {
            additional_context: Some("ctx".into()),
            updated_mcp_tool_output: None,
        })),
        ..Default::default()
    };

    let json_str = serde_json::to_string(&original).unwrap();
    let deserialized: HookOutput = serde_json::from_str(&json_str).unwrap();

    assert_eq!(deserialized.continue_processing, Some(true));
    assert_eq!(deserialized.suppress_output, Some(true));
    assert_eq!(deserialized.system_message.as_deref(), Some("warning"));
    assert_eq!(deserialized.decision.as_deref(), Some("block"));
    assert_eq!(deserialized.reason.as_deref(), Some("test"));
}
