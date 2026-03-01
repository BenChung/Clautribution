use anyhow::{Context, Result};
use crate::decision::{decide_stop, StopContext, StopDecision};
use crate::metadata::{ContinuationBreadcrumb, PlanContext, PlanSnapshot, PromptMetadata};
use crate::preferences::{CommitTemplate, Preferences};
use crate::transcript::{Transcript, Verbosity};
use serde::de::DeserializeOwned;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use crate::types::{
    HookOutput, SessionEndInput, SessionStartInput, SessionStartSource, StopInput,
    UserPromptSubmitInput,
};


/// Read and deserialize a JSON file, returning `None` if it doesn't exist.
fn read_json_file<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match fs::read_to_string(path) {
        Ok(s) => {
            let val = serde_json::from_str(&s)
                .with_context(|| format!("parsing {}", path.display()))?;
            Ok(Some(val))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

/// Remove a file, ignoring "not found" errors.
fn remove_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("removing {}", path.display())),
    }
}

fn hint(message: String) -> Option<HookOutput> {
    Some(HookOutput {
        system_message: Some(message),
        ..Default::default()
    })
}

/// Detect whether a UserPromptSubmit prompt is a `/preview` skill invocation.
fn is_preview_command(prompt: &str) -> bool {
    let p = prompt.trim();
    p == "/preview" || p == "/claudtributter:preview"
}

/// Detect whether a UserPromptSubmit prompt is a `/drop` skill invocation.
fn is_drop_command(prompt: &str) -> bool {
    let p = prompt.trim();
    p == "/drop" || p == "/claudtributter:drop"
}

pub fn read_transcript(path: &str) -> Result<Transcript> {
    let contents = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Transcript::empty()),
        Err(e) => return Err(e).with_context(|| format!("reading transcript {path}")),
    };
    let (transcript, errors) = Transcript::parse(&contents);
    for (line, err) in &errors {
        eprintln!("claudtributter: transcript parse error at line {line}: {err}");
    }
    Ok(transcript)
}

/// All the owned data needed to construct a borrowed `StopContext`.
/// Returned by `Session::build_stop_context` so callers can derive a
/// `StopContext` reference without duplicating the gathering logic.
pub struct OwnedStopContext {
    pub transcript: Transcript,
    pub file_metadata: Option<PromptMetadata>,
    pub pending_plan: Option<String>,
    pub plan_context: Option<PlanContext>,
    pub plan_entries: Vec<serde_json::Value>,
    pub session_id: String,
    pub breadcrumb: Option<ContinuationBreadcrumb>,
    pub committed_tail: Option<String>,
    pub has_uncommitted_changes: bool,
    pub commit_template: String,
    pub verbosity: Verbosity,
}

impl OwnedStopContext {
    /// Produce a borrowed `StopContext` referencing this struct's data.
    pub fn as_ref(&self) -> StopContext<'_> {
        StopContext {
            transcript: &self.transcript,
            file_metadata: self.file_metadata.clone(),
            pending_plan: self.pending_plan.clone(),
            plan_context: self.plan_context.clone(),
            plan_entries: self.plan_entries.clone(),
            session_id: &self.session_id,
            breadcrumb: self.breadcrumb.clone(),
            committed_tail: self.committed_tail.clone(),
            has_uncommitted_changes: self.has_uncommitted_changes,
            commit_template: &self.commit_template,
            verbosity: self.verbosity,
        }
    }
}

pub struct Session {
    repo: git2::Repository,
    dir: PathBuf,
    session_id: String,
    pub prefs: Preferences,
}

impl Session {
    /// Open the git repo from `cwd`, ensure `.claudetributer/` exists, load
    /// preferences, and return a `Session` ready for use.
    pub fn open(cwd: &str, session_id: &str) -> Result<Self> {
        let repo = git2::Repository::discover(cwd)
            .with_context(|| format!("finding git repo from {cwd}"))?;
        let workdir = repo
            .workdir()
            .context("git repo is bare, no working directory")?;
        let dir = workdir.join(".claudetributer");
        if !dir.exists() {
            fs::create_dir_all(&dir)
                .with_context(|| format!("creating {}", dir.display()))?;
        }
        let prefs = Preferences::load(&dir)?;
        Ok(Self {
            repo,
            dir,
            session_id: session_id.to_string(),
            prefs,
        })
    }

    // ---------------------------------------------------------------
    // Private path helpers
    // ---------------------------------------------------------------

    fn prompt_path(&self) -> PathBuf {
        self.dir.join(format!("prompt-{}.json", self.session_id))
    }

    fn continuation_path(&self) -> PathBuf {
        self.dir.join(format!("continuation-{}.json", self.session_id))
    }

    fn drop_marker_path(&self) -> PathBuf {
        self.dir.join(format!("drop-marker-{}.json", self.session_id))
    }

    fn plan_history_path(&self) -> PathBuf {
        self.dir.join(format!("plan-history-{}.json", self.session_id))
    }

    fn pending_plan_path(&self) -> PathBuf {
        self.dir.join(format!("pending-plan-{}.txt", self.session_id))
    }

    /// Project-wide (NOT session-specific) so it survives across the
    /// planning→implementation session boundary.
    fn plan_context_path(&self) -> PathBuf {
        self.dir.join("plan-context.json")
    }

    // ---------------------------------------------------------------
    // Git helpers
    // ---------------------------------------------------------------

    /// Check whether the repo has any uncommitted or untracked changes,
    /// excluding `.claudetributer/` (which is never staged by `commit_changes`).
    fn has_uncommitted_changes(&self) -> Result<bool> {
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true).include_ignored(false);
        let statuses = self.repo.statuses(Some(&mut opts))
            .context("checking git status")?;
        let all_in_metadata = statuses.iter().all(|s| {
            s.path()
                .is_some_and(|p| std::path::Path::new(p).starts_with(".claudetributer"))
        });
        Ok(!statuses.is_empty() && !all_in_metadata)
    }

    /// Stage all changes (including untracked files) except `.claudetributer/`,
    /// commit, and return the new commit OID.
    fn commit_changes(&self, message: &str) -> Result<git2::Oid> {
        let mut index = self.repo.index().context("opening index")?;
        index
            .add_all(
                ["*"].iter(),
                git2::IndexAddOption::DEFAULT,
                Some(&mut |path: &std::path::Path, _matched: &[u8]| {
                    if path.starts_with(".claudetributer") {
                        1 // skip
                    } else {
                        0 // add
                    }
                }),
            )
            .context("staging changes")?;
        index.write().context("writing index")?;
        let tree_oid = index.write_tree().context("writing tree")?;
        let tree = self.repo.find_tree(tree_oid).context("finding tree")?;
        let sig = self.repo
            .signature()
            .context("reading git signature (user.name / user.email)")?;
        let parent = self.repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        let oid = self.repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .context("creating commit")?;
        Ok(oid)
    }

    /// Return the OID of the current HEAD commit, if one exists.
    fn head_oid(&self) -> Option<git2::Oid> {
        self.repo
            .head()
            .ok()
            .and_then(|h| h.peel_to_commit().ok())
            .map(|c| c.id())
    }

    /// Read a plain-text git note from `ref_name` on the given commit OID.
    /// Returns `None` if no note exists.
    fn read_note(&self, ref_name: &str, oid: git2::Oid) -> Option<String> {
        self.repo
            .find_note(Some(ref_name), oid)
            .ok()
            .and_then(|note| note.message().map(|s| s.trim().to_string()))
    }

    /// Write a set of per-category git notes on a commit.
    fn write_notes(&self, oid: git2::Oid, notes: &[(&str, &str)]) -> Result<()> {
        let sig = self.repo.signature().context("reading git signature")?;
        for (ref_name, content) in notes {
            self.repo
                .note(&sig, &sig, Some(ref_name), oid, content, true)
                .with_context(|| format!("writing note to {ref_name}"))?;
        }
        Ok(())
    }

    /// Check whether `.claudetributer` is covered by the repo's ignore rules.
    fn is_data_dir_ignored(&self) -> bool {
        self.repo
            .is_path_ignored(std::path::Path::new(".claudetributer"))
            .unwrap_or(false)
    }

    // ---------------------------------------------------------------
    // Prompt metadata
    // ---------------------------------------------------------------

    /// Read the prompt metadata file for this session.
    /// Returns `None` if the file does not exist.
    fn read_prompt_metadata(&self) -> Result<Option<PromptMetadata>> {
        read_json_file(&self.prompt_path())
    }

    /// Write the prompt metadata file for this session from a `UserPromptSubmit` event.
    fn write_prompt_metadata(
        &self,
        input: &UserPromptSubmitInput,
        transcript: &Transcript,
    ) -> Result<()> {
        let path = self.prompt_path();
        let meta = PromptMetadata {
            prompt: input.prompt.clone(),
            session_id: self.session_id.clone(),
            uuid: transcript.find_user_prompt(&input.prompt).map(String::from),
        };
        let json = serde_json::to_string_pretty(&meta).context("serializing prompt metadata")?;
        fs::write(&path, json).with_context(|| format!("writing {}", path.display()))
    }

    /// Delete the prompt metadata file for this session if it exists.
    fn clear_prompt_metadata(&self) -> Result<()> {
        remove_if_exists(&self.prompt_path())
    }

    // ---------------------------------------------------------------
    // Continuation breadcrumb
    // ---------------------------------------------------------------

    /// Read the continuation breadcrumb for this session.
    /// Returns `None` if the file does not exist.
    fn read_breadcrumb(&self) -> Result<Option<ContinuationBreadcrumb>> {
        read_json_file(&self.continuation_path())
    }

    /// Write the continuation breadcrumb for this session.
    fn write_breadcrumb(&self, breadcrumb: &ContinuationBreadcrumb) -> Result<()> {
        let path = self.continuation_path();
        let json = serde_json::to_string_pretty(breadcrumb).context("serializing breadcrumb")?;
        fs::write(&path, json).with_context(|| format!("writing {}", path.display()))
    }

    /// Delete the continuation breadcrumb for this session if it exists.
    fn clear_breadcrumb(&self) -> Result<()> {
        remove_if_exists(&self.continuation_path())
    }

    // ---------------------------------------------------------------
    // Drop marker (antibreadcrumb)
    // ---------------------------------------------------------------

    /// Read the drop marker — a transcript tail UUID written by `/drop`
    /// that acts as a synthetic `committed_tail`.  Cleared on productive
    /// commit.
    fn read_drop_marker(&self) -> Result<Option<String>> {
        let path = self.drop_marker_path();
        match fs::read_to_string(&path) {
            Ok(s) => Ok(Some(s.trim().to_string())),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    pub fn write_drop_marker(&self, tail_uuid: &str) -> Result<()> {
        let path = self.drop_marker_path();
        fs::write(&path, tail_uuid).with_context(|| format!("writing {}", path.display()))
    }

    fn clear_drop_marker(&self) -> Result<()> {
        remove_if_exists(&self.drop_marker_path())
    }

    // ---------------------------------------------------------------
    // Plan history
    // ---------------------------------------------------------------

    /// Append a plan snapshot (prompt + plan text) to this session's
    /// plan history file, creating it if it doesn't exist.
    fn append_plan_snapshot(&self, prompt: &str, plan: &str) -> Result<()> {
        let path = self.plan_history_path();
        let mut snapshots: Vec<PlanSnapshot> =
            read_json_file(&path)?.unwrap_or_default();
        snapshots.push(PlanSnapshot {
            prompt: prompt.to_string(),
            plan: plan.to_string(),
        });
        let json =
            serde_json::to_string_pretty(&snapshots).context("serializing plan history")?;
        fs::write(&path, json).with_context(|| format!("writing {}", path.display()))
    }

    /// Write the plan text to `pending-plan.txt` for pickup by the next
    /// productive stop, overwriting any previous value.
    fn write_pending_plan(&self, plan: &str) -> Result<()> {
        let path = self.pending_plan_path();
        fs::write(&path, plan).with_context(|| format!("writing {}", path.display()))
    }

    /// Read `pending-plan.txt` without removing it. Returns `None` if absent.
    fn read_pending_plan(&self) -> Result<Option<String>> {
        let path = self.pending_plan_path();
        match fs::read_to_string(&path) {
            Ok(plan) => Ok(Some(plan)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    fn read_plan_context(&self) -> Result<Option<PlanContext>> {
        read_json_file(&self.plan_context_path())
    }

    fn write_plan_context(&self, ctx: &PlanContext) -> Result<()> {
        let path = self.plan_context_path();
        let json = serde_json::to_string_pretty(ctx).context("serializing plan context")?;
        fs::write(&path, json).with_context(|| format!("writing {}", path.display()))
    }

    fn clear_plan_context(&self) -> Result<()> {
        remove_if_exists(&self.plan_context_path())
    }

    /// Read all raw transcript entries from a previous planning session's
    /// JSONL file.  `current_transcript_path` is used to locate the Claude
    /// project directory; the planning session file is
    /// `{dir}/{planning_session_id}.jsonl`.  Returns an empty vec if the
    /// file cannot be found or parsed.
    fn read_planning_session_entries(
        &self,
        current_transcript_path: &str,
        planning_session_id: &str,
    ) -> Result<Vec<serde_json::Value>> {
        let dir = match std::path::Path::new(current_transcript_path).parent() {
            Some(d) => d,
            None => return Ok(vec![]),
        };
        let path = dir.join(format!("{planning_session_id}.jsonl"));
        let path_str = match path.to_str() {
            Some(s) => s,
            None => return Ok(vec![]),
        };
        let transcript = read_transcript(path_str)?;
        Ok(match transcript.tail() {
            Some(tail) => transcript.turn_raw(tail, None),
            None => vec![],
        })
    }

    fn read_and_clear_pending_plan(&self) -> Result<Option<String>> {
        let path = self.pending_plan_path();
        match fs::read_to_string(&path) {
            Ok(plan) => {
                let _ = fs::remove_file(&path);
                Ok(Some(plan))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    // ---------------------------------------------------------------
    // Commit message template
    // ---------------------------------------------------------------

    /// Resolve the commit message template to a string.
    fn load_commit_template(&self) -> Result<String> {
        match &self.prefs.commit_template {
            CommitTemplate::Inline(s) => Ok(s.clone()),
            CommitTemplate::File(filename) => {
                let path = self.dir.join(filename);
                fs::read_to_string(&path)
                    .with_context(|| format!("reading template {}", path.display()))
            }
        }
    }

    // ---------------------------------------------------------------
    // Cross-session plan context recovery
    // ---------------------------------------------------------------

    /// Scan the Claude project transcript directory (parent of
    /// `transcript_path`) for the most recently modified JSONL file that
    /// belongs to a different session.  If that file contains an
    /// `ExitPlanMode` tool call — the signal that a plan was built and
    /// approved — extract the original user prompt, Q&A, and the planning
    /// session ID and return them as a `PlanContext`.
    ///
    /// Only the session ID is stored; the transcript entries are re-read
    /// from the existing JSONL at commit time rather than copying them into
    /// a separate file.
    ///
    /// This covers the case where the Stop hook never fires for the planning
    /// session because Claude Code transitions to the implementation session
    /// via an immediate /clear when the plan is approved.
    fn recover_plan_context(
        &self,
        transcript_path: &str,
    ) -> Result<Option<crate::metadata::PlanContext>> {
        let transcript_file = std::path::Path::new(transcript_path);
        let dir = match transcript_file.parent() {
            Some(d) => d,
            None => return Ok(None),
        };
        let current_filename = format!("{}.jsonl", self.session_id);

        let mut candidates: Vec<_> = Vec::new();
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                let filename = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                if filename == current_filename {
                    continue;
                }
                if let Ok(meta) = entry.metadata() {
                    if let Ok(modified) = meta.modified() {
                        candidates.push((modified, path));
                    }
                }
            }
        }
        // Most recently modified first.
        candidates.sort_by(|a, b| b.0.cmp(&a.0));

        for (_, path) in candidates.iter().take(3) {
            let path_str = match path.to_str() {
                Some(s) => s,
                None => continue,
            };
            // Derive the planning session ID from the filename (strip ".jsonl").
            let planning_session_id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if planning_session_id.is_empty() {
                continue;
            }
            let transcript = read_transcript(path_str)?;
            let tail = match transcript.tail() {
                Some(t) => t,
                None => continue,
            };
            if transcript.find_exit_plan_mode_plan(tail, None).is_none() {
                continue;
            }
            // Find the original user prompt: prefer the first post-commit
            // user text over the plan-mode trigger (e.g. "do it in plan
            // mode").  The committed tail marks where the last productive
            // commit ended; user texts after that point are the planning
            // discussion.
            let committed_tail = self
                .head_oid()
                .and_then(|oid| self.read_note("refs/notes/tail", oid));
            let user_texts =
                transcript.user_texts_until(tail, committed_tail.as_deref());
            let original_prompt = if user_texts.len() >= 2 {
                // Skip the plan-mode trigger (first = most recent); use
                // the chronologically earliest post-commit user text.
                user_texts.last().unwrap().1.to_string()
            } else {
                match user_texts.first() {
                    Some((_, text, _)) if !text.is_empty() => text.to_string(),
                    _ => continue,
                }
            };
            let turn = transcript.turn(tail, None);
            let qa = Transcript::extract_qa(&turn);
            return Ok(Some(crate::metadata::PlanContext {
                original_prompt,
                qa,
                planning_session_id: Some(planning_session_id),
            }));
        }
        Ok(None)
    }

    // ---------------------------------------------------------------
    // Hook handlers
    // ---------------------------------------------------------------

    pub fn handle_session_start(&self, input: &SessionStartInput) -> Result<Option<HookOutput>> {
        let mut warnings: Vec<String> = Vec::new();

        // On resume/clear, clean up this session's stale prompt metadata
        // so tracking starts fresh.  Compact preserves the original prompt
        // metadata so the commit message uses the real user prompt rather
        // than the auto-generated compaction summary.
        if input.source != SessionStartSource::Startup
            && input.source != SessionStartSource::Compact
        {
            self.clear_prompt_metadata()?;
            self.clear_breadcrumb()?;
        }

        // When Claude Code approves a plan via ExitPlanMode it immediately
        // creates a new implementation session via /clear.  The planning
        // session's Stop hook never fires in that transition, so plan-context
        // and plan-entries are never persisted through the normal nonproductive
        // Stop path.  Recover them here from the previous session's JSONL.
        if input.source == SessionStartSource::Clear && self.read_plan_context()?.is_none() {
            if let Some(context) =
                self.recover_plan_context(&input.common.transcript_path)?
            {
                let preview = context
                    .original_prompt
                    .chars()
                    .take(60)
                    .collect::<String>();
                self.write_plan_context(&context)?;
                warnings.push(format!(
                    "recovered plan context from planning session ({preview:?})"
                ));
            }
        }

        if let Ok(head) = self.repo.head() {
            if let Some(branch) = head.shorthand() {
                if self.prefs.warn_branches.iter().any(|b| b == branch) {
                    warnings.push(format!(
                        "on branch `{branch}` — \
                         claudtributter makes frequent commits; consider using a feature branch"
                    ));
                }
            }
        }

        if !self.is_data_dir_ignored() {
            warnings.push(
                ".claudetributer is not in .gitignore — \
                 add it to avoid committing claudtributer metadata"
                    .into(),
            );
        }

        if self.has_uncommitted_changes()? {
            warnings.push(
                "there are uncommitted changes from a previous session — \
                 please commit or discard them before prompting"
                    .into(),
            );
        }

        if warnings.is_empty() {
            Ok(None)
        } else {
            Ok(hint(format!(
                "[claudtributter] warning: {}",
                warnings.join("; ")
            )))
        }
    }

    pub fn handle_user_prompt_submit(
        &self,
        input: &UserPromptSubmitInput,
    ) -> Result<Option<HookOutput>> {
        // Intercept /preview and /drop skill invocations so the output
        // is relayed verbatim via the block reason (skills get paraphrased).
        if is_preview_command(&input.prompt) {
            return self.handle_preview_command(&input.common.transcript_path);
        }
        if is_drop_command(&input.prompt) {
            return self.handle_drop_command(&input.common.transcript_path);
        }

        if self.has_uncommitted_changes()? {
            return Ok(Some(HookOutput {
                decision: Some("block".into()),
                reason: Some(
                    "There are uncommitted changes. Please commit your manual changes \
                     before prompting Claude."
                        .into(),
                ),
                ..Default::default()
            }));
        }

        let transcript = read_transcript(&input.common.transcript_path)?;

        if self.read_prompt_metadata()?.is_some() {
            // A previous prompt was being tracked but never reached a
            // productive Stop (e.g. the user interrupted and reprompted).
            // Write a breadcrumb for transcript continuity, then overwrite
            // the metadata with the new prompt.  Earlier prompts are
            // recovered from the transcript at commit time.
            if let Some(conv_tail) = transcript.conversation_tail() {
                self.write_breadcrumb(&ContinuationBreadcrumb {
                    tail_uuid: conv_tail.to_string(),
                    session_id: self.session_id.clone(),
                })?;
            }
        }

        self.write_prompt_metadata(input, &transcript)?;

        Ok(hint("[claudtributter] tracking prompt".into()))
    }

    /// Handle a `/preview` skill invocation: build the stop context,
    /// run the decision logic, and return the commit message verbatim
    /// as a block reason.
    fn handle_preview_command(&self, transcript_path: &str) -> Result<Option<HookOutput>> {
        let mut owned = self.build_stop_context(transcript_path)?;
        // Force productive path so we always render a commit message,
        // even when there are no uncommitted changes yet.
        owned.has_uncommitted_changes = true;
        let ctx = owned.as_ref();
        let decision = decide_stop(&ctx).map_err(|e| anyhow::anyhow!("{e}"))?;
        let message = match decision {
            StopDecision::NoMetadata => "No prompt metadata — nothing to preview.".to_string(),
            StopDecision::NoTail => "No transcript tail — nothing to preview.".to_string(),
            StopDecision::Productive { commit_message, .. } => commit_message,
            StopDecision::Nonproductive { .. } => "No preview available.".to_string(),
        };
        Ok(Some(HookOutput {
            decision: Some("block".into()),
            reason: Some(message),
            ..Default::default()
        }))
    }

    /// Handle a `/drop` skill invocation: record the current transcript
    /// tail as a drop marker (antibreadcrumb) and clear accumulated state.
    fn handle_drop_command(&self, transcript_path: &str) -> Result<Option<HookOutput>> {
        let transcript = read_transcript(transcript_path)?;
        if let Some(tail) = transcript.conversation_tail() {
            self.write_drop_marker(tail)?;
        }
        self.drop_accumulated()?;
        Ok(Some(HookOutput {
            decision: Some("block".into()),
            reason: Some(
                "Accumulated state dropped. Future commits will start from this point."
                    .into(),
            ),
            ..Default::default()
        }))
    }

    /// Discover the active session ID by scanning for `prompt-*.json`
    /// files in `.claudetributer/`.  Returns `None` if no prompt file
    /// exists.
    pub fn active_session_id(&self) -> Result<Option<String>> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e).context("reading .claudetributer"),
        };
        let mut candidates: Vec<(std::time::SystemTime, String)> = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_str().unwrap_or("");
            if let Some(rest) = name.strip_prefix("prompt-") {
                if let Some(sid) = rest.strip_suffix(".json") {
                    let mtime = entry
                        .metadata()
                        .and_then(|m| m.modified())
                        .unwrap_or(std::time::UNIX_EPOCH);
                    candidates.push((mtime, sid.to_string()));
                }
            }
        }
        candidates.sort_by(|a, b| b.0.cmp(&a.0));
        Ok(candidates.into_iter().next().map(|(_, sid)| sid))
    }

    /// Return the Claude Code projects directory for this repo's workdir,
    /// e.g. `~/.claude/projects/-home-user-myrepo/`.
    fn claude_projects_dir(&self) -> Result<PathBuf> {
        let workdir = self
            .repo
            .workdir()
            .context("bare repo")?
            .canonicalize()
            .context("canonicalize workdir")?;
        let workdir_str = workdir.to_str().context("non-UTF-8 workdir")?;
        // Convention: /foo/bar → -foo-bar
        let mangled = workdir_str.replace('/', "-");
        let home = std::env::var("HOME").context("$HOME not set")?;
        Ok(PathBuf::from(format!("{home}/.claude/projects/{mangled}")))
    }

    /// Discover the most recently modified session transcript (`.jsonl`)
    /// in the Claude Code projects directory.  Returns the session ID
    /// and full transcript path.
    pub fn active_transcript(&self) -> Result<Option<(String, String)>> {
        let dir = self.claude_projects_dir()?;
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e).context("reading claude projects dir"),
        };
        let mut candidates: Vec<(std::time::SystemTime, String, PathBuf)> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let sid = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if sid.is_empty() {
                continue;
            }
            let mtime = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            candidates.push((mtime, sid, path));
        }
        candidates.sort_by(|a, b| b.0.cmp(&a.0));
        Ok(candidates.into_iter().next().map(|(_, sid, path)| {
            (sid, path.to_str().unwrap_or("").to_string())
        }))
    }

    /// Gather all I/O-derived state needed for `decide_stop` into an
    /// owned struct.  Used by both `handle_stop` (hook path) and the
    /// `preview` subcommand.
    pub fn build_stop_context(&self, transcript_path: &str) -> Result<OwnedStopContext> {
        let transcript = read_transcript(transcript_path)?;
        let plan_context = self.read_plan_context()?;
        let plan_entries = match plan_context
            .as_ref()
            .and_then(|pc| pc.planning_session_id.as_deref())
        {
            Some(sid) => self.read_planning_session_entries(transcript_path, sid)?,
            None => vec![],
        };
        Ok(OwnedStopContext {
            transcript,
            file_metadata: self.read_prompt_metadata()?,
            pending_plan: self.read_pending_plan()?,
            plan_context,
            plan_entries,
            session_id: self.session_id.clone(),
            breadcrumb: self.read_breadcrumb()?,
            committed_tail: self.read_drop_marker()?.or_else(|| {
                self.head_oid()
                    .and_then(|oid| self.read_note("refs/notes/tail", oid))
            }),
            has_uncommitted_changes: self.has_uncommitted_changes()?,
            commit_template: self.load_commit_template()?,
            verbosity: self.prefs.summary_verbosity(),
        })
    }

    /// Clear accumulated nonproductive state (prompt metadata and
    /// continuation breadcrumb), resetting to the state as of the last
    /// commit.
    pub fn drop_accumulated(&self) -> Result<()> {
        self.clear_prompt_metadata()?;
        self.clear_breadcrumb()?;
        Ok(())
    }

    pub fn handle_stop(&self, input: &StopInput) -> Result<Option<HookOutput>> {
        let owned = self.build_stop_context(&input.common.transcript_path)?;
        let ctx = owned.as_ref();

        // --- Decide (pure) ---
        let decision = decide_stop(&ctx).map_err(|e| anyhow::anyhow!("{e}"))?;

        // --- Execute ---
        match decision {
            StopDecision::NoMetadata | StopDecision::NoTail => Ok(None),
            StopDecision::Nonproductive {
                hint_message,
                breadcrumb,
                plan_snapshot,
                pending_plan,
                plan_context,
            } => {
                if let Some((prompt, plan)) = plan_snapshot {
                    self.append_plan_snapshot(&prompt, &plan)?;
                }
                if let Some(plan) = pending_plan {
                    self.write_pending_plan(&plan)?;
                }
                if let Some(pc) = plan_context {
                    self.write_plan_context(&pc)?;
                }
                self.write_breadcrumb(&breadcrumb)?;
                Ok(hint(hint_message))
            }
            StopDecision::Productive {
                hint_message,
                commit_message,
                transcript_note_entries,
                simple_notes,
                consumed_pending_plan,
                consumed_plan_context,
            } => {
                if consumed_pending_plan {
                    self.read_and_clear_pending_plan()?;
                }
                if consumed_plan_context {
                    self.clear_plan_context()?;
                }
                let oid = self.commit_changes(&commit_message)?;
                let json = serde_json::to_string_pretty(&transcript_note_entries)
                    .context("serializing transcript")?;
                let mut notes: Vec<(&str, &str)> = vec![("refs/notes/transcript", &json)];
                notes.extend(
                    simple_notes
                        .iter()
                        .map(|(r, c)| (r.as_str(), c.as_str())),
                );
                self.write_notes(oid, &notes)?;
                self.clear_breadcrumb()?;
                self.clear_drop_marker()?;
                Ok(hint(hint_message))
            }
        }
    }

    pub fn handle_session_end(&self, _input: &SessionEndInput) -> Result<Option<HookOutput>> {
        self.clear_prompt_metadata()?;
        self.clear_breadcrumb()?;
        self.clear_drop_marker()?;
        self.clear_pending_plan()?;
        self.clear_plan_history()?;
        Ok(None)
    }

    // ---------------------------------------------------------------
    // Cleanup helpers
    // ---------------------------------------------------------------

    fn clear_pending_plan(&self) -> Result<()> {
        remove_if_exists(&self.pending_plan_path())
    }

    fn clear_plan_history(&self) -> Result<()> {
        remove_if_exists(&self.plan_history_path())
    }
}
