use anyhow::{Context, Result};
use crate::metadata::{ContinuationBreadcrumb, PlanSnapshot, PromptMetadata};
use minijinja::{context, Environment};
use crate::preferences::{CommitTemplate, Preferences};
use serde::de::DeserializeOwned;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use crate::transcript::Transcript;
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

    fn plan_history_path(&self) -> PathBuf {
        self.dir.join(format!("plan-history-{}.json", self.session_id))
    }

    fn pending_plan_path(&self) -> PathBuf {
        self.dir.join(format!("pending-plan-{}.txt", self.session_id))
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

    /// Render a commit message template with the given prompt text.
    fn render_commit_message(&self, template: &str, prompt: &str) -> Result<String> {
        let env = Environment::new();
        let tmpl = env
            .template_from_str(template)
            .context("parsing commit message template")?;
        tmpl.render(context! { prompt })
            .context("rendering commit message template")
    }

    // ---------------------------------------------------------------
    // Hook handlers
    // ---------------------------------------------------------------

    pub fn handle_session_start(&self, input: &SessionStartInput) -> Result<Option<HookOutput>> {
        let mut warnings: Vec<String> = Vec::new();

        // On resume/clear/compact, clean up this session's stale prompt metadata
        // so tracking starts fresh.
        if input.source != SessionStartSource::Startup {
            self.clear_prompt_metadata()?;
            self.clear_breadcrumb()?;
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

        let is_new = self.read_prompt_metadata()?.is_none();

        // Always write metadata — even if the prompt text is identical to the
        // previous submission, this is a new turn and we need a fresh UUID.
        self.write_prompt_metadata(input, &transcript)?;

        if is_new {
            Ok(hint("[claudtributter] tracking prompt".into()))
        } else {
            Ok(hint("[claudtributter] tracking new prompt".into()))
        }
    }

    pub fn handle_stop(&self, input: &StopInput) -> Result<Option<HookOutput>> {
        let transcript = read_transcript(&input.common.transcript_path)?;

        // Derive prompt metadata. Normally written by UserPromptSubmit, but
        // plan-injected implementation prompts bypass that hook entirely.
        // Fallback chain:
        //   1. Prompt metadata file (written by UserPromptSubmit)
        //   2. Pending plan file (written by a preceding plan-mode nonproductive stop)
        //   3. Last user text in the transcript (covers plan implementation prompts
        //      where Claude Code clears the session before Stop fires for ExitPlanMode)
        let mut meta = if let Some(m) = self.read_prompt_metadata()? {
            m
        } else if let Some(plan) = self.read_pending_plan()? {
            let prompt = plan
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("Implement plan")
                .to_string();
            PromptMetadata {
                prompt,
                session_id: self.session_id.clone(),
                uuid: None,
            }
        } else if let Some((uuid, text, plan_content)) = transcript.last_user_text() {
            // Write the plan to the pending plan file so the productive stop
            // path includes it in the commit message.
            if let Some(plan) = plan_content {
                self.write_pending_plan(plan)?;
            }
            PromptMetadata {
                prompt: text.to_string(),
                session_id: self.session_id.clone(),
                uuid: Some(uuid.to_string()),
            }
        } else {
            return Ok(None);
        };

        // Always re-resolve the UUID from the transcript. At prompt-submit
        // time the new entry may not have been written yet, and if the same
        // prompt text was submitted again we need the *latest* UUID, not the
        // one from a previous turn.
        if let Some(uuid) = transcript.find_user_prompt(&meta.prompt) {
            if meta.uuid.as_deref() != Some(uuid) {
                meta.uuid = Some(uuid.to_string());
                let path = self.prompt_path();
                let json =
                    serde_json::to_string_pretty(&meta).context("serializing prompt metadata")?;
                fs::write(&path, json)
                    .with_context(|| format!("updating {}", path.display()))?;
            }
        }

        let tail_uuid = match transcript.tail() {
            Some(uuid) => uuid,
            None => return Ok(None),
        };

        let head_oid = self.head_oid();

        // --- Reset detection ---
        // Prefer the breadcrumb tail (covers nonproductive gaps); fall back to
        // refs/notes/tail on HEAD (covers the case where no breadcrumb exists yet).
        let mut hints: Vec<String> = Vec::new();
        let breadcrumb = self.read_breadcrumb()?;
        let prev_tail: Option<String> = breadcrumb
            .as_ref()
            .map(|b| b.tail_uuid.clone())
            .or_else(|| head_oid.and_then(|oid| self.read_note("refs/notes/tail", oid)));

        if let Some(ref pt) = prev_tail {
            if transcript.uuid_exists(pt) && !transcript.is_ancestor(tail_uuid, pt) {
                hints.push("reset detected (conversation branched from earlier point)".into());
            }
        }

        // --- Nonproductive stop: leave a breadcrumb and return ---
        if !self.has_uncommitted_changes()? {
            // Capture a plan snapshot if this turn finalized a plan via
            // ExitPlanMode. We check unconditionally because the permission
            // mode in the Stop event may have already transitioned away from
            // Plan by the time the stop fires.
            if let Some(plan) =
                transcript.find_exit_plan_mode_plan(tail_uuid, meta.uuid.as_deref())
            {
                self.append_plan_snapshot(&meta.prompt, &plan)?;
                self.write_pending_plan(&plan)?;
                hints.push("plan snapshot saved".into());
            }

            self.write_breadcrumb(&ContinuationBreadcrumb {
                tail_uuid: tail_uuid.to_string(),
                session_id: meta.session_id.clone(),
            })?;
            let mut msg = "[claudtributter] nonproductive turn recorded".to_string();
            if !hints.is_empty() {
                msg = format!(
                    "[claudtributter] {}, nonproductive turn recorded",
                    hints.join(", ")
                );
            }
            return Ok(hint(msg));
        }

        // --- Productive stop: commit, expand transcript, write notes, clear breadcrumb ---

        // The turn summary (for the commit message body) covers only the current
        // prompt→tail span.
        let turn = transcript.turn(tail_uuid, meta.uuid.as_deref());
        let turn_summary = Transcript::summarize_turn(&turn, self.prefs.summary_verbosity());

        // The transcript note covers the full span since the last committed tail.
        let committed_tail = head_oid.and_then(|oid| self.read_note("refs/notes/tail", oid));
        let chain_values = transcript.turn_raw(tail_uuid, committed_tail.as_deref());

        let tmpl = self.load_commit_template()?;
        let mut msg = self.render_commit_message(&tmpl, &meta.prompt)?;
        if let Some(plan) = self.read_and_clear_pending_plan()? {
            msg.push_str("\n\n## Plan\n\n");
            msg.push_str(&plan);
        }
        if let Some(summary) = &turn_summary {
            msg.push_str("\n\n");
            msg.push_str(summary);
        }
        let commit_oid = self.commit_changes(&msg)?;
        hints.push("committed changes".into());

        let transcript_json =
            serde_json::to_string_pretty(&chain_values).context("serializing transcript")?;

        self.write_notes(
            commit_oid,
            &[
                ("refs/notes/transcript", &transcript_json),
                ("refs/notes/prompt", &meta.prompt),
                ("refs/notes/session", &meta.session_id),
                ("refs/notes/tail", tail_uuid),
            ],
        )?;

        self.clear_breadcrumb()?;

        hints.push(format!("attached notes ({} transcript entries)", chain_values.len()));
        Ok(hint(format!("[claudtributter] {}", hints.join(", "))))
    }

    pub fn handle_session_end(&self, _input: &SessionEndInput) -> Result<Option<HookOutput>> {
        self.clear_prompt_metadata()?;
        self.clear_breadcrumb()?;
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
