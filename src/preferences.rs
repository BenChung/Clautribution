use crate::transcript::Verbosity;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

const FILENAME: &str = "claudtributter.toml";

const DEFAULT_WARN_BRANCHES: &[&str] = &[
    "main", "master", "develop", "dev", "staging", "production", "prod", "release", "trunk",
];

/// Commit message template: either an inline Jinja2 string or a path to a
/// template file (relative to `.claudetributer/`).
///
/// In TOML this looks like one of:
///
/// ```toml
/// [commit_template]
/// inline = "{{ prompt }}"
///
/// # — or —
///
/// [commit_template]
/// file = "prompt.tmpl"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CommitTemplate {
    /// An inline Jinja2 template string.
    Inline(String),
    /// Path to a template file (relative to `.claudetributer/`).
    File(String),
}

impl Default for CommitTemplate {
    fn default() -> Self {
        CommitTemplate::Inline("{{ prompt }}".into())
    }
}

/// User-facing preferences stored in `.claudetributer/claudtributter.toml`.
#[derive(Debug, Serialize, Deserialize)]
pub struct Preferences {
    /// Controls how much tool detail appears in commit message summaries.
    /// Options: "short", "medium", "full"
    #[serde(default = "default_summary_verbosity")]
    pub summary_verbosity: String,

    /// Commit message template (inline or file reference).
    #[serde(default)]
    pub commit_template: CommitTemplate,

    /// Branches that trigger a warning when claudtributter is active.
    #[serde(default = "default_warn_branches")]
    pub warn_branches: Vec<String>,
}

fn default_summary_verbosity() -> String {
    "medium".into()
}

fn default_warn_branches() -> Vec<String> {
    DEFAULT_WARN_BRANCHES.iter().map(|s| s.to_string()).collect()
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            summary_verbosity: default_summary_verbosity(),
            commit_template: CommitTemplate::default(),
            warn_branches: default_warn_branches(),
        }
    }
}

impl Preferences {
    /// Load preferences from `.claudetributer/claudtributter.toml`.
    ///
    /// If the file doesn't exist it is created with defaults. Missing keys
    /// in an existing file are filled in with defaults via serde.
    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join(FILENAME);
        match fs::read_to_string(&path) {
            Ok(contents) => {
                let prefs: Preferences = toml::from_str(&contents)
                    .with_context(|| format!("parsing {}", path.display()))?;
                Ok(prefs)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let prefs = Preferences::default();
                let toml_str = toml::to_string_pretty(&prefs)
                    .context("serializing default preferences")?;
                fs::write(&path, &toml_str)
                    .with_context(|| format!("writing default {}", path.display()))?;
                Ok(prefs)
            }
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    pub fn summary_verbosity(&self) -> Verbosity {
        match self.summary_verbosity.as_str() {
            "short" => Verbosity::Short,
            "full" => Verbosity::Full,
            _ => Verbosity::Medium,
        }
    }
}
