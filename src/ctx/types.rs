use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct AnatomyStore {
    #[serde(default)]
    pub scanned: String,
    pub files: Vec<FileEntry>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct FileEntry {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub tokens: u64,
    #[serde(default)]
    pub reads: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_read: Option<String>,
    pub last_scanned: String,
    /// Truncated sha256 of the content. Drives change and rename detection,
    /// and keys the global enrichment cache.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Modification time in ms since the epoch. With `size`, lets a rescan
    /// reuse the stored sha without reading the file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtime_ms: Option<u64>,
    /// Deterministic outline: `L<line> <kind> <name>` per top-level symbol.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outline: Vec<String>,
    /// Who wrote `description`. Model enrichment never overwrites `Agent`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desc_source: Option<DescSource>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum DescSource {
    Heuristic,
    Agent,
    Model,
}

#[derive(Serialize, Deserialize)]
pub struct LearningStore {
    pub learnings: Vec<Learning>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Learning {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub category: LearningCategory,
    pub created: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum LearningCategory {
    #[default]
    Preference,
    Correction,
    Bug,
}

impl LearningCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Preference => "preference",
            Self::Correction => "correction",
            Self::Bug => "bug",
        }
    }
}

impl std::str::FromStr for LearningCategory {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "preference" => Ok(Self::Preference),
            "correction" => Ok(Self::Correction),
            "bug" => Ok(Self::Bug),
            _ => Err(format!("unknown category: {s}")),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct BugStore {
    pub bugs: Vec<BugEntry>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BugEntry {
    pub id: String,
    pub symptom: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    pub created: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<String>,
}

// ── Correction ledger ──

#[derive(Serialize, Deserialize)]
pub struct CorrectionStore {
    pub corrections: Vec<Correction>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Correction {
    pub id: String,
    pub mistake: String,
    pub correction: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    pub created: String,
}

// ── Status summary (agent-first: one call gets everything) ──

#[derive(Serialize)]
pub struct CtxStatus {
    pub total_files: usize,
    pub total_tokens: u64,
    pub total_reads: u64,
    pub learnings_count: usize,
    pub bugs_open: usize,
    pub bugs_resolved: usize,
    pub corrections_count: usize,
    pub last_scan: String,
}
