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
