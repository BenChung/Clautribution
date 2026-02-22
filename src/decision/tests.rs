use super::*;
use crate::transcript::{Transcript, Verbosity};
use serde_json::json;

// ===================================================================
// Test helpers
// ===================================================================

/// Build a Transcript from a slice of JSON values (one per JSONL line).
fn make_transcript(lines: &[serde_json::Value]) -> Transcript {
    let contents = lines
        .iter()
        .map(|v| serde_json::to_string(v).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    let (transcript, errors) = Transcript::parse(&contents);
    assert!(errors.is_empty(), "transcript parse errors: {errors:?}");
    transcript
}

/// Minimal user entry with text content.
fn user_entry(uuid: &str, parent: Option<&str>, text: &str) -> serde_json::Value {
    let mut v = json!({
        "type": "user",
        "uuid": uuid,
        "isSidechain": false,
        "userType": "external",
        "cwd": "/tmp",
        "sessionId": "s",
        "timestamp": "t",
        "version": "v",
        "message": { "role": "user", "content": text }
    });
    if let Some(p) = parent {
        v["parentUuid"] = json!(p);
    }
    v
}

/// Minimal assistant entry with text content.
fn asst_entry(uuid: &str, parent: &str, text: &str) -> serde_json::Value {
    json!({
        "type": "assistant",
        "uuid": uuid,
        "parentUuid": parent,
        "isSidechain": false,
        "userType": "external",
        "cwd": "/tmp",
        "sessionId": "s",
        "timestamp": "t",
        "version": "v",
        "message": { "role": "assistant", "content": [{"type": "text", "text": text}] }
    })
}

/// Progress entry (hook_progress).
fn progress_entry(uuid: &str, parent: &str) -> serde_json::Value {
    json!({
        "type": "progress",
        "uuid": uuid,
        "parentUuid": parent,
        "isSidechain": false,
        "cwd": "/tmp",
        "sessionId": "s",
        "timestamp": "t",
        "version": "v",
        "data": { "type": "hook_progress", "hookEvent": "Stop", "hookName": "Stop" }
    })
}

/// System entry (e.g. stop_hook_summary).
fn system_entry(uuid: &str, parent: &str) -> serde_json::Value {
    json!({
        "type": "system",
        "uuid": uuid,
        "parentUuid": parent,
        "subtype": "stop_hook_summary",
        "cwd": "/tmp",
        "sessionId": "s",
        "timestamp": "t",
        "version": "v"
    })
}

/// Build a default StopContext with common defaults.
fn make_ctx<'a>(
    transcript: &'a Transcript,
    file_metadata: Option<PromptMetadata>,
    has_uncommitted: bool,
) -> StopContext<'a> {
    StopContext {
        transcript,
        file_metadata,
        pending_plan: None,
        plan_context: None,
        plan_entries: vec![],
        session_id: "test-session",
        breadcrumb: None,
        committed_tail: None,
        has_uncommitted_changes: has_uncommitted,
        commit_template: "{{ prompt }}",
        verbosity: Verbosity::Medium,
    }
}

fn meta(prompt: &str, uuid: Option<&str>) -> PromptMetadata {
    PromptMetadata {
        prompt: prompt.to_string(),
        session_id: "s".to_string(),
        uuid: uuid.map(String::from),
    }
}

// ===================================================================
// Tests
// ===================================================================

// 1. Empty transcript → NoTail
#[test]
fn empty_transcript_returns_no_tail() {
    let t = Transcript::empty();
    let ctx = make_ctx(&t, Some(meta("hello", None)), false);
    let decision = decide_stop(&ctx).unwrap();
    assert!(matches!(decision, StopDecision::NoTail));
}

// 2. No metadata, no fallback → NoMetadata
#[test]
fn no_metadata_no_fallback_returns_no_metadata() {
    // No file_metadata, no pending_plan, and last_user_text returns "hello"
    // but we need ALL three sources to fail. Since user_entry has text,
    // last_user_text will work. So test with a transcript that has no user text.
    let lines = vec![json!({
        "type": "assistant",
        "uuid": "a1",
        "isSidechain": false,
        "userType": "external",
        "cwd": "/tmp",
        "sessionId": "s",
        "timestamp": "t",
        "version": "v",
        "message": { "role": "assistant", "content": [{"type": "text", "text": "hi"}] }
    })];
    let t = make_transcript(&lines);
    let ctx = make_ctx(&t, None, false);
    let decision = decide_stop(&ctx).unwrap();
    assert!(matches!(decision, StopDecision::NoMetadata));
}

// 3. Nonproductive turn → breadcrumb
#[test]
fn nonproductive_turn_returns_breadcrumb() {
    let t = make_transcript(&[
        user_entry("u1", None, "hello"),
        asst_entry("a1", "u1", "hi"),
    ]);
    let ctx = make_ctx(&t, Some(meta("hello", Some("u1"))), false);
    let decision = decide_stop(&ctx).unwrap();
    match decision {
        StopDecision::Nonproductive {
            hint_message,
            breadcrumb,
            plan_snapshot,
            pending_plan,
            ..
        } => {
            assert!(hint_message.contains("nonproductive"));
            assert_eq!(breadcrumb.tail_uuid, "a1");
            assert_eq!(breadcrumb.session_id, "s");
            assert!(plan_snapshot.is_none());
            assert!(pending_plan.is_none());
        }
        other => panic!("expected Nonproductive, got: {other:?}"),
    }
}

// 4. Productive turn → commit message
#[test]
fn productive_turn_returns_commit_message() {
    let t = make_transcript(&[
        user_entry("u1", None, "fix the bug"),
        asst_entry("a1", "u1", "fixed it"),
    ]);
    let ctx = make_ctx(&t, Some(meta("fix the bug", Some("u1"))), true);
    let decision = decide_stop(&ctx).unwrap();
    match decision {
        StopDecision::Productive {
            hint_message,
            commit_message,
            transcript_note_entries,
            simple_notes,
            consumed_pending_plan,
            ..
        } => {
            assert!(hint_message.contains("committed"));
            assert!(commit_message.contains("fix the bug"));
            assert!(!transcript_note_entries.is_empty());
            // Check simple_notes has prompt, session, tail
            let refs: Vec<&str> = simple_notes.iter().map(|(r, _)| r.as_str()).collect();
            assert!(refs.contains(&"refs/notes/prompt"));
            assert!(refs.contains(&"refs/notes/session"));
            assert!(refs.contains(&"refs/notes/tail"));
            assert!(!consumed_pending_plan);
        }
        other => panic!("expected Productive, got: {other:?}"),
    }
}

// 5. Reset detected via committed_tail
#[test]
fn reset_after_productive_via_committed_tail() {
    // Transcript has u1→a1 and a separate branch u1→u2→a2.
    // committed_tail is "a1", current tail is "a2" (not an ancestor of a1).
    let t = make_transcript(&[
        user_entry("u1", None, "hello"),
        asst_entry("a1", "u1", "hi"),
        user_entry("u2", Some("u1"), "try again"),
        asst_entry("a2", "u2", "retrying"),
    ]);
    let mut ctx = make_ctx(&t, Some(meta("try again", Some("u2"))), false);
    ctx.committed_tail = Some("a1".to_string());

    let decision = decide_stop(&ctx).unwrap();
    match decision {
        StopDecision::Nonproductive { hint_message, .. } => {
            assert!(hint_message.contains("reset detected"), "got: {hint_message}");
        }
        other => panic!("expected Nonproductive, got: {other:?}"),
    }
}

// 6. Reset detected via breadcrumb
#[test]
fn reset_after_nonproductive_via_breadcrumb() {
    let t = make_transcript(&[
        user_entry("u1", None, "hello"),
        asst_entry("a1", "u1", "hi"),
        user_entry("u2", Some("u1"), "try again"),
        asst_entry("a2", "u2", "retrying"),
    ]);
    let mut ctx = make_ctx(&t, Some(meta("try again", Some("u2"))), false);
    ctx.breadcrumb = Some(ContinuationBreadcrumb {
        tail_uuid: "a1".to_string(),
        session_id: "s".to_string(),
    });

    let decision = decide_stop(&ctx).unwrap();
    match decision {
        StopDecision::Nonproductive { hint_message, .. } => {
            assert!(hint_message.contains("reset detected"), "got: {hint_message}");
        }
        other => panic!("expected Nonproductive, got: {other:?}"),
    }
}

// 7. Normal continuation → no false reset
#[test]
fn normal_continuation_no_false_reset() {
    let t = make_transcript(&[
        user_entry("u1", None, "hello"),
        asst_entry("a1", "u1", "hi"),
        user_entry("u2", Some("a1"), "more"),
        asst_entry("a2", "u2", "done"),
    ]);
    let mut ctx = make_ctx(&t, Some(meta("more", Some("u2"))), false);
    ctx.committed_tail = Some("a1".to_string());

    let decision = decide_stop(&ctx).unwrap();
    match decision {
        StopDecision::Nonproductive { hint_message, .. } => {
            assert!(!hint_message.contains("reset"), "got: {hint_message}");
        }
        other => panic!("expected Nonproductive, got: {other:?}"),
    }
}

// 8. Multiple consecutive resets
#[test]
fn multiple_consecutive_resets() {
    // u1→a1 (branch 1), u1→u2→a2 (branch 2), breadcrumb points to a1
    // a2 is not an ancestor of a1 → reset
    let t = make_transcript(&[
        user_entry("u1", None, "hello"),
        asst_entry("a1", "u1", "hi"),
        user_entry("u2", Some("u1"), "retry"),
        asst_entry("a2", "u2", "ok"),
    ]);
    let mut ctx = make_ctx(&t, Some(meta("retry", Some("u2"))), false);
    ctx.breadcrumb = Some(ContinuationBreadcrumb {
        tail_uuid: "a1".to_string(),
        session_id: "s".to_string(),
    });

    let decision = decide_stop(&ctx).unwrap();
    match decision {
        StopDecision::Nonproductive { hint_message, breadcrumb, .. } => {
            assert!(hint_message.contains("reset detected"), "got: {hint_message}");
            assert_eq!(breadcrumb.tail_uuid, "a2");
        }
        other => panic!("expected Nonproductive, got: {other:?}"),
    }
}

// 9. Missing prev_tail → no false reset
#[test]
fn missing_prev_tail_no_false_reset() {
    let t = make_transcript(&[
        user_entry("u1", None, "hello"),
        asst_entry("a1", "u1", "hi"),
    ]);
    // prev_tail points to a UUID that doesn't exist in the transcript
    let mut ctx = make_ctx(&t, Some(meta("hello", Some("u1"))), false);
    ctx.committed_tail = Some("nonexistent".to_string());

    let decision = decide_stop(&ctx).unwrap();
    match decision {
        StopDecision::Nonproductive { hint_message, .. } => {
            assert!(!hint_message.contains("reset"), "got: {hint_message}");
        }
        other => panic!("expected Nonproductive, got: {other:?}"),
    }
}

// 10. Metadata fallback to pending plan
#[test]
fn metadata_fallback_to_pending_plan() {
    let t = make_transcript(&[
        user_entry("u1", None, "hello"),
        asst_entry("a1", "u1", "hi"),
    ]);
    let mut ctx = make_ctx(&t, None, false);
    ctx.pending_plan = Some("Implement the feature\n\nDetailed steps...".to_string());

    let decision = decide_stop(&ctx).unwrap();
    match decision {
        StopDecision::Nonproductive { hint_message, .. } => {
            assert!(hint_message.contains("nonproductive"), "got: {hint_message}");
        }
        other => panic!("expected Nonproductive, got: {other:?}"),
    }
}

// 11. Metadata fallback to transcript
#[test]
fn metadata_fallback_to_transcript() {
    let t = make_transcript(&[
        user_entry("u1", None, "hello from transcript"),
        asst_entry("a1", "u1", "hi"),
    ]);
    // No file_metadata, no pending_plan → falls back to last_user_text
    let ctx = make_ctx(&t, None, true);

    let decision = decide_stop(&ctx).unwrap();
    match decision {
        StopDecision::Productive { commit_message, .. } => {
            assert!(commit_message.contains("hello from transcript"), "got: {commit_message}");
        }
        other => panic!("expected Productive, got: {other:?}"),
    }
}

// 12. Commit message includes plan and summary
#[test]
fn commit_message_includes_plan_and_summary() {
    let t = make_transcript(&[
        user_entry("u1", None, "fix it"),
        json!({
            "type": "assistant",
            "uuid": "a1",
            "parentUuid": "u1",
            "isSidechain": false,
            "userType": "external",
            "cwd": "/tmp",
            "sessionId": "s",
            "timestamp": "t",
            "version": "v",
            "message": { "role": "assistant", "content": [
                { "type": "tool_use", "id": "t1", "name": "Edit", "input": { "file_path": "/src/main.rs", "old_string": "a", "new_string": "b" } },
                { "type": "text", "text": "Fixed the issue." }
            ]}
        }),
    ]);
    let mut ctx = make_ctx(&t, Some(meta("fix it", Some("u1"))), true);
    ctx.pending_plan = Some("Step 1: fix\nStep 2: test".to_string());

    let decision = decide_stop(&ctx).unwrap();
    match decision {
        StopDecision::Productive {
            commit_message,
            consumed_pending_plan,
            ..
        } => {
            assert!(commit_message.contains("fix it"), "prompt: {commit_message}");
            assert!(commit_message.contains("## Plan"), "plan: {commit_message}");
            assert!(commit_message.contains("Step 1: fix"), "plan content: {commit_message}");
            assert!(commit_message.contains("edited: main.rs"), "summary: {commit_message}");
            assert!(consumed_pending_plan);
        }
        other => panic!("expected Productive, got: {other:?}"),
    }
}

// 13. Productive after nonproductive → expanded transcript span
#[test]
fn productive_after_nonproductive() {
    let t = make_transcript(&[
        user_entry("u1", None, "hello"),
        asst_entry("a1", "u1", "hi"),
        user_entry("u2", Some("a1"), "do more"),
        asst_entry("a2", "u2", "done"),
    ]);
    let mut ctx = make_ctx(&t, Some(meta("do more", Some("u2"))), true);
    // Breadcrumb from nonproductive stop at a1, no committed_tail
    ctx.breadcrumb = Some(ContinuationBreadcrumb {
        tail_uuid: "a1".to_string(),
        session_id: "s".to_string(),
    });

    let decision = decide_stop(&ctx).unwrap();
    match decision {
        StopDecision::Productive {
            hint_message,
            transcript_note_entries,
            ..
        } => {
            assert!(!hint_message.contains("reset"), "should not detect reset");
            // With no committed_tail, transcript spans full chain
            assert!(transcript_note_entries.len() >= 2, "expanded transcript: {} entries", transcript_note_entries.len());
        }
        other => panic!("expected Productive, got: {other:?}"),
    }
}

// 14. Breadcrumb priority over committed_tail
#[test]
fn breadcrumb_priority_over_committed_tail() {
    // u1→a1→u2→a2, and a separate branch u1→u3→a3
    // Breadcrumb says tail=a2 (correctly continues from a2)
    // committed_tail says "a1" (would falsely flag reset if used)
    // Since a3 extends from u1, it's NOT an ancestor of a2 via breadcrumb → reset.
    // But if we set breadcrumb to a2 and current chain is u1→a1→u2→a2→u3→a3, no reset.
    let t = make_transcript(&[
        user_entry("u1", None, "hello"),
        asst_entry("a1", "u1", "hi"),
        user_entry("u2", Some("a1"), "more"),
        asst_entry("a2", "u2", "ok"),
        user_entry("u3", Some("a2"), "even more"),
        asst_entry("a3", "u3", "done"),
    ]);
    let mut ctx = make_ctx(&t, Some(meta("even more", Some("u3"))), false);
    // Breadcrumb correctly continues: a3 is descendant of a2
    ctx.breadcrumb = Some(ContinuationBreadcrumb {
        tail_uuid: "a2".to_string(),
        session_id: "s".to_string(),
    });
    // committed_tail would cause false reset if breadcrumb didn't take priority
    ctx.committed_tail = Some("nonexistent-should-be-ignored".to_string());

    let decision = decide_stop(&ctx).unwrap();
    match decision {
        StopDecision::Nonproductive { hint_message, .. } => {
            // Breadcrumb takes priority, a2 is ancestor of a3 → no reset
            assert!(!hint_message.contains("reset"), "breadcrumb should take priority: {hint_message}");
        }
        other => panic!("expected Nonproductive, got: {other:?}"),
    }
}

// 15. Bad template → DecisionError
#[test]
fn bad_template_returns_error() {
    let t = make_transcript(&[
        user_entry("u1", None, "hello"),
        asst_entry("a1", "u1", "hi"),
    ]);
    let mut ctx = make_ctx(&t, Some(meta("hello", Some("u1"))), true);
    ctx.commit_template = "{{ invalid syntax {% %}";

    let result = decide_stop(&ctx);
    match result {
        Err(DecisionError::TemplateRender(_)) => {}
        Ok(_) => panic!("expected template error"),
    }
}

// 16. detect_reset standalone
#[test]
fn detect_reset_standalone() {
    // Linear chain: u1→a1→u2→a2
    let t = make_transcript(&[
        user_entry("u1", None, "hello"),
        asst_entry("a1", "u1", "hi"),
        user_entry("u2", Some("a1"), "more"),
        asst_entry("a2", "u2", "done"),
    ]);
    let ctx_no_reset = StopContext {
        transcript: &t,
        file_metadata: None,
        pending_plan: None,
        plan_context: None,
        plan_entries: vec![],
        session_id: "s",
        breadcrumb: None,
        committed_tail: Some("a1".to_string()),
        has_uncommitted_changes: false,
        commit_template: "{{ prompt }}",
        verbosity: Verbosity::Medium,
    };
    assert!(detect_reset(&ctx_no_reset, "a2").is_empty(), "no reset for linear chain");

    // Branching: u1→a1 and u1→u2→a2, committed_tail=a1
    let t2 = make_transcript(&[
        user_entry("u1", None, "hello"),
        asst_entry("a1", "u1", "hi"),
        user_entry("u2", Some("u1"), "retry"),
        asst_entry("a2", "u2", "ok"),
    ]);
    let ctx_reset = StopContext {
        transcript: &t2,
        file_metadata: None,
        pending_plan: None,
        plan_context: None,
        plan_entries: vec![],
        session_id: "s",
        breadcrumb: None,
        committed_tail: Some("a1".to_string()),
        has_uncommitted_changes: false,
        commit_template: "{{ prompt }}",
        verbosity: Verbosity::Medium,
    };
    let hints = detect_reset(&ctx_reset, "a2");
    assert!(!hints.is_empty(), "should detect reset for branch");
    assert!(hints[0].contains("reset detected"));
}

// 17. No false reset with progress entries
#[test]
fn no_false_reset_with_progress_entries() {
    // u1→a1→p1 (progress), then u2 branches from a1 (not p1)
    let t = make_transcript(&[
        user_entry("u1", None, "hello"),
        asst_entry("a1", "u1", "hi"),
        progress_entry("p1", "a1"),
        user_entry("u2", Some("a1"), "save it"),
        asst_entry("a2", "u2", "saved"),
    ]);
    // Breadcrumb stored conversation_tail (a1), not progress tail (p1)
    let mut ctx = make_ctx(&t, Some(meta("save it", Some("u2"))), true);
    ctx.breadcrumb = Some(ContinuationBreadcrumb {
        tail_uuid: "a1".to_string(), // conversation_tail, not p1
        session_id: "s".to_string(),
    });

    let decision = decide_stop(&ctx).unwrap();
    match decision {
        StopDecision::Productive { hint_message, .. } => {
            assert!(!hint_message.contains("reset"), "progress entries should not cause false reset: {hint_message}");
        }
        other => panic!("expected Productive, got: {other:?}"),
    }
}

// 18. Breadcrumb stores conversation_tail, not raw tail
#[test]
fn breadcrumb_stores_conversation_tail() {
    // Transcript ends with progress entry: u1→a1→p1
    // conversation_tail should be a1, not p1
    let t = make_transcript(&[
        user_entry("u1", None, "hello"),
        asst_entry("a1", "u1", "hi"),
        progress_entry("p1", "a1"),
    ]);
    let ctx = make_ctx(&t, Some(meta("hello", Some("u1"))), false);
    let decision = decide_stop(&ctx).unwrap();
    match decision {
        StopDecision::Nonproductive { breadcrumb, .. } => {
            assert_eq!(breadcrumb.tail_uuid, "a1", "should use conversation_tail, not raw tail p1");
        }
        other => panic!("expected Nonproductive, got: {other:?}"),
    }
}

// 19. Plan prompt extraction — pending plan (Source 2)
#[test]
fn plan_prompt_from_pending_plan() {
    let t = make_transcript(&[
        user_entry("u1", None, "hello"),
        asst_entry("a1", "u1", "hi"),
    ]);
    let mut ctx = make_ctx(&t, None, true);
    ctx.pending_plan = Some("# Plan: Toggle Statement Truthiness\n\nDetailed steps...".to_string());

    let decision = decide_stop(&ctx).unwrap();
    match decision {
        StopDecision::Productive { commit_message, .. } => {
            // Prompt should be the plan title with # stripped, not the raw first line.
            assert!(
                commit_message.starts_with("Plan: Toggle Statement Truthiness"),
                "prompt should be clean plan title: {commit_message}"
            );
            // Plan appears in ## Plan section
            assert!(commit_message.contains("## Plan"), "should include plan section");
            // Plan should NOT appear twice (no duplication)
            let plan_count = commit_message.matches("Toggle Statement Truthiness").count();
            assert_eq!(plan_count, 2, "plan title should appear exactly twice (prompt + ## Plan heading): {commit_message}");
        }
        other => panic!("expected Productive, got: {other:?}"),
    }
}

// 20. Plan prompt extraction — transcript fallback with planContent (Source 3)
#[test]
fn plan_prompt_from_plan_content_not_scaffolding() {
    // Simulate Claude Code's auto-injected plan implementation prompt.
    let scaffolding = "Implement the following plan:\n\n# Plan: Do the thing\n\nSteps here\n\nIf you need specific details...";
    let plan_content = "# Plan: Do the thing\n\nSteps here";

    let mut user = user_entry("u1", None, scaffolding);
    user["planContent"] = serde_json::Value::String(plan_content.to_string());

    let t = make_transcript(&[
        user,
        asst_entry("a1", "u1", "done"),
    ]);
    // No file_metadata, no pending_plan → falls to Source 3 (transcript).
    let ctx = make_ctx(&t, None, true);

    let decision = decide_stop(&ctx).unwrap();
    match decision {
        StopDecision::Productive { commit_message, simple_notes, .. } => {
            // The prompt should be the clean plan title, not the scaffolding.
            assert!(
                commit_message.starts_with("Plan: Do the thing"),
                "should use plan title, not scaffolding: {commit_message}"
            );
            assert!(
                !commit_message.contains("Implement the following plan"),
                "scaffolding should not appear in commit: {commit_message}"
            );
            // Check prompt note also uses clean title.
            let prompt_note = simple_notes.iter().find(|(r, _)| r == "refs/notes/prompt").unwrap();
            assert_eq!(prompt_note.1, "Plan: Do the thing");
        }
        other => panic!("expected Productive, got: {other:?}"),
    }
}

// 21. Plan prompt — plain text (no heading)
#[test]
fn plan_prompt_plain_text_no_heading() {
    let t = make_transcript(&[
        user_entry("u1", None, "hello"),
        asst_entry("a1", "u1", "hi"),
    ]);
    let mut ctx = make_ctx(&t, None, true);
    ctx.pending_plan = Some("Implement the feature\n\nStep 1: do it".to_string());

    let decision = decide_stop(&ctx).unwrap();
    match decision {
        StopDecision::Productive { commit_message, .. } => {
            assert!(
                commit_message.starts_with("Implement the feature"),
                "plain text plan prompt: {commit_message}"
            );
        }
        other => panic!("expected Productive, got: {other:?}"),
    }
}

// 22. Cross-session plan context: original prompt + Q&A survive session boundary
#[test]
fn cross_session_plan_context_with_qa() {
    // Simulate the implementation session: auto-injected plan prompt with
    // planContent, and a PlanContext from the planning session's nonproductive stop.
    let scaffolding = "Implement the following plan:\n\n# Plan: Add auth\n\nStep 1\n\nIf you need...";
    let plan_content = "# Plan: Add auth\n\nStep 1";

    let mut user = user_entry("u1", None, scaffolding);
    user["planContent"] = json!(plan_content);

    let t = make_transcript(&[
        user,
        asst_entry("a1", "u1", "done"),
    ]);
    let mut ctx = make_ctx(&t, None, true);
    // Simulates the plan-context.json that survived SessionEnd.
    ctx.plan_context = Some(PlanContext {
        original_prompt: "add user authentication".to_string(),
        qa: vec![
            r#""Which auth method?"="JWT""#.to_string(),
            r#""Where to store tokens?"="httpOnly cookies""#.to_string(),
        ],
        planning_session_id: None,
    });

    let decision = decide_stop(&ctx).unwrap();
    match decision {
        StopDecision::Productive {
            commit_message,
            simple_notes,
            consumed_plan_context,
            ..
        } => {
            // Original prompt from planning session, not scaffolding.
            assert!(
                commit_message.starts_with("add user authentication"),
                "should use original prompt: {commit_message}"
            );
            assert!(
                !commit_message.contains("Implement the following plan"),
                "no scaffolding: {commit_message}"
            );
            // Q&A section present.
            assert!(commit_message.contains("## Q&A"), "Q&A section: {commit_message}");
            assert!(commit_message.contains("JWT"), "Q&A answers: {commit_message}");
            assert!(commit_message.contains("httpOnly cookies"), "Q&A answers: {commit_message}");
            // Plan section present (from planContent fallback).
            assert!(commit_message.contains("## Plan"), "plan section: {commit_message}");
            assert!(commit_message.contains("Step 1"), "plan content: {commit_message}");
            // Prompt note uses original prompt.
            let prompt_note = simple_notes.iter().find(|(r, _)| r == "refs/notes/prompt").unwrap();
            assert_eq!(prompt_note.1, "add user authentication");
            // Plan context consumed.
            assert!(consumed_plan_context);
        }
        other => panic!("expected Productive, got: {other:?}"),
    }
}

// 23. Nonproductive stop with ExitPlanMode produces plan_context with Q&A
#[test]
fn nonproductive_with_exit_plan_mode_captures_qa() {
    // Build a turn that has AskUserQuestion + answer + ExitPlanMode.
    let t = make_transcript(&[
        user_entry("u1", None, "plan the feature"),
        json!({
            "type": "assistant", "uuid": "a1", "parentUuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [
                { "type": "tool_use", "id": "ask1", "name": "AskUserQuestion", "input": {
                    "questions": [{ "question": "Which approach?", "header": "H", "options": [], "multiSelect": false }]
                }},
            ]}
        }),
        // User answers
        json!({
            "type": "user", "uuid": "u2", "parentUuid": "a1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": [
                { "type": "tool_result", "tool_use_id": "ask1",
                  "content": "User has answered your questions: \"Which approach?\"=\"Option B\". You can now continue with the user's answers in mind." }
            ]}
        }),
        // Claude calls ExitPlanMode with the plan in the input
        json!({
            "type": "assistant", "uuid": "a2", "parentUuid": "u2",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [
                { "type": "tool_use", "id": "epm1", "name": "ExitPlanMode", "input": {
                    "plan": "# The Plan\n\nDo option B because reasons."
                }}
            ]}
        }),
    ]);
    let ctx = make_ctx(&t, Some(meta("plan the feature", Some("u1"))), false);

    let decision = decide_stop(&ctx).unwrap();
    match decision {
        StopDecision::Nonproductive {
            plan_snapshot,
            plan_context,
            ..
        } => {
            assert!(plan_snapshot.is_some(), "should capture plan snapshot");
            let pc = plan_context.expect("should produce plan_context");
            assert_eq!(pc.original_prompt, "plan the feature");
            assert_eq!(pc.qa.len(), 1);
            assert!(pc.qa[0].contains("Option B"), "Q&A should contain answer: {:?}", pc.qa);
        }
        other => panic!("expected Nonproductive, got: {other:?}"),
    }
}

// 24. plan_entries prepended to transcript note; original prompt used
#[test]
fn plan_entries_prepended_and_original_prompt_used() {
    // Implementation session: auto-injected plan prompt with planContent.
    let scaffolding = "Implement the following plan:\n\n# Plan: Falsify facts\n\nStep 1";
    let plan_content = "# Plan: Falsify facts\n\nStep 1";
    let mut impl_user = user_entry("impl_u1", None, scaffolding);
    impl_user["planContent"] = json!(plan_content);

    let t = make_transcript(&[
        impl_user,
        asst_entry("impl_a1", "impl_u1", "done"),
    ]);

    // Fake planning session entries (as raw JSON values).
    let plan_entry1 = user_entry("plan_u1", None, "make 10% of facts wrong");
    let plan_entry2 = asst_entry("plan_a1", "plan_u1", "I'll build a plan.");

    let mut ctx = make_ctx(&t, None, true);
    ctx.plan_context = Some(PlanContext {
        original_prompt: "make 10% of facts wrong".to_string(),
        qa: vec![],
        planning_session_id: None,
    });
    ctx.plan_entries = vec![plan_entry1.clone(), plan_entry2.clone()];

    let decision = decide_stop(&ctx).unwrap();
    match decision {
        StopDecision::Productive {
            commit_message,
            transcript_note_entries,
            simple_notes,
            ..
        } => {
            // Commit message uses original prompt, not plan title.
            assert!(
                commit_message.starts_with("make 10% of facts wrong"),
                "should use original prompt: {commit_message}"
            );
            // Transcript note starts with the planning entries.
            assert!(
                transcript_note_entries.len() >= 4,
                "should include planning + impl entries: {}",
                transcript_note_entries.len()
            );
            // First two entries are from the planning session.
            assert_eq!(transcript_note_entries[0]["uuid"], plan_entry1["uuid"]);
            assert_eq!(transcript_note_entries[1]["uuid"], plan_entry2["uuid"]);
            // Prompt note uses original prompt.
            let prompt_note =
                simple_notes.iter().find(|(r, _)| r == "refs/notes/prompt").unwrap();
            assert_eq!(prompt_note.1, "make 10% of facts wrong");
        }
        other => panic!("expected Productive, got: {other:?}"),
    }
}

// 25. Q&A from intervening nonproductive turn included in productive commit
#[test]
fn qa_from_nonproductive_turn_included_in_productive_commit() {
    // Simulates: productive turn 1 (edits) → nonproductive turn 2
    // (AskUserQuestion) → productive turn 3 (edits).  The Q&A from turn 2
    // should appear in turn 3's commit message.
    let t = make_transcript(&[
        // Turn 1: user prompt + edit + reply
        user_entry("u1", None, "fix 3 of them"),
        json!({
            "type": "assistant", "uuid": "a1", "parentUuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [
                { "type": "tool_use", "id": "e1", "name": "Edit",
                  "input": { "file_path": "/f.txt", "old_string": "a", "new_string": "b" } }
            ]}
        }),
        json!({
            "type": "user", "uuid": "u1r", "parentUuid": "a1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": [
                { "type": "tool_result", "tool_use_id": "e1", "content": "ok" }
            ]}
        }),
        asst_entry("a1done", "u1r", "Fixed 3 factoids."),
        progress_entry("p1", "a1done"),
        system_entry("s1", "p1"),
        // Turn 2 (nonproductive): AskUserQuestion
        user_entry("u2", Some("s1"), "ask me how many to fix next"),
        json!({
            "type": "assistant", "uuid": "a2", "parentUuid": "u2",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [
                { "type": "tool_use", "id": "ask1", "name": "AskUserQuestion", "input": {
                    "questions": [{ "question": "How many should be fixed?", "header": "H",
                                    "options": [], "multiSelect": false }]
                }}
            ]}
        }),
        json!({
            "type": "user", "uuid": "u2r", "parentUuid": "a2",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": [
                { "type": "tool_result", "tool_use_id": "ask1",
                  "content": "User has answered your questions: \"How many should be fixed?\"=\"3\". You can now continue with the user's answers in mind." }
            ]}
        }),
        asst_entry("a2done", "u2r", "Got it, 3 more."),
        progress_entry("p2", "a2done"),
        system_entry("s2", "p2"),
        // Turn 3 (productive): more edits
        user_entry("u3", Some("s2"), "any ones you like"),
        json!({
            "type": "assistant", "uuid": "a3", "parentUuid": "u3",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [
                { "type": "tool_use", "id": "e2", "name": "Edit",
                  "input": { "file_path": "/f.txt", "old_string": "c", "new_string": "d" } }
            ]}
        }),
        json!({
            "type": "user", "uuid": "u3r", "parentUuid": "a3",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": [
                { "type": "tool_result", "tool_use_id": "e2", "content": "ok" }
            ]}
        }),
        asst_entry("a3done", "u3r", "Fixed 3 more."),
        progress_entry("p3", "a3done"),
        system_entry("s3", "p3"),
    ]);

    // committed_tail points to turn 1's conversation tail (a1done),
    // simulating that turn 1 was already committed.
    let mut ctx = make_ctx(&t, Some(meta("any ones you like", Some("u3"))), true);
    ctx.committed_tail = Some("a1done".to_string());

    let decision = decide_stop(&ctx).unwrap();
    match decision {
        StopDecision::Productive { commit_message, .. } => {
            assert!(
                commit_message.contains("## Q&A"),
                "commit should include Q&A section from nonproductive turn: {commit_message}"
            );
            assert!(
                commit_message.contains("\"3\""),
                "Q&A should contain the user's answer: {commit_message}"
            );
        }
        other => panic!("expected Productive, got: {other:?}"),
    }
}

// Helper for debug formatting StopDecision in panic messages
impl std::fmt::Debug for StopDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StopDecision::NoMetadata => write!(f, "NoMetadata"),
            StopDecision::NoTail => write!(f, "NoTail"),
            StopDecision::Nonproductive { hint_message, .. } => {
                write!(f, "Nonproductive({hint_message:?})")
            }
            StopDecision::Productive { hint_message, .. } => {
                write!(f, "Productive({hint_message:?})")
            }
        }
    }
}
