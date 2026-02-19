use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

// ===================================================================
// Verbosity — controls how much tool detail appears in turn summaries
// ===================================================================

/// Controls the level of detail in `summarize_turn` output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
    /// Tool counts only: "edited 2 files, ran 3 commands"
    Short,
    /// Tool names, capped at 3 per category with "+ N more"
    Medium,
    /// All tool details, no cap
    Full,
}

// ===================================================================
// Top-level transcript entry — one per JSONL line
// ===================================================================

/// A single line in a Claude Code `.jsonl` transcript file.
///
/// Discriminated by the `type` field (camelCase JSON throughout).
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum TranscriptEntry {
    #[serde(rename = "user")]
    User(ConversationEntry),
    #[serde(rename = "assistant")]
    Assistant(ConversationEntry),
    #[serde(rename = "progress")]
    Progress(ProgressEntry),
    #[serde(rename = "file-history-snapshot")]
    FileHistorySnapshot(FileHistorySnapshotEntry),
    #[serde(rename = "queue-operation")]
    QueueOperation(QueueOperationEntry),
    #[serde(rename = "system")]
    System(SystemEntry),
}

// ===================================================================
// Conversation entries (user + assistant share the same shape)
// ===================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationEntry {
    pub uuid: String,
    #[serde(default)]
    pub parent_uuid: Option<String>,
    pub is_sidechain: bool,
    pub user_type: String,
    pub cwd: String,
    pub session_id: String,
    pub timestamp: String,
    pub version: String,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    pub message: Message,

    // --- fields that only appear on some entries ---
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub thinking_metadata: Option<ThinkingMetadata>,
    #[serde(default)]
    pub todos: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub permission_mode: Option<String>,
    #[serde(default)]
    pub tool_use_result: Option<ToolUseResult>,
    #[serde(default)]
    pub source_tool_assistant_uuid: Option<String>,
    #[serde(default)]
    pub is_meta: Option<bool>,
    /// Present on plan-implementation prompts injected by Claude Code after
    /// the user approves an ExitPlanMode plan.
    #[serde(default)]
    pub plan_content: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingMetadata {
    pub level: String,
    pub disabled: bool,
    #[serde(default)]
    pub triggers: Vec<String>,
}

// ===================================================================
// Message
// ===================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub role: String,
    pub content: MessageContent,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default, rename = "type")]
    pub message_type: Option<String>,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub stop_sequence: Option<String>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

/// `message.content` can be a plain string (user text) or an array of
/// content blocks (assistant responses, tool results).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

// ===================================================================
// Content blocks inside message.content[]
// ===================================================================

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text(TextBlock),
    #[serde(rename = "thinking")]
    Thinking(ThinkingBlock),
    #[serde(rename = "tool_use")]
    ToolUse(ToolUseBlock),
    #[serde(rename = "tool_result")]
    ToolResult(ToolResultBlock),
}

#[derive(Debug, Deserialize)]
pub struct TextBlock {
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct ThinkingBlock {
    pub thinking: String,
    #[serde(default)]
    pub signature: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ToolUseBlock {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
    #[serde(default)]
    pub caller: Option<Caller>,
}

#[derive(Debug, Deserialize)]
pub struct Caller {
    #[serde(rename = "type")]
    pub caller_type: String,
}

#[derive(Debug, Deserialize)]
pub struct ToolResultBlock {
    pub tool_use_id: String,
    pub content: serde_json::Value,
    #[serde(default)]
    pub is_error: Option<bool>,
}

// ===================================================================
// Usage (token counts on assistant messages)
// ===================================================================

#[derive(Debug, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub service_tier: Option<String>,
    #[serde(default)]
    pub cache_creation: Option<CacheCreation>,
    #[serde(default)]
    pub inference_geo: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CacheCreation {
    #[serde(default)]
    pub ephemeral_5m_input_tokens: u64,
    #[serde(default)]
    pub ephemeral_1h_input_tokens: u64,
}

// ===================================================================
// ToolUseResult — attached to user entries that carry tool responses
// ===================================================================

/// The result payload varies by tool. We use an untagged enum because
/// some variants have a `type` field and some don't.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ToolUseResult {
    Read(ReadToolResult),
    Write(WriteToolResult),
    Edit(EditToolResult),
    Bash(BashToolResult),
    /// Catch-all for tools we haven't typed yet.
    Other(serde_json::Value),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadToolResult {
    /// Always `"text"` for Read results.
    #[serde(rename = "type")]
    pub result_type: String,
    pub file: ReadFileInfo,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadFileInfo {
    pub file_path: String,
    pub content: String,
    pub num_lines: i64,
    pub start_line: i64,
    pub total_lines: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteToolResult {
    /// Always `"update"` for Write results.
    #[serde(rename = "type")]
    pub result_type: String,
    pub file_path: String,
    pub content: String,
    #[serde(default)]
    pub structured_patch: Option<Vec<DiffHunk>>,
    #[serde(default)]
    pub original_file: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditToolResult {
    pub file_path: String,
    pub old_string: String,
    pub new_string: String,
    #[serde(default)]
    pub original_file: Option<String>,
    #[serde(default)]
    pub structured_patch: Option<Vec<DiffHunk>>,
    #[serde(default)]
    pub user_modified: Option<bool>,
    #[serde(default)]
    pub replace_all: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    pub old_start: i64,
    pub old_lines: i64,
    pub new_start: i64,
    pub new_lines: i64,
    pub lines: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BashToolResult {
    pub stdout: String,
    pub stderr: String,
    #[serde(default)]
    pub interrupted: Option<bool>,
    #[serde(default)]
    pub is_image: Option<bool>,
}

// ===================================================================
// Progress entries (e.g. streaming bash output)
// ===================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEntry {
    pub uuid: String,
    #[serde(default)]
    pub parent_uuid: Option<String>,
    // Fields below vary by progress subtype (e.g. hook-fired progress
    // entries may omit toolUseID/data), so all are defaulted.
    #[serde(default)]
    pub is_sidechain: bool,
    #[serde(default)]
    pub user_type: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default, rename = "toolUseID")]
    pub tool_use_id: Option<String>,
    #[serde(default, rename = "parentToolUseID")]
    pub parent_tool_use_id: Option<String>,
    #[serde(default)]
    pub data: Option<ProgressData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressData {
    #[serde(rename = "type")]
    pub progress_type: String,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub full_output: Option<String>,
    #[serde(default)]
    pub elapsed_time_seconds: Option<f64>,
    #[serde(default)]
    pub total_lines: Option<i64>,
}

// ===================================================================
// File history snapshots
// ===================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHistorySnapshotEntry {
    pub message_id: String,
    pub snapshot: FileSnapshot,
    #[serde(default)]
    pub is_snapshot_update: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSnapshot {
    pub message_id: String,
    pub timestamp: String,
    pub tracked_file_backups: HashMap<String, FileBackup>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileBackup {
    pub backup_file_name: String,
    pub version: i64,
    pub backup_time: String,
}

// ===================================================================
// Queue operations
// ===================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueOperationEntry {
    pub operation: String,
    pub timestamp: String,
    pub session_id: String,
    #[serde(default)]
    pub content: Option<String>,
}

// ===================================================================
// System entries (e.g. turn_duration)
// ===================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemEntry {
    pub uuid: String,
    pub subtype: String,
    #[serde(default)]
    pub parent_uuid: Option<String>,
    // Fields below vary by subtype (e.g. stop_hook_summary omits isSidechain
    // and userType), so all are defaulted to allow any system entry to parse.
    #[serde(default)]
    pub is_sidechain: bool,
    #[serde(default)]
    pub user_type: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub is_meta: Option<bool>,
}

impl TranscriptEntry {
    /// Return the UUID if this entry type carries one.
    pub fn uuid(&self) -> Option<&str> {
        match self {
            Self::User(e) | Self::Assistant(e) => Some(&e.uuid),
            Self::Progress(e) => Some(&e.uuid),
            Self::System(e) => Some(&e.uuid),
            Self::FileHistorySnapshot(_) | Self::QueueOperation(_) => None,
        }
    }

    /// Return the parent UUID if this entry type carries one.
    pub fn parent_uuid(&self) -> Option<&str> {
        match self {
            Self::User(e) | Self::Assistant(e) => e.parent_uuid.as_deref(),
            Self::Progress(e) => e.parent_uuid.as_deref(),
            Self::System(e) => e.parent_uuid.as_deref(),
            Self::FileHistorySnapshot(_) | Self::QueueOperation(_) => None,
        }
    }
}

// ===================================================================
// Transcript — parsed JSONL with typed entries, raw JSON, and a UUID index
// ===================================================================

/// A parsed Claude Code JSONL transcript.
///
/// Owns the typed entries, a UUID→entry index for DAG traversal,
/// and the original raw JSON values keyed by UUID.
pub struct Transcript {
    entries: Vec<TranscriptEntry>,
    by_uuid: HashMap<String, usize>, // uuid → index into entries
    raw: HashMap<String, serde_json::Value>, // uuid → original JSONL value
}

// ===================================================================
// Iterator helpers for drilling into assistant content blocks
// ===================================================================

/// Iterate over all `ContentBlock`s from assistant entries in a slice.
fn assistant_blocks<'a>(entries: &'a [&'a TranscriptEntry]) -> impl Iterator<Item = &'a ContentBlock> + 'a {
    entries.iter().flat_map(|entry| match entry {
        TranscriptEntry::Assistant(conv) => match &conv.message.content {
            MessageContent::Blocks(b) => b.as_slice(),
            _ => &[],
        },
        _ => &[],
    })
}

/// Iterate over assistant entries, yielding each entry's content blocks as a
/// separate sub-iterator. Useful when per-entry grouping matters (e.g.
/// `last_text_response` returns text from the first entry that has any).
fn assistant_blocks_by_entry<'a>(
    entries: &'a [&'a TranscriptEntry],
) -> impl Iterator<Item = std::slice::Iter<'a, ContentBlock>> + 'a {
    entries.iter().filter_map(|entry| match entry {
        TranscriptEntry::Assistant(conv) => match &conv.message.content {
            MessageContent::Blocks(b) => Some(b.iter()),
            _ => None,
        },
        _ => None,
    })
}

impl Transcript {
    /// An empty transcript (no entries).
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            by_uuid: HashMap::new(),
            raw: HashMap::new(),
        }
    }

    /// Parse a JSONL transcript string. Returns the transcript and any
    /// lines that failed to parse (with 1-based line number and error).
    pub fn parse(contents: &str) -> (Self, Vec<(usize, String)>) {
        let mut entries = Vec::new();
        let mut errors = Vec::new();
        let mut by_uuid = HashMap::new();
        let mut raw = HashMap::new();

        for (i, line) in contents.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            // Parse the line once as a raw Value, then deserialize the typed
            // entry from the already-parsed tree to avoid double tokenization.
            match serde_json::from_str::<serde_json::Value>(line) {
                Ok(val) => {
                    match serde_json::from_value::<TranscriptEntry>(val.clone()) {
                        Ok(entry) => {
                            if let Some(uuid) = entry.uuid() {
                                by_uuid.insert(uuid.to_string(), entries.len());
                                raw.insert(uuid.to_string(), val);
                            }
                            entries.push(entry);
                        }
                        Err(e) => errors.push((i + 1, format!("{e}"))),
                    }
                }
                Err(e) => errors.push((i + 1, format!("{e}"))),
            }
        }

        (Self { entries, by_uuid, raw }, errors)
    }

    /// Look up a typed entry by UUID.
    pub fn get(&self, uuid: &str) -> Option<&TranscriptEntry> {
        self.by_uuid.get(uuid).map(|&i| &self.entries[i])
    }

    /// Look up the original raw JSON value by UUID.
    pub fn get_raw(&self, uuid: &str) -> Option<&serde_json::Value> {
        self.raw.get(uuid)
    }

    /// The UUID of the last entry in the transcript that has one.
    pub fn tail(&self) -> Option<&str> {
        self.entries.iter().rev().find_map(|e| e.uuid())
    }

    /// All typed entries in parse order.
    pub fn entries(&self) -> &[TranscriptEntry] {
        &self.entries
    }

    // ---------------------------------------------------------------
    // DAG traversal
    // ---------------------------------------------------------------

    /// Iterate ancestors starting from `uuid`, walking `parentUuid` links.
    /// Yields entries from the starting node upward (inclusive).
    /// Tracks visited UUIDs to guard against cycles.
    pub fn ancestors<'a>(&'a self, uuid: &'a str) -> AncestorIter<'a> {
        AncestorIter {
            transcript: self,
            next_uuid: Some(uuid),
            visited: HashSet::new(),
        }
    }

    /// Check whether `ancestor` is reachable from `uuid` via parentUuid links.
    pub fn is_ancestor(&self, uuid: &str, ancestor: &str) -> bool {
        self.ancestors(uuid).any(|e| e.uuid() == Some(ancestor))
    }

    // ---------------------------------------------------------------
    // Turn operations
    // ---------------------------------------------------------------

    /// Walk ancestors from `tail` back toward (but not including) `prompt_uuid`.
    /// Returns entries in reverse chronological order.
    /// If `prompt_uuid` is `None`, walks the entire chain to the root.
    pub fn turn<'a>(&'a self, tail: &'a str, prompt_uuid: Option<&str>) -> Vec<&TranscriptEntry> {
        self.ancestors(tail)
            .take_while(|e| {
                prompt_uuid.map_or(true, |pu| e.uuid() != Some(pu))
            })
            .collect()
    }

    /// Search the turn for an `ExitPlanMode` tool_use block and return the
    /// value of its `plan` input field, if present.
    pub fn find_exit_plan_mode_plan(&self, tail: &str, prompt_uuid: Option<&str>) -> Option<String> {
        let turn = self.turn(tail, prompt_uuid);
        assistant_blocks(&turn).find_map(|block| match block {
            ContentBlock::ToolUse(tu) if tu.name == "ExitPlanMode" => {
                tu.input.get("plan").and_then(|v| v.as_str()).map(String::from)
            }
            _ => None,
        })
    }

    /// Like `turn`, but returns the original raw JSON values in
    /// chronological order.
    pub fn turn_raw(&self, tail: &str, prompt_uuid: Option<&str>) -> Vec<serde_json::Value> {
        let mut values: Vec<serde_json::Value> = self
            .turn(tail, prompt_uuid)
            .iter()
            .filter_map(|e| e.uuid().and_then(|uuid| self.raw.get(uuid).cloned()))
            .collect();
        values.reverse();
        values
    }

    // ---------------------------------------------------------------
    // Content queries
    // ---------------------------------------------------------------

    /// Find the UUID of the *last* user message whose text content matches
    /// `text`. Scanning in reverse handles resets where the same prompt
    /// text may appear multiple times.
    pub fn find_user_prompt(&self, text: &str) -> Option<&str> {
        self.entries.iter().rev().find_map(|entry| {
            if let TranscriptEntry::User(conv) = entry {
                if let MessageContent::Text(t) = &conv.message.content {
                    if t == text {
                        return Some(conv.uuid.as_str());
                    }
                }
            }
            None
        })
    }

    /// Return the last user message that has plain text content (i.e. not a
    /// tool_result array). Returns `(uuid, text, plan_content)`. Useful as a
    /// fallback when UserPromptSubmit didn't fire (e.g. plan implementation
    /// prompts auto-injected after ExitPlanMode approval).
    pub fn last_user_text(&self) -> Option<(&str, &str, Option<&str>)> {
        self.entries.iter().rev().find_map(|entry| {
            if let TranscriptEntry::User(conv) = entry {
                if let MessageContent::Text(t) = &conv.message.content {
                    return Some((
                        conv.uuid.as_str(),
                        t.as_str(),
                        conv.plan_content.as_deref(),
                    ));
                }
            }
            None
        })
    }

    /// Check whether a UUID appears as any user entry in the transcript.
    pub fn uuid_exists(&self, uuid: &str) -> bool {
        self.by_uuid.contains_key(uuid)
    }

    /// Extract text from the last assistant response in a reverse-chronological
    /// chain of entries. Returns `None` if no assistant text is found.
    pub fn last_text_response(chain: &[&TranscriptEntry]) -> Option<String> {
        // Per-entry grouping: return text from the first assistant entry that
        // has any text blocks (chain is reverse-chronological, so "first" =
        // most recent).
        for blocks in assistant_blocks_by_entry(chain) {
            let text_parts: Vec<&str> = blocks
                .filter_map(|b| match b {
                    ContentBlock::Text(t) => Some(t.text.as_str()),
                    _ => None,
                })
                .collect();
            if !text_parts.is_empty() {
                return Some(text_parts.join("\n\n"));
            }
        }
        None
    }

    // ---------------------------------------------------------------
    // Turn summarization
    // ---------------------------------------------------------------

    /// Summarize a turn's tool activity and assistant text messages at the
    /// given verbosity level. `turn` should be in reverse-chronological
    /// order (as returned by `Transcript::turn`).
    ///
    /// Returns `None` if the turn has no tool activity and no text messages.
    pub fn summarize_turn(
        turn: &[&TranscriptEntry],
        verbosity: Verbosity,
    ) -> Option<String> {
        let mut cats = ToolCategories::default();
        let mut messages: Vec<String> = Vec::new();

        // Walk in reverse-chronological order (turn entries come newest-first).
        for block in assistant_blocks(turn) {
            match block {
                ContentBlock::ToolUse(tu) => cats.categorize(&tu.name, &tu.input),
                ContentBlock::Text(t) => {
                    let trimmed = t.text.trim();
                    if !trimmed.is_empty() {
                        messages.push(trimmed.to_string());
                    }
                }
                _ => {}
            }
        }

        // Messages were collected newest-first; reverse to chronological.
        messages.reverse();

        let tool_summary = match verbosity {
            Verbosity::Short => cats.format_short(),
            Verbosity::Medium => cats.format_detailed(Some(3)),
            Verbosity::Full => cats.format_detailed(None),
        };

        let messages_section = if messages.is_empty() {
            None
        } else {
            Some(messages.join("\n\n"))
        };

        match (tool_summary, messages_section) {
            (Some(tools), Some(msgs)) => Some(format!("{tools}\n---\n{msgs}")),
            (Some(tools), None) => Some(tools),
            (None, Some(msgs)) => Some(msgs),
            (None, None) => None,
        }
    }

}

// ===================================================================
// Tool categorization for turn summaries
// ===================================================================

/// Collects tool usage into named category buckets for summarization.
#[derive(Default)]
struct ToolCategories {
    edited: Vec<String>,
    wrote: Vec<String>,
    read: Vec<String>,
    ran: Vec<String>,
    searched: Vec<String>,
    fetched: Vec<String>,
    delegated: Vec<String>,
}

impl ToolCategories {
    /// Truncate a string to `max` chars, appending "..." if truncated.
    fn truncate(s: &str, max: usize) -> String {
        match s.char_indices().nth(max) {
            None => s.to_string(),
            Some((byte_idx, _)) => format!("{}...", &s[..byte_idx]),
        }
    }

    /// Classify a tool_use block into the appropriate category.
    fn categorize(&mut self, name: &str, input: &serde_json::Value) {
        match name {
            "Edit" => self.push("edited", Self::extract_filename(input, "file_path")),
            "NotebookEdit" => self.push("edited", Self::extract_filename(input, "notebook_path")),
            "Write" => self.push("wrote", Self::extract_filename(input, "file_path")),
            "Read" => {
                let mut label = Self::extract_filename(input, "file_path");
                if let Some(offset) = input["offset"].as_i64() {
                    let limit = input["limit"].as_i64().unwrap_or(2000);
                    label = format!("{label}:{offset}-{}", offset + limit);
                }
                self.push("read", label);
            }
            "Bash" => {
                let label = input["description"]
                    .as_str()
                    .map(|s| Self::truncate(s, 80))
                    .or_else(|| input["command"].as_str().map(|s| Self::truncate(s, 80)))
                    .unwrap_or_else(|| "(unknown)".to_string());
                self.push("ran", label);
            }
            "Grep" => {
                let mut label = input["pattern"].as_str().unwrap_or("(unknown)").to_string();
                if let Some(path) = input["path"].as_str() {
                    label = format!("{label} in {path}");
                }
                if let Some(glob) = input["glob"].as_str() {
                    label = format!("{label} ({glob})");
                }
                self.push("searched", label);
            }
            "Glob" => {
                let mut label = input["pattern"].as_str().unwrap_or("(unknown)").to_string();
                if let Some(path) = input["path"].as_str() {
                    label = format!("{label} in {path}");
                }
                self.push("searched", label);
            }
            "WebFetch" => {
                let label = input["url"].as_str()
                    .map(|s| Self::truncate(s, 80))
                    .unwrap_or_else(|| "(unknown)".to_string());
                self.push("fetched", label);
            }
            "WebSearch" => {
                let label = input["query"].as_str().unwrap_or("(unknown)").to_string();
                self.push("fetched", label);
            }
            "Task" => {
                let label = input["description"].as_str().unwrap_or("(unknown)").to_string();
                self.push("delegated", label);
            }
            _ => {}
        }
    }

    /// Push a value into the named category, deduplicating.
    fn push(&mut self, category: &str, value: String) {
        let vec = match category {
            "edited" => &mut self.edited,
            "wrote" => &mut self.wrote,
            "read" => &mut self.read,
            "ran" => &mut self.ran,
            "searched" => &mut self.searched,
            "fetched" => &mut self.fetched,
            "delegated" => &mut self.delegated,
            _ => return,
        };
        if !vec.contains(&value) {
            vec.push(value);
        }
    }

    /// Extract a file path field and strip it to just the filename.
    fn extract_filename(input: &serde_json::Value, field: &str) -> String {
        input[field]
            .as_str()
            .map(|p| {
                Path::new(p)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(p)
                    .to_string()
            })
            .unwrap_or_else(|| "(unknown)".to_string())
    }

    /// Ordered (label, items) pairs for formatting.
    fn as_pairs(&self) -> Vec<(&str, &Vec<String>)> {
        vec![
            ("edited", &self.edited),
            ("wrote", &self.wrote),
            ("read", &self.read),
            ("ran", &self.ran),
            ("searched", &self.searched),
            ("fetched", &self.fetched),
            ("delegated", &self.delegated),
        ]
    }

    /// Format at Short verbosity: "edited 2 files, ran 3 commands"
    fn format_short(&self) -> Option<String> {
        let parts: Vec<String> = self
            .as_pairs()
            .iter()
            .filter(|(_, items)| !items.is_empty())
            .map(|(cat, items)| {
                let count = items.len();
                let noun = match *cat {
                    "edited" | "wrote" | "read" => {
                        if count == 1 { "file" } else { "files" }
                    }
                    "ran" => {
                        if count == 1 { "command" } else { "commands" }
                    }
                    "searched" => {
                        if count == 1 { "pattern" } else { "patterns" }
                    }
                    "fetched" => {
                        if count == 1 { "url" } else { "urls" }
                    }
                    "delegated" => {
                        if count == 1 { "task" } else { "tasks" }
                    }
                    _ => "items",
                };
                format!("{cat} {count} {noun}")
            })
            .collect();

        if parts.is_empty() { None } else { Some(parts.join(", ")) }
    }

    /// Format at Medium/Full verbosity with optional item cap.
    fn format_detailed(&self, cap: Option<usize>) -> Option<String> {
        let lines: Vec<String> = self
            .as_pairs()
            .iter()
            .filter(|(_, items)| !items.is_empty())
            .map(|(cat, items)| match cap {
                Some(max) if items.len() > max => {
                    let shown: Vec<&str> = items.iter().take(max).map(|s| s.as_str()).collect();
                    let remaining = items.len() - max;
                    format!("{}: {} + {} more", cat, shown.join(", "), remaining)
                }
                _ => {
                    let all: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
                    format!("{}: {}", cat, all.join(", "))
                }
            })
            .collect();

        if lines.is_empty() { None } else { Some(lines.join("\n")) }
    }
}

/// Iterator that walks the `parentUuid` chain from a given entry upward.
/// Tracks visited UUIDs to guard against cycles in malformed transcripts.
pub struct AncestorIter<'a> {
    transcript: &'a Transcript,
    next_uuid: Option<&'a str>,
    visited: HashSet<&'a str>,
}

impl<'a> Iterator for AncestorIter<'a> {
    type Item = &'a TranscriptEntry;

    fn next(&mut self) -> Option<Self::Item> {
        let uuid = self.next_uuid.take()?;
        if !self.visited.insert(uuid) {
            return None; // cycle detected
        }
        let entry = self.transcript.get(uuid)?;
        self.next_uuid = entry.parent_uuid();
        Some(entry)
    }
}

#[cfg(test)]
mod tests;
