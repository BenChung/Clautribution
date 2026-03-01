use crate::metadata::{ContinuationBreadcrumb, PlanContext, PromptMetadata};
use crate::transcript::{Transcript, Verbosity};
use minijinja::{context, Environment};
use std::fmt;

// ===================================================================
// Input: all I/O-derived state, gathered by Session before calling decide_stop()
// ===================================================================

pub struct StopContext<'a> {
    pub transcript: &'a Transcript,
    pub file_metadata: Option<PromptMetadata>,
    pub pending_plan: Option<String>,
    /// Cross-session plan context (original prompt + Q&A) from a previous
    /// planning session.  Read from the project-wide `plan-context.json`.
    pub plan_context: Option<PlanContext>,
    /// Raw transcript entries recovered from a preceding planning session whose
    /// Stop hook never fired (e.g. ExitPlanMode approval).  Read from the
    /// project-wide `plan-entries.json`.  Prepended to the transcript note so
    /// the planning conversation is visible in the commit.
    pub plan_entries: Vec<serde_json::Value>,
    pub session_id: &'a str,
    pub breadcrumb: Option<ContinuationBreadcrumb>,
    /// The value of refs/notes/tail on HEAD (if any).
    pub committed_tail: Option<String>,
    pub has_uncommitted_changes: bool,
    /// Pre-resolved commit message template string.
    pub commit_template: &'a str,
    pub verbosity: Verbosity,
}

// ===================================================================
// Output: what handle_stop() should do
// ===================================================================

pub enum StopDecision {
    /// No prompt metadata could be resolved from any source.
    NoMetadata,
    /// Transcript has no tail entry.
    NoTail,
    /// Nonproductive stop: no uncommitted changes.
    Nonproductive {
        hint_message: String,
        breadcrumb: ContinuationBreadcrumb,
        plan_snapshot: Option<(String, String)>,
        pending_plan: Option<String>,
        /// Cross-session plan context to persist (original prompt + Q&A).
        /// Written to `plan-context.json` so it survives SessionEnd.
        plan_context: Option<PlanContext>,
    },
    /// Productive stop: uncommitted changes to commit.
    Productive {
        hint_message: String,
        commit_message: String,
        transcript_note_entries: Vec<serde_json::Value>,
        /// (ref_name, content) pairs for prompt/session/tail notes.
        simple_notes: Vec<(String, String)>,
        consumed_pending_plan: bool,
        consumed_plan_context: bool,
    },
}

// ===================================================================
// Error: only template rendering can fail in pure code
// ===================================================================

#[derive(Debug)]
pub enum DecisionError {
    TemplateRender(String),
}

impl fmt::Display for DecisionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecisionError::TemplateRender(msg) => write!(f, "template render error: {msg}"),
        }
    }
}

// ===================================================================
// Internal: resolved metadata from the 3-source fallback
// ===================================================================

struct ResolvedMetadata {
    prompt: String,
    session_id: String,
    uuid: Option<String>,
    /// If the metadata came from the transcript fallback and contained
    /// plan_content, carry it here instead of writing to disk mid-decision.
    pending_plan_from_fallback: Option<String>,
}

// ===================================================================
// Pure entry point
// ===================================================================

pub fn decide_stop(ctx: &StopContext) -> Result<StopDecision, DecisionError> {
    // 1. Resolve prompt metadata via 3-source fallback.
    let resolved = match resolve_metadata(ctx) {
        Some(r) => r,
        None => return Ok(StopDecision::NoMetadata),
    };

    let prompt = resolved.prompt;
    let session_id = resolved.session_id;
    let mut uuid = resolved.uuid;

    // Always re-resolve the UUID from the transcript — at prompt-submit
    // time the new entry may not have been written yet, and if the same
    // prompt text was submitted again we need the *latest* UUID.
    if let Some(found) = ctx.transcript.find_user_prompt(&prompt) {
        if uuid.as_deref() != Some(found) {
            uuid = Some(found.to_string());
        }
    }

    // 2. Get the transcript tail.
    let tail_uuid = match ctx.transcript.tail() {
        Some(t) => t,
        None => return Ok(StopDecision::NoTail),
    };

    // For storage (breadcrumbs, git notes) use the last conversation
    // entry's UUID rather than the raw tail. Progress and system entries
    // sit on side branches of the DAG.
    let conv_tail = ctx.transcript.conversation_tail().unwrap_or(tail_uuid);

    // 3. Reset detection.
    let mut hints = detect_reset(ctx, tail_uuid);

    // 4. Branch: nonproductive vs productive.
    if !ctx.has_uncommitted_changes {
        return Ok(build_nonproductive(
            ctx,
            tail_uuid,
            conv_tail,
            &prompt,
            &session_id,
            uuid.as_deref(),
            &mut hints,
            resolved.pending_plan_from_fallback,
        ));
    }

    build_productive(
        ctx,
        tail_uuid,
        conv_tail,
        &prompt,
        &session_id,
        uuid.as_deref(),
        &mut hints,
        resolved.pending_plan_from_fallback,
    )
}

// ===================================================================
// Metadata resolution: 3-source fallback
// ===================================================================

/// Extract a concise prompt from plan content.  Strips leading markdown
/// heading markers (`# `) and returns the first non-empty line, falling
/// back to "Implement plan".
fn plan_prompt(plan: &str) -> String {
    plan.lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim().trim_start_matches('#').trim().to_string())
        .unwrap_or_else(|| "Implement plan".to_string())
}

/// Prompts above this byte threshold are too large for a commit
/// message.  The full text is moved to a `refs/notes/prompt-full`
/// git note and the commit message uses a short summary instead.
const PROMPT_SIZE_LIMIT: usize = 4096;

/// If `prompt` exceeds the size limit, return a short summary for the
/// commit message and the full text for a separate git note.
fn split_long_prompt(prompt: &str) -> (String, Option<String>) {
    if prompt.len() <= PROMPT_SIZE_LIMIT {
        return (prompt.to_string(), None);
    }
    let first_line = prompt.lines().next().unwrap_or(prompt).trim();
    let summary = if first_line.len() > 200 {
        // Find the last char boundary at or before 200 bytes.
        let mut end = 200;
        while end > 0 && !first_line.is_char_boundary(end) {
            end -= 1;
        }
        format!(
            "{}... [full prompt in refs/notes/prompt-full]",
            &first_line[..end]
        )
    } else {
        format!(
            "{first_line} [full prompt in refs/notes/prompt-full]"
        )
    };
    (summary, Some(prompt.to_string()))
}

fn resolve_metadata(ctx: &StopContext) -> Option<ResolvedMetadata> {
    // Source 1: prompt metadata file (written by UserPromptSubmit).
    if let Some(m) = &ctx.file_metadata {
        return Some(ResolvedMetadata {
            prompt: m.prompt.clone(),
            session_id: m.session_id.clone(),
            uuid: m.uuid.clone(),
            pending_plan_from_fallback: None,
        });
    }

    // Source 2: pending plan file (written by a preceding plan-mode nonproductive stop).
    if let Some(plan) = &ctx.pending_plan {
        return Some(ResolvedMetadata {
            prompt: plan_prompt(plan),
            session_id: ctx.session_id.to_string(),
            uuid: None,
            pending_plan_from_fallback: None,
        });
    }

    // Source 3: last user text in the transcript.
    if let Some((uuid, text, plan_content)) = ctx.transcript.last_user_text() {
        // If this entry is at or before the committed tail, it's already
        // been committed (or dropped) — treat as no metadata.
        if let Some(ct) = ctx.committed_tail.as_deref() {
            if uuid == ct || ctx.transcript.is_ancestor(ct, uuid) {
                return None;
            }
        }
        // When planContent is present the user text is Claude Code's
        // auto-injected scaffolding ("Implement the following plan: ...").
        // Use a concise title derived from the plan content instead.
        let prompt = match plan_content {
            Some(plan) => plan_prompt(plan),
            None => text.to_string(),
        };
        return Some(ResolvedMetadata {
            prompt,
            session_id: ctx.session_id.to_string(),
            uuid: Some(uuid.to_string()),
            pending_plan_from_fallback: plan_content.map(String::from),
        });
    }

    None
}

// ===================================================================
// Reset detection (public for standalone testing)
// ===================================================================

/// Check whether the current tail represents a reset (conversation branched
/// from an earlier point). Returns a vec of hint strings (empty = no reset).
pub fn detect_reset(ctx: &StopContext, tail_uuid: &str) -> Vec<String> {
    let mut hints = Vec::new();

    // Prefer the breadcrumb tail (covers nonproductive gaps); fall back to
    // refs/notes/tail on HEAD (covers the case where no breadcrumb exists yet).
    let prev_tail: Option<&str> = ctx
        .breadcrumb
        .as_ref()
        .map(|b| b.tail_uuid.as_str())
        .or(ctx.committed_tail.as_deref());

    if let Some(pt) = prev_tail {
        if ctx.transcript.uuid_exists(pt) && !ctx.transcript.is_ancestor(tail_uuid, pt) {
            hints.push("reset detected (conversation branched from earlier point)".into());
        }
    }

    hints
}

// ===================================================================
// Nonproductive path
// ===================================================================

fn build_nonproductive(
    ctx: &StopContext,
    tail_uuid: &str,
    conv_tail: &str,
    prompt: &str,
    session_id: &str,
    prompt_uuid: Option<&str>,
    hints: &mut Vec<String>,
    pending_plan_from_fallback: Option<String>,
) -> StopDecision {
    // Check for ExitPlanMode plan snapshot.
    let plan_snapshot =
        ctx.transcript
            .find_exit_plan_mode_plan(tail_uuid, prompt_uuid)
            .map(|plan| {
                hints.push("plan snapshot saved".into());
                (prompt.to_string(), plan.clone())
            });

    // If we found a plan snapshot, that plan also becomes the pending plan.
    // If the metadata came from the transcript fallback with plan_content,
    // carry that through as well.
    let pending_plan = plan_snapshot
        .as_ref()
        .map(|(_, plan)| plan.clone())
        .or(pending_plan_from_fallback);

    // Build cross-session plan context when a plan was finalized.
    // This captures the original user prompt and any Q&A interactions
    // from the planning turn so they survive the session boundary.
    let plan_context = if plan_snapshot.is_some() {
        let turn = ctx.transcript.turn(tail_uuid, prompt_uuid);
        let qa = Transcript::extract_qa(&turn);
        Some(PlanContext {
            original_prompt: prompt.to_string(),
            qa,
            planning_session_id: None,
        })
    } else {
        None
    };

    let breadcrumb = ContinuationBreadcrumb {
        tail_uuid: conv_tail.to_string(),
        session_id: session_id.to_string(),
    };

    let hint_message = if hints.is_empty() {
        "[claudtributter] nonproductive turn recorded".to_string()
    } else {
        format!(
            "[claudtributter] {}, nonproductive turn recorded",
            hints.join(", ")
        )
    };

    StopDecision::Nonproductive {
        hint_message,
        breadcrumb,
        plan_snapshot,
        pending_plan,
        plan_context,
    }
}

// ===================================================================
// Productive path
// ===================================================================

fn build_productive(
    ctx: &StopContext,
    tail_uuid: &str,
    conv_tail: &str,
    prompt: &str,
    session_id: &str,
    _prompt_uuid: Option<&str>,
    hints: &mut Vec<String>,
    pending_plan_from_fallback: Option<String>,
) -> Result<StopDecision, DecisionError> {
    // Transcript note: planning session entries (if recovered) followed by
    // the full implementation span since the last committed tail.
    let impl_entries = ctx
        .transcript
        .turn_raw(tail_uuid, ctx.committed_tail.as_deref());
    let chain_values = if !ctx.plan_entries.is_empty() {
        let mut all = ctx.plan_entries.clone();
        all.extend(impl_entries);
        all
    } else {
        impl_entries
    };

    // Full implementation span (committed_tail→tail) — used for Q&A
    // extraction and the turn summary.  The wider span ensures we capture
    // content from intervening nonproductive turns and interrupted prompts.
    let impl_turn = ctx
        .transcript
        .turn(tail_uuid, ctx.committed_tail.as_deref());

    // Turn summary covers the full committed_tail→tail span so interrupted
    // prompts and their partial responses appear naturally in the flow.
    let turn_summary = Transcript::summarize_turn(&impl_turn, ctx.verbosity);

    // If a cross-session plan context exists, prefer its original prompt
    // over the plan-title fallback — it's the user's actual words.
    let effective_prompt = ctx
        .plan_context
        .as_ref()
        .map(|pc| pc.original_prompt.as_str())
        .unwrap_or(prompt);

    // Split out pasted content (large prompts) into a separate note.
    let (commit_prompt, full_prompt) = split_long_prompt(effective_prompt);

    // Render commit message.
    let mut msg = render_commit_message(ctx.commit_template, &commit_prompt)?;

    // Determine whether to consume the pending plan (either from ctx or fallback).
    let has_pending_plan = ctx.pending_plan.is_some() || pending_plan_from_fallback.is_some();
    let plan_text = ctx
        .pending_plan
        .as_deref()
        .or(pending_plan_from_fallback.as_deref());
    let qa: Vec<String> = ctx
        .plan_context
        .as_ref()
        .filter(|pc| !pc.qa.is_empty())
        .map(|pc| pc.qa.clone())
        .unwrap_or_else(|| Transcript::extract_qa(&impl_turn));

    // Collect earlier user prompts for the git notes (refs/notes/prompt).
    let all_user_texts = ctx
        .transcript
        .user_texts_until(tail_uuid, ctx.committed_tail.as_deref());
    let earlier_prompts: Vec<&str> = all_user_texts
        .iter()
        .filter(|(_, text, plan_content)| {
            plan_content.is_none() && *text != prompt && *text != effective_prompt
        })
        .map(|(_, text, _)| *text)
        .rev()
        .collect();
    if !qa.is_empty() {
        msg.push_str("\n\n## Q&A\n\n");
        for line in &qa {
            msg.push_str(line);
            msg.push('\n');
        }
    }
    if let Some(plan) = plan_text {
        msg.push_str("\n\n## Plan\n\n");
        msg.push_str(plan);
    }
    if let Some(summary) = &turn_summary {
        msg.push_str("\n\n");
        msg.push_str(summary);
    }

    hints.push("committed changes".into());
    hints.push(format!(
        "attached notes ({} transcript entries)",
        chain_values.len()
    ));

    let prompt_note = if earlier_prompts.is_empty() {
        commit_prompt
    } else {
        let mut note = String::new();
        for p in &earlier_prompts {
            note.push_str(p);
            note.push_str("\n---\n");
        }
        note.push_str(&commit_prompt);
        note
    };
    let mut simple_notes = vec![
        ("refs/notes/prompt".to_string(), prompt_note),
        ("refs/notes/session".to_string(), session_id.to_string()),
        ("refs/notes/tail".to_string(), conv_tail.to_string()),
    ];
    if let Some(full) = full_prompt {
        simple_notes.push(("refs/notes/prompt-full".to_string(), full));
    }

    Ok(StopDecision::Productive {
        hint_message: format!("[claudtributter] {}", hints.join(", ")),
        commit_message: msg,
        transcript_note_entries: chain_values,
        simple_notes,
        consumed_pending_plan: has_pending_plan,
        consumed_plan_context: ctx.plan_context.is_some(),
    })
}

// ===================================================================
// Template rendering (pure computation)
// ===================================================================

fn render_commit_message(template: &str, prompt: &str) -> Result<String, DecisionError> {
    let env = Environment::new();
    let tmpl = env
        .template_from_str(template)
        .map_err(|e| DecisionError::TemplateRender(format!("parsing template: {e}")))?;
    tmpl.render(context! { prompt })
        .map_err(|e| DecisionError::TemplateRender(format!("rendering template: {e}")))
}

#[cfg(test)]
mod tests;
