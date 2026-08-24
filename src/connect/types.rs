//! Serde types for `.map.yaml` connector mapping files. Deserializes both
//! `connectors/runzero/exposure.map.yaml` and `connectors/maze/investigations.map.yaml`
//! per the verb catalog in `connectors/CONNECT_SPEC.md`.

use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct MappingFile {
    pub mapping: String,
    pub version: String,
    pub source: Source,
    pub output: Output,
    #[serde(default)]
    pub semantics: Vec<String>,
    pub pulls: HashMap<String, Pull>,
    #[serde(default)]
    pub personas: HashMap<String, Persona>,
    #[serde(default)]
    pub shapes: HashMap<String, Shape>,
    #[serde(default)]
    pub sections: HashMap<String, Section>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Source {
    pub spec: String,
    pub api_version: String,
    pub base_url: String,
    pub auth: Auth,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Auth {
    Bearer {
        value: String,
    },
    ApiKey {
        header: String,
        value: String,
    },
    Oauth2 {
        token_url: String,
        client_id: String,
        client_secret: String,
        #[serde(default)]
        scope: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct Output {
    /// curata | terminal | both | file
    pub target: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default)]
    pub folder: String,
}

fn default_mode() -> String {
    "upsert".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct Persona {
    pub question: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Pull {
    pub request: Request,
    #[serde(default)]
    pub paginate: Option<Paginate>,
    pub collect: Collect,
    #[serde(default)]
    pub rate_limit: Option<RateLimit>,
    #[serde(default)]
    pub transforms: Vec<Transform>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    pub method: String,
    pub path: String,
    #[serde(default)]
    pub params: HashMap<String, Value>,
    #[serde(default)]
    pub body: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "style", rename_all = "snake_case")]
pub enum Paginate {
    Keyset {
        next_from: String,
        send_as: String,
    },
    Cursor {
        next_from: String,
        #[serde(rename = "while")]
        r#while: String,
    },
    Offset {
        param: String,
        #[serde(default)]
        start: i64,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Collect {
    Simple(String),
    Conditional { paged: String, bare: String },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "strategy", rename_all = "snake_case")]
pub enum RateLimit {
    RetryAfter {
        #[serde(default)]
        max_retries: u32,
        #[serde(default)]
        backoff: Option<String>,
    },
    FixedDelay {
        delay_ms: u64,
    },
}

/// Per-record transforms, executed once per pulled record before aggregation.
/// Untagged: each verb is discriminated by its own key name (`coerce`,
/// `default`, `rename`, ...) rather than a shared `type` tag, matching the
/// mapping file's flat sibling-key shape (e.g. `{coerce: .x, to: list}`).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Transform {
    Coerce {
        coerce: String,
        to: String,
        #[serde(default)]
        join: Option<String>,
    },
    Default {
        default: String,
        value: Value,
        #[serde(default)]
        when: Option<String>,
    },
    Rename {
        rename: String,
        to: String,
    },
    Lowercase {
        lowercase: String,
    },
    Strip {
        strip: String,
        #[serde(default)]
        prefix: Option<String>,
        #[serde(default)]
        suffix: Option<String>,
        #[serde(default)]
        before_last: Option<String>,
        #[serde(default)]
        after_first: Option<String>,
        #[serde(default)]
        pattern: Option<String>,
    },
    Regex {
        regex: String,
        pattern: String,
        capture: usize,
        into: String,
        #[serde(default)]
        default: Option<Value>,
    },
    Epoch {
        epoch: String,
        to: String,
        #[serde(default)]
        into: Option<String>,
        #[serde(default)]
        zero_means: Option<Value>,
        #[serde(default)]
        unit: Option<String>,
    },
    Flatten {
        flatten: String,
        #[serde(default)]
        separator: Option<String>,
        #[serde(default)]
        prefix: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct Shape {
    pub pull: String,
    /// Not yet interpreted by the runtime - conditional shapes always run.
    /// See `connectors/CONNECT_SPEC.md`'s "Potential future gaps".
    #[serde(default)]
    pub conditional: Option<String>,
    pub aggregate: Vec<AggStep>,
    #[serde(default)]
    pub persona: String,
    #[serde(default)]
    pub decided: bool,
}

/// Aggregation verbs. Untagged over each verb's own key, mirroring `Transform`.
/// `filter`/`bucket`/`tally`/`derive`/`compare`/`rank` nest a substructure one
/// level under their key; `expand`/`take`/`distinct` are flat scalars.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum AggStep {
    Expand { expand: String },
    Filter { filter: FilterCond },
    Bucket { bucket: BucketSpec },
    Tally { tally: TallySpec },
    Derive { derive: DeriveSpec },
    Compare { compare: CompareSpec },
    Rank { rank: RankSpec },
    Take { take: usize },
    Distinct { distinct: String },
}

#[derive(Debug, Clone, Deserialize)]
pub struct FilterCond {
    #[serde(rename = "where")]
    pub r#where: String,
    #[serde(rename = "in", default)]
    pub r#in: Option<Vec<String>>,
    #[serde(default)]
    pub not_in: Option<Vec<String>>,
    #[serde(default)]
    pub is_empty: Option<bool>,
    #[serde(default)]
    pub not_empty: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BucketSpec {
    pub by: Vec<String>,
    #[serde(default)]
    pub ordered: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TallySpec {
    #[serde(default)]
    pub all: Option<bool>,
    #[serde(rename = "where", default)]
    pub r#where: Option<String>,
    #[serde(default)]
    pub not_empty: Option<bool>,
    #[serde(rename = "in", default)]
    pub r#in: Option<Vec<String>>,
    #[serde(default)]
    pub distinct: Option<String>,
    #[serde(rename = "as")]
    pub r#as: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeriveSpec {
    pub name: String,
    pub expr: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompareSpec {
    pub a: String,
    pub b: String,
    pub op: String,
    #[serde(rename = "as")]
    pub r#as: String,
}

/// A single string (`by: asset_count`) or a list (`by: [.a, .b]`) - both
/// forms appear across the two reference mapping files.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum StringOrVec {
    One(String),
    Many(Vec<String>),
}

impl StringOrVec {
    pub fn as_vec(&self) -> Vec<String> {
        match self {
            StringOrVec::One(s) => vec![s.clone()],
            StringOrVec::Many(v) => v.clone(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RankSpec {
    pub by: StringOrVec,
    #[serde(default)]
    pub direction: Option<StringOrVec>,
    #[serde(default)]
    pub order: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Section {
    pub shape: String,
    pub component: String,
    #[serde(default)]
    pub config: Value,
    #[serde(default)]
    pub secondary: Option<SecondarySection>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SecondarySection {
    pub component: String,
    #[serde(default)]
    pub config: Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deser_runzero_mapping() {
        let content = include_str!("../../connectors/runzero/exposure.map.yaml");
        let m: MappingFile = serde_yaml::from_str(content).expect("runzero mapping should parse");
        assert!(!m.pulls.is_empty());
        assert!(!m.shapes.is_empty());
        assert!(!m.sections.is_empty());
        for (name, pull) in &m.pulls {
            assert!(pull.rate_limit.is_some(), "pull '{}' should have rate_limit", name);
        }
    }

    #[test]
    fn deser_maze_mapping() {
        let content = include_str!("../../connectors/maze/investigations.map.yaml");
        let m: MappingFile = serde_yaml::from_str(content).expect("maze mapping should parse");
        assert!(!m.pulls.is_empty());
        assert!(!m.shapes.is_empty());
        assert!(!m.sections.is_empty());
    }
}
