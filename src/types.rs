use serde::{Deserialize, Serialize};

// ===================================================================
// Shared Enums
// ===================================================================

/// Permission mode for the current session.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    Default,
    Plan,
    AcceptEdits,
    DontAsk,
    BypassPermissions,
}

/// How a session was started (used by SessionStart).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStartSource {
    Startup,
    Resume,
    Clear,
    Compact,
}

/// Notification type (used by Notification).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationType {
    PermissionPrompt,
    IdlePrompt,
    AuthSuccess,
    ElicitationDialog,
}

/// Compaction trigger (used by PreCompact).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CompactTrigger {
    Manual,
    Auto,
}

/// Session end reason (used by SessionEnd).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionEndReason {
    Clear,
    Logout,
    PromptInputExit,
    BypassPermissionsDisabled,
    Other,
}

// ===================================================================
// Hook Input Types (received via stdin, snake_case JSON)
// ===================================================================

/// Fields shared by all hook event inputs.
#[derive(Debug, Clone, Deserialize)]
pub struct CommonInput {
    pub session_id: String,
    pub transcript_path: String,
    pub cwd: String,
    #[serde(default)]
    pub permission_mode: Option<PermissionMode>,
}

/// A permission suggestion shown in PermissionRequest dialogs.
#[derive(Debug, Clone, Deserialize)]
pub struct PermissionSuggestion {
    #[serde(rename = "type")]
    pub suggestion_type: String,
    #[serde(default)]
    pub tool: Option<String>,
}

// --- Per-event input structs ---

#[derive(Debug, Deserialize)]
pub struct SessionStartInput {
    #[serde(flatten)]
    pub common: CommonInput,
    pub source: SessionStartSource,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub agent_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UserPromptSubmitInput {
    #[serde(flatten)]
    pub common: CommonInput,
    pub prompt: String,
}

#[derive(Debug, Deserialize)]
pub struct PreToolUseInput {
    #[serde(flatten)]
    pub common: CommonInput,
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub tool_use_id: String,
}

#[derive(Debug, Deserialize)]
pub struct PermissionRequestInput {
    #[serde(flatten)]
    pub common: CommonInput,
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    #[serde(default)]
    pub permission_suggestions: Option<Vec<PermissionSuggestion>>,
}

#[derive(Debug, Deserialize)]
pub struct PostToolUseInput {
    #[serde(flatten)]
    pub common: CommonInput,
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub tool_response: serde_json::Value,
    pub tool_use_id: String,
}

#[derive(Debug, Deserialize)]
pub struct PostToolUseFailureInput {
    #[serde(flatten)]
    pub common: CommonInput,
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub tool_use_id: String,
    pub error: String,
    #[serde(default)]
    pub is_interrupt: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct NotificationInput {
    #[serde(flatten)]
    pub common: CommonInput,
    pub message: String,
    #[serde(default)]
    pub title: Option<String>,
    pub notification_type: NotificationType,
}

#[derive(Debug, Deserialize)]
pub struct SubagentStartInput {
    #[serde(flatten)]
    pub common: CommonInput,
    pub agent_id: String,
    pub agent_type: String,
}

#[derive(Debug, Deserialize)]
pub struct SubagentStopInput {
    #[serde(flatten)]
    pub common: CommonInput,
    pub stop_hook_active: bool,
    pub agent_id: String,
    pub agent_type: String,
    pub agent_transcript_path: String,
}

#[derive(Debug, Deserialize)]
pub struct StopInput {
    #[serde(flatten)]
    pub common: CommonInput,
    pub stop_hook_active: bool,
}

#[derive(Debug, Deserialize)]
pub struct TeammateIdleInput {
    #[serde(flatten)]
    pub common: CommonInput,
    pub teammate_name: String,
    pub team_name: String,
}

#[derive(Debug, Deserialize)]
pub struct TaskCompletedInput {
    #[serde(flatten)]
    pub common: CommonInput,
    pub task_id: String,
    pub task_subject: String,
    #[serde(default)]
    pub task_description: Option<String>,
    #[serde(default)]
    pub teammate_name: Option<String>,
    #[serde(default)]
    pub team_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PreCompactInput {
    #[serde(flatten)]
    pub common: CommonInput,
    pub trigger: CompactTrigger,
    pub custom_instructions: String,
}

#[derive(Debug, Deserialize)]
pub struct SessionEndInput {
    #[serde(flatten)]
    pub common: CommonInput,
    pub reason: SessionEndReason,
}

/// Top-level hook input, deserialized from stdin JSON.
///
/// Tagged by the `hook_event_name` field to determine which event fired.
#[derive(Debug, Deserialize)]
#[serde(tag = "hook_event_name")]
pub enum HookInput {
    SessionStart(SessionStartInput),
    UserPromptSubmit(UserPromptSubmitInput),
    PreToolUse(PreToolUseInput),
    PermissionRequest(PermissionRequestInput),
    PostToolUse(PostToolUseInput),
    PostToolUseFailure(PostToolUseFailureInput),
    Notification(NotificationInput),
    SubagentStart(SubagentStartInput),
    SubagentStop(SubagentStopInput),
    Stop(StopInput),
    TeammateIdle(TeammateIdleInput),
    TaskCompleted(TaskCompletedInput),
    PreCompact(PreCompactInput),
    SessionEnd(SessionEndInput),
}

impl HookInput {
    /// Access the common fields shared by all hook events.
    pub fn common(&self) -> &CommonInput {
        match self {
            Self::SessionStart(e) => &e.common,
            Self::UserPromptSubmit(e) => &e.common,
            Self::PreToolUse(e) => &e.common,
            Self::PermissionRequest(e) => &e.common,
            Self::PostToolUse(e) => &e.common,
            Self::PostToolUseFailure(e) => &e.common,
            Self::Notification(e) => &e.common,
            Self::SubagentStart(e) => &e.common,
            Self::SubagentStop(e) => &e.common,
            Self::Stop(e) => &e.common,
            Self::TeammateIdle(e) => &e.common,
            Self::TaskCompleted(e) => &e.common,
            Self::PreCompact(e) => &e.common,
            Self::SessionEnd(e) => &e.common,
        }
    }
}

// ===================================================================
// Tool-Specific Input Types
// ===================================================================

/// Parsed tool call, matching `tool_name` to a typed `tool_input`.
#[derive(Debug)]
pub enum ToolCall {
    Bash(BashToolInput),
    Write(WriteToolInput),
    Edit(EditToolInput),
    Read(ReadToolInput),
    Glob(GlobToolInput),
    Grep(GrepToolInput),
    WebFetch(WebFetchToolInput),
    WebSearch(WebSearchToolInput),
    Task(TaskToolInput),
    /// MCP or other unknown tools — keeps the raw JSON.
    Other {
        tool_name: String,
        tool_input: serde_json::Value,
    },
}

impl PreToolUseInput {
    /// Parse `tool_name` + `tool_input` into a typed `ToolCall`.
    pub fn tool_call(&self) -> Result<ToolCall, serde_json::Error> {
        ToolCall::parse(&self.tool_name, &self.tool_input)
    }
}

impl PostToolUseInput {
    /// Parse `tool_name` + `tool_input` into a typed `ToolCall`.
    pub fn tool_call(&self) -> Result<ToolCall, serde_json::Error> {
        ToolCall::parse(&self.tool_name, &self.tool_input)
    }
}

impl ToolCall {
    pub fn parse(
        tool_name: &str,
        tool_input: &serde_json::Value,
    ) -> Result<Self, serde_json::Error> {
        match tool_name {
            "Bash" => Ok(Self::Bash(serde_json::from_value(tool_input.clone())?)),
            "Write" => Ok(Self::Write(serde_json::from_value(tool_input.clone())?)),
            "Edit" => Ok(Self::Edit(serde_json::from_value(tool_input.clone())?)),
            "Read" => Ok(Self::Read(serde_json::from_value(tool_input.clone())?)),
            "Glob" => Ok(Self::Glob(serde_json::from_value(tool_input.clone())?)),
            "Grep" => Ok(Self::Grep(serde_json::from_value(tool_input.clone())?)),
            "WebFetch" => Ok(Self::WebFetch(serde_json::from_value(tool_input.clone())?)),
            "WebSearch" => Ok(Self::WebSearch(serde_json::from_value(
                tool_input.clone(),
            )?)),
            "Task" => Ok(Self::Task(serde_json::from_value(tool_input.clone())?)),
            other => Ok(Self::Other {
                tool_name: other.to_string(),
                tool_input: tool_input.clone(),
            }),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct BashToolInput {
    pub command: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub timeout: Option<u64>,
    #[serde(default)]
    pub run_in_background: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WriteToolInput {
    pub file_path: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EditToolInput {
    pub file_path: String,
    pub old_string: String,
    pub new_string: String,
    #[serde(default)]
    pub replace_all: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReadToolInput {
    pub file_path: String,
    #[serde(default)]
    pub offset: Option<u64>,
    #[serde(default)]
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GlobToolInput {
    pub pattern: String,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GrepToolInput {
    pub pattern: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub glob: Option<String>,
    #[serde(default)]
    pub output_mode: Option<String>,
    #[serde(default, rename = "-i")]
    pub case_insensitive: Option<bool>,
    #[serde(default)]
    pub multiline: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebFetchToolInput {
    pub url: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebSearchToolInput {
    pub query: String,
    #[serde(default)]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(default)]
    pub blocked_domains: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TaskToolInput {
    pub prompt: String,
    pub description: String,
    pub subagent_type: String,
    #[serde(default)]
    pub model: Option<String>,
}

// ===================================================================
// Hook Output Types (written to stdout as JSON, camelCase)
// ===================================================================

/// Top-level hook output written to stdout on exit code 0.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookOutput {
    /// If `false`, Claude stops processing entirely after this hook.
    #[serde(rename = "continue", skip_serializing_if = "Option::is_none")]
    pub continue_processing: Option<bool>,

    /// Message shown to the user when `continue_processing` is `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,

    /// If `true`, hides stdout from verbose mode output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppress_output: Option<bool>,

    /// Warning message shown to the user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_message: Option<String>,

    /// Set to `"block"` to prevent the action.
    /// Used by UserPromptSubmit, PostToolUse, PostToolUseFailure, Stop, SubagentStop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,

    /// Explanation shown to Claude when `decision` is `"block"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// Event-specific output fields.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_specific_output: Option<HookSpecificOutput>,
}

/// Event-specific output, tagged by `hookEventName`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "hookEventName")]
pub enum HookSpecificOutput {
    SessionStart(SessionStartOutput),
    UserPromptSubmit(UserPromptSubmitOutput),
    PreToolUse(PreToolUseOutput),
    PermissionRequest(PermissionRequestOutput),
    PostToolUse(PostToolUseOutput),
    PostToolUseFailure(PostToolUseFailureOutput),
    Notification(NotificationOutput),
    SubagentStart(SubagentStartOutput),
}

// --- Per-event output structs ---

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStartOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserPromptSubmitOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
}

/// PreToolUse permission decision values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PreToolUsePermissionDecision {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreToolUseOutput {
    /// `"allow"` bypasses permission, `"deny"` blocks the call, `"ask"` prompts user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_decision: Option<PreToolUsePermissionDecision>,

    /// Reason for the permission decision.
    /// For allow/ask: shown to user. For deny: shown to Claude.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_decision_reason: Option<String>,

    /// Modified tool input parameters, applied before execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
}

/// PermissionRequest behavior values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionRequestBehavior {
    Allow,
    Deny,
}

/// The `decision` object inside PermissionRequest hookSpecificOutput.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequestDecision {
    pub behavior: PermissionRequestBehavior,

    /// For `allow`: modifies tool input before execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<serde_json::Value>,

    /// For `allow`: applies permission rule updates (equivalent to "always allow" options).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_permissions: Option<Vec<serde_json::Value>>,

    /// For `deny`: tells Claude why the permission was denied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    /// For `deny`: if true, stops Claude entirely.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interrupt: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequestOutput {
    pub decision: PermissionRequestDecision,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostToolUseOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,

    /// For MCP tools only: replaces the tool's output with this value.
    #[serde(
        rename = "updatedMCPToolOutput",
        skip_serializing_if = "Option::is_none"
    )]
    pub updated_mcp_tool_output: Option<serde_json::Value>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostToolUseFailureOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentStartOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
}

#[cfg(test)]
mod tests {
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
}
