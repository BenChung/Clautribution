use super::*;
use serde_json::json;

#[test]
fn parse_user_text_message() {
    let input = json!({
        "type": "user",
        "uuid": "aaa",
        "parentUuid": null,
        "isSidechain": false,
        "userType": "external",
        "cwd": "/tmp",
        "sessionId": "sess-1",
        "timestamp": "2025-01-01T00:00:00Z",
        "version": "1.0",
        "message": {
            "role": "user",
            "content": "hello world"
        }
    });

    let entry: TranscriptEntry = serde_json::from_value(input).unwrap();
    match entry {
        TranscriptEntry::User(e) => {
            assert_eq!(e.uuid, "aaa");
            assert!(e.parent_uuid.is_none());
            match &e.message.content {
                MessageContent::Text(t) => assert_eq!(t, "hello world"),
                other => panic!("expected Text, got {:?}", other),
            }
        }
        other => panic!("expected User, got {:?}", other),
    }
}

#[test]
fn parse_assistant_with_text_and_tool_use() {
    let input = json!({
        "type": "assistant",
        "uuid": "bbb",
        "parentUuid": "aaa",
        "isSidechain": false,
        "userType": "external",
        "cwd": "/tmp",
        "sessionId": "sess-1",
        "timestamp": "2025-01-01T00:00:01Z",
        "version": "1.0",
        "requestId": "req-1",
        "message": {
            "role": "assistant",
            "type": "message",
            "model": "claude-opus-4-5-20251101",
            "id": "msg_01",
            "content": [
                { "type": "thinking", "thinking": "hmm", "signature": "sig" },
                { "type": "text", "text": "Let me read that file." },
                {
                    "type": "tool_use",
                    "id": "toolu_01",
                    "name": "Read",
                    "input": { "file_path": "/tmp/f.txt" }
                }
            ],
            "stop_reason": "tool_use",
            "usage": {
                "input_tokens": 100,
                "output_tokens": 50,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 80,
                "service_tier": "standard"
            }
        }
    });

    let entry: TranscriptEntry = serde_json::from_value(input).unwrap();
    match entry {
        TranscriptEntry::Assistant(e) => {
            assert_eq!(e.request_id.as_deref(), Some("req-1"));
            assert_eq!(e.message.model.as_deref(), Some("claude-opus-4-5-20251101"));
            let blocks = match &e.message.content {
                MessageContent::Blocks(b) => b,
                other => panic!("expected Blocks, got {:?}", other),
            };
            assert_eq!(blocks.len(), 3);
            assert!(matches!(&blocks[0], ContentBlock::Thinking(_)));
            assert!(matches!(&blocks[1], ContentBlock::Text(_)));
            assert!(matches!(&blocks[2], ContentBlock::ToolUse(_)));

            let usage = e.message.usage.as_ref().unwrap();
            assert_eq!(usage.input_tokens, 100);
            assert_eq!(usage.output_tokens, 50);
            assert_eq!(usage.cache_read_input_tokens, 80);
        }
        other => panic!("expected Assistant, got {:?}", other),
    }
}

#[test]
fn parse_user_tool_result() {
    let input = json!({
        "type": "user",
        "uuid": "ccc",
        "parentUuid": "bbb",
        "isSidechain": false,
        "userType": "external",
        "cwd": "/tmp",
        "sessionId": "sess-1",
        "timestamp": "2025-01-01T00:00:02Z",
        "version": "1.0",
        "message": {
            "role": "user",
            "content": [
                {
                    "type": "tool_result",
                    "tool_use_id": "toolu_01",
                    "content": "file contents here"
                }
            ]
        },
        "toolUseResult": {
            "type": "text",
            "file": {
                "filePath": "/tmp/f.txt",
                "content": "file contents here",
                "numLines": 10,
                "startLine": 1,
                "totalLines": 10
            }
        }
    });

    let entry: TranscriptEntry = serde_json::from_value(input).unwrap();
    match entry {
        TranscriptEntry::User(e) => {
            let result = e.tool_use_result.unwrap();
            match result {
                ToolUseResult::Read(r) => {
                    assert_eq!(r.result_type, "text");
                    assert_eq!(r.file.file_path, "/tmp/f.txt");
                    assert_eq!(r.file.total_lines, 10);
                }
                other => panic!("expected Read, got {:?}", other),
            }
        }
        other => panic!("expected User, got {:?}", other),
    }
}

#[test]
fn parse_progress_entry() {
    let input = json!({
        "type": "progress",
        "uuid": "ddd",
        "parentUuid": "bbb",
        "isSidechain": false,
        "userType": "external",
        "cwd": "/tmp",
        "sessionId": "sess-1",
        "timestamp": "2025-01-01T00:00:03Z",
        "version": "1.0",
        "toolUseID": "bash-progress-0",
        "parentToolUseID": "toolu_02",
        "data": {
            "type": "bash_progress",
            "output": "line 1\n",
            "fullOutput": "line 1\n",
            "elapsedTimeSeconds": 2,
            "totalLines": 1
        }
    });

    let entry: TranscriptEntry = serde_json::from_value(input).unwrap();
    match entry {
        TranscriptEntry::Progress(p) => {
            assert_eq!(p.tool_use_id.as_deref(), Some("bash-progress-0"));
            let data = p.data.as_ref().unwrap();
            assert_eq!(data.progress_type, "bash_progress");
            assert_eq!(data.total_lines, Some(1));
        }
        other => panic!("expected Progress, got {:?}", other),
    }
}

#[test]
fn parse_file_history_snapshot() {
    let input = json!({
        "type": "file-history-snapshot",
        "messageId": "msg-1",
        "isSnapshotUpdate": false,
        "snapshot": {
            "messageId": "msg-1",
            "timestamp": "2025-01-01T00:00:00Z",
            "trackedFileBackups": {
                "/tmp/f.txt": {
                    "backupFileName": "abc123@v1",
                    "version": 1,
                    "backupTime": "2025-01-01T00:00:00Z"
                }
            }
        }
    });

    let entry: TranscriptEntry = serde_json::from_value(input).unwrap();
    match entry {
        TranscriptEntry::FileHistorySnapshot(f) => {
            assert_eq!(f.message_id, "msg-1");
            let backup = f.snapshot.tracked_file_backups.get("/tmp/f.txt").unwrap();
            assert_eq!(backup.version, 1);
        }
        other => panic!("expected FileHistorySnapshot, got {:?}", other),
    }
}

#[test]
fn parse_queue_operation() {
    let input = json!({
        "type": "queue-operation",
        "operation": "enqueue",
        "timestamp": "2025-01-01T00:00:00Z",
        "sessionId": "sess-1",
        "content": "/model"
    });

    let entry: TranscriptEntry = serde_json::from_value(input).unwrap();
    match entry {
        TranscriptEntry::QueueOperation(q) => {
            assert_eq!(q.operation, "enqueue");
            assert_eq!(q.content.as_deref(), Some("/model"));
        }
        other => panic!("expected QueueOperation, got {:?}", other),
    }
}

#[test]
fn parse_system_turn_duration() {
    let input = json!({
        "type": "system",
        "uuid": "eee",
        "subtype": "turn_duration",
        "parentUuid": "bbb",
        "isSidechain": false,
        "userType": "external",
        "cwd": "/tmp",
        "sessionId": "sess-1",
        "timestamp": "2025-01-01T00:00:04Z",
        "version": "1.0",
        "durationMs": 12345,
        "isMeta": false
    });

    let entry: TranscriptEntry = serde_json::from_value(input).unwrap();
    match entry {
        TranscriptEntry::System(s) => {
            assert_eq!(s.subtype, "turn_duration");
            assert_eq!(s.duration_ms, Some(12345));
        }
        other => panic!("expected System, got {:?}", other),
    }
}

#[test]
fn parse_bash_tool_use_result() {
    let input = json!({
        "stdout": "hello\n",
        "stderr": "",
        "interrupted": false,
        "isImage": false
    });

    let result: ToolUseResult = serde_json::from_value(input).unwrap();
    match result {
        ToolUseResult::Bash(b) => {
            assert_eq!(b.stdout, "hello\n");
            assert_eq!(b.stderr, "");
        }
        other => panic!("expected Bash, got {:?}", other),
    }
}

#[test]
fn parse_edit_tool_use_result() {
    let input = json!({
        "filePath": "/tmp/f.rs",
        "oldString": "foo",
        "newString": "bar",
        "originalFile": "fn foo() {}",
        "structuredPatch": [{
            "oldStart": 1,
            "oldLines": 1,
            "newStart": 1,
            "newLines": 1,
            "lines": ["-fn foo() {}", "+fn bar() {}"]
        }],
        "userModified": false,
        "replaceAll": false
    });

    let result: ToolUseResult = serde_json::from_value(input).unwrap();
    match result {
        ToolUseResult::Edit(e) => {
            assert_eq!(e.file_path, "/tmp/f.rs");
            assert_eq!(e.old_string, "foo");
            assert_eq!(e.new_string, "bar");
            let patch = &e.structured_patch.unwrap()[0];
            assert_eq!(patch.lines.len(), 2);
        }
        other => panic!("expected Edit, got {:?}", other),
    }
}

#[test]
fn parse_transcript_helper() {
    let lines = [
        json!({
            "type": "user",
            "uuid": "a",
            "isSidechain": false,
            "userType": "external",
            "cwd": "/tmp",
            "sessionId": "s",
            "timestamp": "t",
            "version": "v",
            "message": { "role": "user", "content": "hi" }
        }),
        json!({
            "type": "system",
            "uuid": "b",
            "subtype": "turn_duration",
            "isSidechain": false,
            "userType": "external",
            "cwd": "/tmp",
            "sessionId": "s",
            "timestamp": "t",
            "version": "v",
            "durationMs": 100,
            "isMeta": false
        }),
    ];
    let contents = lines
        .iter()
        .map(|v| serde_json::to_string(v).unwrap())
        .collect::<Vec<_>>()
        .join("\n");

    let (transcript, errors) = Transcript::parse(&contents);
    assert_eq!(transcript.entries().len(), 2);
    assert!(errors.is_empty());
    assert!(matches!(&transcript.entries()[0], TranscriptEntry::User(_)));
    assert!(matches!(&transcript.entries()[1], TranscriptEntry::System(_)));
}

#[test]
fn ancestor_iter_terminates_on_cycle() {
    // Create two entries that point at each other: a→b→a
    let lines = [
        json!({
            "type": "user",
            "uuid": "a",
            "parentUuid": "b",
            "isSidechain": false,
            "userType": "external",
            "cwd": "/tmp",
            "sessionId": "s",
            "timestamp": "t",
            "version": "v",
            "message": { "role": "user", "content": "hi" }
        }),
        json!({
            "type": "assistant",
            "uuid": "b",
            "parentUuid": "a",
            "isSidechain": false,
            "userType": "external",
            "cwd": "/tmp",
            "sessionId": "s",
            "timestamp": "t",
            "version": "v",
            "message": { "role": "assistant", "content": [{"type": "text", "text": "yo"}] }
        }),
    ];
    let contents = lines
        .iter()
        .map(|v| serde_json::to_string(v).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    let (transcript, _) = Transcript::parse(&contents);

    // Without cycle detection this would loop forever.
    let ancestors: Vec<&str> = transcript.ancestors("a").filter_map(|e| e.uuid()).collect();
    assert_eq!(ancestors, vec!["a", "b"], "should visit each node once then stop");
}

#[test]
fn transcript_lookup_and_raw() {
    let lines = [
        json!({
            "type": "user",
            "uuid": "u1",
            "isSidechain": false,
            "userType": "external",
            "cwd": "/tmp",
            "sessionId": "s",
            "timestamp": "t",
            "version": "v",
            "message": { "role": "user", "content": "hello" }
        }),
    ];
    let contents = lines
        .iter()
        .map(|v| serde_json::to_string(v).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    let (transcript, _) = Transcript::parse(&contents);

    assert!(transcript.get("u1").is_some());
    assert!(transcript.get("nonexistent").is_none());
    assert!(transcript.get_raw("u1").is_some());
    assert_eq!(transcript.tail(), Some("u1"));
    assert!(transcript.uuid_exists("u1"));
    assert!(!transcript.uuid_exists("nope"));
}

#[test]
fn find_user_prompt_returns_last_match() {
    let lines = [
        json!({
            "type": "user", "uuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": "hello" }
        }),
        json!({
            "type": "user", "uuid": "u2", "parentUuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": "hello" }
        }),
    ];
    let contents = lines.iter().map(|v| serde_json::to_string(v).unwrap()).collect::<Vec<_>>().join("\n");
    let (transcript, _) = Transcript::parse(&contents);

    // Should return the *last* match (u2), not the first.
    assert_eq!(transcript.find_user_prompt("hello"), Some("u2"));
    assert_eq!(transcript.find_user_prompt("nonexistent"), None);
}

#[test]
fn turn_and_turn_raw() {
    let lines = [
        json!({
            "type": "user", "uuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": "hello" }
        }),
        json!({
            "type": "assistant", "uuid": "a1", "parentUuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [{"type": "text", "text": "hi"}] }
        }),
        json!({
            "type": "user", "uuid": "u2", "parentUuid": "a1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": "more" }
        }),
        json!({
            "type": "assistant", "uuid": "a2", "parentUuid": "u2",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [{"type": "text", "text": "done"}] }
        }),
    ];
    let contents = lines.iter().map(|v| serde_json::to_string(v).unwrap()).collect::<Vec<_>>().join("\n");
    let (transcript, _) = Transcript::parse(&contents);

    // Turn from a2 back to u2 (exclusive) should be just a2.
    let turn = transcript.turn("a2", Some("u2"));
    let uuids: Vec<&str> = turn.iter().filter_map(|e| e.uuid()).collect();
    assert_eq!(uuids, vec!["a2"]);

    // turn_raw should return the same entries in chronological order.
    let raw = transcript.turn_raw("a2", Some("u2"));
    assert_eq!(raw.len(), 1);
    assert_eq!(raw[0]["uuid"], "a2");

    // Turn with None walks to root.
    let full = transcript.turn("a2", None);
    let uuids: Vec<&str> = full.iter().filter_map(|e| e.uuid()).collect();
    assert_eq!(uuids, vec!["a2", "u2", "a1", "u1"]);
}

#[test]
fn is_ancestor_check() {
    let lines = [
        json!({
            "type": "user", "uuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": "hello" }
        }),
        json!({
            "type": "assistant", "uuid": "a1", "parentUuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [{"type": "text", "text": "hi"}] }
        }),
    ];
    let contents = lines.iter().map(|v| serde_json::to_string(v).unwrap()).collect::<Vec<_>>().join("\n");
    let (transcript, _) = Transcript::parse(&contents);

    assert!(transcript.is_ancestor("a1", "u1"));
    assert!(transcript.is_ancestor("a1", "a1")); // self is ancestor
    assert!(!transcript.is_ancestor("u1", "a1")); // wrong direction
}

#[test]
fn last_text_response_extraction() {
    let lines = [
        json!({
            "type": "user", "uuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": "hello" }
        }),
        json!({
            "type": "assistant", "uuid": "a1", "parentUuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [{"type": "text", "text": "the answer"}] }
        }),
    ];
    let contents = lines.iter().map(|v| serde_json::to_string(v).unwrap()).collect::<Vec<_>>().join("\n");
    let (transcript, _) = Transcript::parse(&contents);

    let turn = transcript.turn("a1", Some("u1"));
    assert_eq!(Transcript::last_text_response(&turn), Some("the answer".into()));

    // No assistant in chain → None.
    let user_only = transcript.turn("u1", None);
    assert_eq!(Transcript::last_text_response(&user_only), None);
}

// ---------------------------------------------------------------
// summarize_turn tests
// ---------------------------------------------------------------

/// Helper: build a transcript with a user prompt and a multi-tool assistant response,
/// returning the transcript and the turn entries' tail UUID.
fn build_tool_transcript() -> (String, Vec<serde_json::Value>) {
    let lines = vec![
        json!({
            "type": "user", "uuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": "fix the bug" }
        }),
        // First assistant message: reads files, says something
        json!({
            "type": "assistant", "uuid": "a1", "parentUuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [
                { "type": "text", "text": "Let me read the file." },
                { "type": "tool_use", "id": "t1", "name": "Read", "input": { "file_path": "/src/lib.rs" } },
                { "type": "tool_use", "id": "t2", "name": "Read", "input": { "file_path": "/src/foo.rs" } },
                { "type": "tool_use", "id": "t3", "name": "Read", "input": { "file_path": "/src/bar.rs" } },
                { "type": "tool_use", "id": "t4", "name": "Read", "input": { "file_path": "/src/baz.rs" } },
                { "type": "tool_use", "id": "t5", "name": "Read", "input": { "file_path": "/src/qux.rs" } }
            ]}
        }),
        // Tool result (user entry)
        json!({
            "type": "user", "uuid": "u2", "parentUuid": "a1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": [
                { "type": "tool_result", "tool_use_id": "t1", "content": "..." }
            ]}
        }),
        // Second assistant message: edits, runs commands, says something
        json!({
            "type": "assistant", "uuid": "a2", "parentUuid": "u2",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [
                { "type": "tool_use", "id": "t6", "name": "Edit", "input": { "file_path": "/src/main.rs", "old_string": "a", "new_string": "b" } },
                { "type": "tool_use", "id": "t7", "name": "Edit", "input": { "file_path": "/src/types.rs", "old_string": "c", "new_string": "d" } },
                { "type": "tool_use", "id": "t8", "name": "Bash", "input": { "command": "cargo test", "description": "Run tests" } },
                { "type": "tool_use", "id": "t9", "name": "Bash", "input": { "command": "cargo build", "description": "Build project" } },
                { "type": "tool_use", "id": "t10", "name": "Bash", "input": { "command": "cargo clippy" } },
                { "type": "text", "text": "I've updated the function to use snake_case." }
            ]}
        }),
    ];
    let contents = lines
        .iter()
        .map(|v| serde_json::to_string(v).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    (contents, lines)
}

#[test]
fn summarize_turn_short() {
    let (contents, _) = build_tool_transcript();
    let (transcript, _) = Transcript::parse(&contents);
    let turn = transcript.turn("a2", Some("u1"));
    let summary = Transcript::summarize_turn(&turn, Verbosity::Short).unwrap();

    // Should have tool counts and messages separated by ---
    assert!(summary.contains("edited 2 files"), "summary: {summary}");
    assert!(summary.contains("read 5 files"), "summary: {summary}");
    assert!(summary.contains("ran 3 commands"), "summary: {summary}");
    assert!(summary.contains("---"), "summary: {summary}");
    assert!(summary.contains("Let me read the file."), "summary: {summary}");
    assert!(summary.contains("I've updated the function to use snake_case."), "summary: {summary}");
}

#[test]
fn summarize_turn_medium() {
    let (contents, _) = build_tool_transcript();
    let (transcript, _) = Transcript::parse(&contents);
    let turn = transcript.turn("a2", Some("u1"));
    let summary = Transcript::summarize_turn(&turn, Verbosity::Medium).unwrap();

    // Should show filenames with cap at 3
    assert!(summary.contains("edited: main.rs, types.rs"), "summary: {summary}");
    assert!(summary.contains("read: lib.rs, foo.rs, bar.rs + 2 more"), "summary: {summary}");
    assert!(summary.contains("ran: Run tests, Build project, cargo clippy"), "summary: {summary}");
    assert!(summary.contains("---"), "summary: {summary}");
    assert!(summary.contains("Let me read the file."), "summary: {summary}");
    assert!(summary.contains("I've updated the function to use snake_case."), "summary: {summary}");
}

#[test]
fn summarize_turn_full() {
    let (contents, _) = build_tool_transcript();
    let (transcript, _) = Transcript::parse(&contents);
    let turn = transcript.turn("a2", Some("u1"));
    let summary = Transcript::summarize_turn(&turn, Verbosity::Full).unwrap();

    // Full: no cap — all 5 read files shown
    assert!(summary.contains("read: lib.rs, foo.rs, bar.rs, baz.rs, qux.rs"), "summary: {summary}");
    assert!(!summary.contains("more"), "full should not contain 'more': {summary}");
    assert!(summary.contains("edited: main.rs, types.rs"), "summary: {summary}");
}

#[test]
fn summarize_turn_text_only() {
    let lines = vec![
        json!({
            "type": "user", "uuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": "hello" }
        }),
        json!({
            "type": "assistant", "uuid": "a1", "parentUuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [
                { "type": "text", "text": "Just a text response." }
            ]}
        }),
    ];
    let contents = lines.iter().map(|v| serde_json::to_string(v).unwrap()).collect::<Vec<_>>().join("\n");
    let (transcript, _) = Transcript::parse(&contents);
    let turn = transcript.turn("a1", Some("u1"));
    let summary = Transcript::summarize_turn(&turn, Verbosity::Medium).unwrap();

    // No tools, so no --- separator
    assert!(!summary.contains("---"), "text-only should not have ---: {summary}");
    assert_eq!(summary, "Just a text response.");
}

#[test]
fn summarize_turn_empty() {
    let lines = vec![
        json!({
            "type": "user", "uuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": "hello" }
        }),
    ];
    let contents = lines.iter().map(|v| serde_json::to_string(v).unwrap()).collect::<Vec<_>>().join("\n");
    let (transcript, _) = Transcript::parse(&contents);
    let turn = transcript.turn("u1", None);
    // User-only turn: user message is included in the flow.
    assert_eq!(
        Transcript::summarize_turn(&turn, Verbosity::Short),
        Some("> hello".to_string()),
    );
}

#[test]
fn summarize_turn_deduplicates_files() {
    let lines = vec![
        json!({
            "type": "user", "uuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": "fix" }
        }),
        json!({
            "type": "assistant", "uuid": "a1", "parentUuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [
                { "type": "tool_use", "id": "t1", "name": "Edit", "input": { "file_path": "/src/main.rs", "old_string": "a", "new_string": "b" } },
                { "type": "tool_use", "id": "t2", "name": "Edit", "input": { "file_path": "/src/main.rs", "old_string": "c", "new_string": "d" } },
                { "type": "text", "text": "Done." }
            ]}
        }),
    ];
    let contents = lines.iter().map(|v| serde_json::to_string(v).unwrap()).collect::<Vec<_>>().join("\n");
    let (transcript, _) = Transcript::parse(&contents);
    let turn = transcript.turn("a1", Some("u1"));

    let short = Transcript::summarize_turn(&turn, Verbosity::Short).unwrap();
    assert!(short.contains("edited 1 file"), "dedup should show 1 file: {short}");

    let medium = Transcript::summarize_turn(&turn, Verbosity::Medium).unwrap();
    assert!(medium.contains("edited: main.rs"), "dedup detail: {medium}");
    // Should not duplicate main.rs
    assert_eq!(medium.matches("main.rs").count(), 1, "main.rs appears only once: {medium}");
}

#[test]
fn summarize_turn_messages_in_chronological_order() {
    let lines = vec![
        json!({
            "type": "user", "uuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": "go" }
        }),
        json!({
            "type": "assistant", "uuid": "a1", "parentUuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [
                { "type": "text", "text": "First message." }
            ]}
        }),
        json!({
            "type": "user", "uuid": "u2", "parentUuid": "a1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": [
                { "type": "tool_result", "tool_use_id": "t1", "content": "ok" }
            ]}
        }),
        json!({
            "type": "assistant", "uuid": "a2", "parentUuid": "u2",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [
                { "type": "text", "text": "Second message." }
            ]}
        }),
    ];
    let contents = lines.iter().map(|v| serde_json::to_string(v).unwrap()).collect::<Vec<_>>().join("\n");
    let (transcript, _) = Transcript::parse(&contents);
    let turn = transcript.turn("a2", Some("u1"));
    let summary = Transcript::summarize_turn(&turn, Verbosity::Short).unwrap();

    // Messages should be in chronological order: First before Second
    let first_pos = summary.find("First message.").unwrap();
    let second_pos = summary.find("Second message.").unwrap();
    assert!(first_pos < second_pos, "messages should be chronological: {summary}");
}

#[test]
fn summarize_turn_all_tool_categories() {
    let lines = vec![
        json!({
            "type": "user", "uuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": "do everything" }
        }),
        json!({
            "type": "assistant", "uuid": "a1", "parentUuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [
                { "type": "tool_use", "id": "t1", "name": "Edit", "input": { "file_path": "/a/main.rs", "old_string": "a", "new_string": "b" } },
                { "type": "tool_use", "id": "t2", "name": "Write", "input": { "file_path": "/a/new.rs", "content": "hi" } },
                { "type": "tool_use", "id": "t3", "name": "Read", "input": { "file_path": "/a/lib.rs" } },
                { "type": "tool_use", "id": "t4", "name": "Bash", "input": { "command": "cargo test", "description": "Run tests" } },
                { "type": "tool_use", "id": "t5", "name": "Grep", "input": { "pattern": "TODO" } },
                { "type": "tool_use", "id": "t6", "name": "Glob", "input": { "pattern": "*.rs" } },
                { "type": "tool_use", "id": "t7", "name": "WebFetch", "input": { "url": "https://example.com", "prompt": "get" } },
                { "type": "tool_use", "id": "t8", "name": "WebSearch", "input": { "query": "rust serde" } },
                { "type": "tool_use", "id": "t9", "name": "Task", "input": { "description": "explore codebase", "prompt": "look around", "subagent_type": "Explore" } },
                { "type": "text", "text": "All done." }
            ]}
        }),
    ];
    let contents = lines.iter().map(|v| serde_json::to_string(v).unwrap()).collect::<Vec<_>>().join("\n");
    let (transcript, _) = Transcript::parse(&contents);
    let turn = transcript.turn("a1", Some("u1"));

    let short = Transcript::summarize_turn(&turn, Verbosity::Short).unwrap();
    assert!(short.contains("edited 1 file"), "short: {short}");
    assert!(short.contains("wrote 1 file"), "short: {short}");
    assert!(short.contains("read 1 file"), "short: {short}");
    assert!(short.contains("ran 1 command"), "short: {short}");
    assert!(short.contains("searched 2 patterns"), "short: {short}");
    assert!(short.contains("fetched 2 urls"), "short: {short}");
    assert!(short.contains("delegated 1 task"), "short: {short}");

    let full = Transcript::summarize_turn(&turn, Verbosity::Full).unwrap();
    assert!(full.contains("edited: main.rs"), "full: {full}");
    assert!(full.contains("wrote: new.rs"), "full: {full}");
    assert!(full.contains("read: lib.rs"), "full: {full}");
    assert!(full.contains("ran: Run tests"), "full: {full}");
    assert!(full.contains("searched: TODO, *.rs"), "full: {full}");
    assert!(full.contains("fetched: https://example.com, rust serde"), "full: {full}");
    assert!(full.contains("delegated: explore codebase"), "full: {full}");
}

#[test]
fn summarize_turn_bash_falls_back_to_command() {
    let lines = vec![
        json!({
            "type": "user", "uuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": "go" }
        }),
        json!({
            "type": "assistant", "uuid": "a1", "parentUuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [
                { "type": "tool_use", "id": "t1", "name": "Bash", "input": { "command": "ls -la" } },
                { "type": "text", "text": "Listed files." }
            ]}
        }),
    ];
    let contents = lines.iter().map(|v| serde_json::to_string(v).unwrap()).collect::<Vec<_>>().join("\n");
    let (transcript, _) = Transcript::parse(&contents);
    let turn = transcript.turn("a1", Some("u1"));
    let summary = Transcript::summarize_turn(&turn, Verbosity::Medium).unwrap();
    // No description → falls back to command
    assert!(summary.contains("ran: ls -la"), "fallback to command: {summary}");
}

#[test]
fn summarize_turn_read_with_line_range() {
    let lines = vec![
        json!({
            "type": "user", "uuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": "go" }
        }),
        json!({
            "type": "assistant", "uuid": "a1", "parentUuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [
                { "type": "tool_use", "id": "t1", "name": "Read", "input": { "file_path": "/src/lib.rs", "offset": 10, "limit": 40 } },
                { "type": "tool_use", "id": "t2", "name": "Read", "input": { "file_path": "/src/main.rs" } }
            ]}
        }),
    ];
    let contents = lines.iter().map(|v| serde_json::to_string(v).unwrap()).collect::<Vec<_>>().join("\n");
    let (transcript, _) = Transcript::parse(&contents);
    let turn = transcript.turn("a1", Some("u1"));

    let full = Transcript::summarize_turn(&turn, Verbosity::Full).unwrap();
    assert!(full.contains("read: lib.rs:10-50, main.rs"), "line range: {full}");

    let short = Transcript::summarize_turn(&turn, Verbosity::Short).unwrap();
    assert!(short.contains("read 2 files"), "short: {short}");
}

#[test]
fn summarize_turn_grep_with_path_context() {
    let lines = vec![
        json!({
            "type": "user", "uuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": "go" }
        }),
        json!({
            "type": "assistant", "uuid": "a1", "parentUuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [
                { "type": "tool_use", "id": "t1", "name": "Grep", "input": { "pattern": "TODO", "path": "src/", "glob": "*.rs" } },
                { "type": "tool_use", "id": "t2", "name": "Glob", "input": { "pattern": "**/*.rs", "path": "src/" } }
            ]}
        }),
    ];
    let contents = lines.iter().map(|v| serde_json::to_string(v).unwrap()).collect::<Vec<_>>().join("\n");
    let (transcript, _) = Transcript::parse(&contents);
    let turn = transcript.turn("a1", Some("u1"));

    let full = Transcript::summarize_turn(&turn, Verbosity::Full).unwrap();
    assert!(full.contains("TODO in src/ (*.rs)"), "grep with path+glob: {full}");
    assert!(full.contains("**/*.rs in src/"), "glob with path: {full}");
}

#[test]
fn summarize_turn_bash_truncates_long_command() {
    let long_cmd = "a".repeat(120);
    let lines = vec![
        json!({
            "type": "user", "uuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": "go" }
        }),
        json!({
            "type": "assistant", "uuid": "a1", "parentUuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [
                { "type": "tool_use", "id": "t1", "name": "Bash", "input": { "command": long_cmd } }
            ]}
        }),
    ];
    let contents = lines.iter().map(|v| serde_json::to_string(v).unwrap()).collect::<Vec<_>>().join("\n");
    let (transcript, _) = Transcript::parse(&contents);
    let turn = transcript.turn("a1", Some("u1"));

    let full = Transcript::summarize_turn(&turn, Verbosity::Full).unwrap();
    assert!(full.contains("..."), "should be truncated: {full}");
    assert!(full.len() < 120, "should be shorter than original: {full}");
}

#[test]
fn summarize_turn_ask_user_question_with_answers() {
    let lines = vec![
        json!({
            "type": "user", "uuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": "plan it" }
        }),
        json!({
            "type": "assistant", "uuid": "a1", "parentUuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [
                { "type": "tool_use", "id": "ask1", "name": "AskUserQuestion", "input": {
                    "questions": [
                        { "question": "How many rounds?", "header": "Rounds", "options": [], "multiSelect": false },
                        { "question": "How many per round?", "header": "Count", "options": [], "multiSelect": false }
                    ]
                }},
                { "type": "text", "text": "Let me ask some questions." }
            ]}
        }),
        // User's answers come back as a tool_result
        json!({
            "type": "user", "uuid": "u2", "parentUuid": "a1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": [
                { "type": "tool_result", "tool_use_id": "ask1",
                  "content": "User has answered your questions: \"How many rounds?\"=\"2 rounds\", \"How many per round?\"=\"3-5\". You can now continue with the user's answers in mind." }
            ]}
        }),
    ];
    let contents = lines.iter().map(|v| serde_json::to_string(v).unwrap()).collect::<Vec<_>>().join("\n");
    let (transcript, errors) = Transcript::parse(&contents);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let turn = transcript.turn("u2", Some("u1"));

    // Short: shows question count
    let short = Transcript::summarize_turn(&turn, Verbosity::Short).unwrap();
    assert!(short.contains("asked 2 questions"), "short should count questions: {short}");

    // Medium: shows question texts
    let medium = Transcript::summarize_turn(&turn, Verbosity::Medium).unwrap();
    assert!(medium.contains("How many rounds?"), "medium should show questions: {medium}");
    assert!(medium.contains("How many per round?"), "medium should show both questions: {medium}");

    // Check Q&A extraction from tool_result
    assert!(medium.contains("Q&A:"), "should have Q&A section: {medium}");
    assert!(medium.contains("2 rounds"), "should include answers: {medium}");
}

#[test]
fn summarize_turn_ask_user_question_without_answer() {
    // AskUserQuestion with no matching tool_result (e.g. user interrupted)
    let lines = vec![
        json!({
            "type": "user", "uuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": "go" }
        }),
        json!({
            "type": "assistant", "uuid": "a1", "parentUuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [
                { "type": "tool_use", "id": "ask1", "name": "AskUserQuestion", "input": {
                    "questions": [
                        { "question": "Which approach?", "header": "Approach", "options": [], "multiSelect": false }
                    ]
                }}
            ]}
        }),
    ];
    let contents = lines.iter().map(|v| serde_json::to_string(v).unwrap()).collect::<Vec<_>>().join("\n");
    let (transcript, _) = Transcript::parse(&contents);
    let turn = transcript.turn("a1", Some("u1"));

    let medium = Transcript::summarize_turn(&turn, Verbosity::Medium).unwrap();
    assert!(medium.contains("asked: Which approach?"), "should show question even without answer: {medium}");
    // No Q&A section since there's no tool_result
    assert!(!medium.contains("Q&A:"), "no Q&A without answer: {medium}");
}

#[test]
fn stop_hook_summary_does_not_break_ancestor_chain() {
    // Reproduces a real transcript where a stop_hook_summary system entry
    // sits between two turns. If it fails to parse, the ancestor chain is
    // broken and reset detection gives a false positive.
    let lines = vec![
        json!({
            "type": "user", "uuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": "first prompt" }
        }),
        json!({
            "type": "assistant", "uuid": "a1", "parentUuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [{"type": "text", "text": "ok"}] }
        }),
        // Hook progress for Stop
        json!({
            "type": "progress", "uuid": "p1", "parentUuid": "a1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "toolUseID": "tool-1", "parentToolUseID": "tool-1",
            "data": { "type": "hook_progress", "hookEvent": "Stop", "hookName": "Stop" }
        }),
        // stop_hook_summary — has extra fields not in SystemEntry
        json!({
            "type": "system", "uuid": "sys1", "parentUuid": "p1",
            "subtype": "stop_hook_summary",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "hookCount": 1,
            "hookInfos": [{"command": "/usr/bin/hook"}],
            "hookErrors": [],
            "preventedContinuation": false,
            "stopReason": "",
            "hasOutput": true,
            "level": "suggestion",
            "toolUseID": "tool-1"
        }),
        // Second turn — parent points to the system entry
        json!({
            "type": "user", "uuid": "u2", "parentUuid": "sys1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": "second prompt" }
        }),
        json!({
            "type": "assistant", "uuid": "a2", "parentUuid": "u2",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [{"type": "text", "text": "done"}] }
        }),
    ];
    let contents = lines.iter().map(|v| serde_json::to_string(v).unwrap()).collect::<Vec<_>>().join("\n");
    let (transcript, errors) = Transcript::parse(&contents);

    // All 6 entries should parse without error
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    assert_eq!(transcript.entries().len(), 6);

    // p1 (breadcrumb tail from first stop) must be an ancestor of a2
    assert!(
        transcript.is_ancestor("a2", "p1"),
        "p1 should be ancestor of a2 through sys1"
    );
}

#[test]
fn user_texts_until_skips_committed_entries() {
    // Simulate: productive turn (committed) → nonproductive turn → plan-mode trigger.
    // committed_tail is "a1" (the assistant response of the productive turn).
    let lines = vec![
        json!({
            "type": "user", "uuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": "make a file with 10 random facts" }
        }),
        json!({
            "type": "assistant", "uuid": "a1", "parentUuid": "u1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [{"type": "text", "text": "Created file."}] }
        }),
        json!({
            "type": "user", "uuid": "u2", "parentUuid": "a1",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": "make a plan to pick one and make it wrong" }
        }),
        json!({
            "type": "assistant", "uuid": "a2", "parentUuid": "u2",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [{"type": "text", "text": "Here's a plan..."}] }
        }),
        json!({
            "type": "user", "uuid": "u3", "parentUuid": "a2",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "user", "content": "do it in plan mode" }
        }),
        json!({
            "type": "assistant", "uuid": "a3", "parentUuid": "u3",
            "isSidechain": false, "userType": "external",
            "cwd": "/tmp", "sessionId": "s", "timestamp": "t", "version": "v",
            "message": { "role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "ExitPlanMode", "input": {"plan": "the plan"}}
            ]}
        }),
    ];
    let contents = lines.iter().map(|v| serde_json::to_string(v).unwrap()).collect::<Vec<_>>().join("\n");
    let (transcript, _) = Transcript::parse(&contents);

    // With committed_tail at a1: should return u3 and u2 (post-commit user texts).
    let texts = transcript.user_texts_until("a3", Some("a1"));
    assert_eq!(texts.len(), 2);
    assert_eq!(texts[0].1, "do it in plan mode");       // most recent
    assert_eq!(texts[1].1, "make a plan to pick one and make it wrong"); // intent

    // With no committed_tail: returns all 3 user texts.
    let all = transcript.user_texts_until("a3", None);
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].1, "do it in plan mode");
    assert_eq!(all[1].1, "make a plan to pick one and make it wrong");
    assert_eq!(all[2].1, "make a file with 10 random facts");

    // With committed_tail at a2 (nonproductive turn's response): only u3.
    let just_trigger = transcript.user_texts_until("a3", Some("a2"));
    assert_eq!(just_trigger.len(), 1);
    assert_eq!(just_trigger[0].1, "do it in plan mode");
}
