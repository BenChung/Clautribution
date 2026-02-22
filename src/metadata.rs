use serde::{Deserialize, Serialize};

/// Metadata about the initial prompt that started this session.
/// Stored as `.claudetributer/prompt-{session_id}.json`.
#[derive(Debug, Serialize, Deserialize)]
pub struct PromptMetadata {
    pub prompt: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
}

/// Breadcrumb left after a nonproductive stop so the next productive stop
/// can walk the full transcript span since the last commit.
/// Stored as `.claudetributer/continuation-{session_id}.json`.
#[derive(Debug, Serialize, Deserialize)]
pub struct ContinuationBreadcrumb {
    pub tail_uuid: String,
    pub session_id: String,
}

/// One captured iteration of a plan: the user prompt that produced it and
/// the plan text from the `ExitPlanMode` tool call.
/// Stored as an array in `.claudetributer/plan-history-{session_id}.json`.
#[derive(Debug, Serialize, Deserialize)]
pub struct PlanSnapshot {
    pub prompt: String,
    pub plan: String,
}

/// Cross-session context for a plan: the original user prompt that initiated
/// planning and any Q&A interactions that shaped the plan.
/// Stored as `.claudetributer/plan-context.json` (project-wide, NOT
/// session-specific) so it survives across the planning→implementation
/// session boundary.  Consumed and cleared by the productive stop that
/// commits the plan's implementation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanContext {
    pub original_prompt: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub qa: Vec<String>,
    /// Session ID of the planning session whose Stop hook never fired
    /// (e.g. ExitPlanMode approval).  The JSONL transcript for that session
    /// is still on disk; we re-read it at commit time rather than copying
    /// all the entries into this file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planning_session_id: Option<String>,
}
