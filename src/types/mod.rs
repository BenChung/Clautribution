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
mod tests;
