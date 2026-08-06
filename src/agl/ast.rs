//! AST models for the Agent Graph Language (`.agl`) DSL.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AglSpec {
    pub name: String,
    pub inputs: Vec<TypedParam>,
    pub outputs: Vec<TypedParam>,
    /// Dotted `Server.method` tool names this spec's flow depends on (e.g.
    /// `TechnicalSuccessHub.write_page`), declared once so a compiled skill
    /// can render a preflight check instead of failing mid-graph on a
    /// missing tool. Empty for specs written before this field existed —
    /// nothing downstream treats an empty list as an error.
    pub requires: Vec<String>,
    /// Explicit name for the compiled skill/subagent this spec becomes.
    /// `None` means the caller should default to a kebab-cased `name`
    /// (`skill::default_skill_name`) — most specs need this unset.
    pub skill: Option<String>,
    /// Named local JSONL caches this spec's runtime reads and appends to,
    /// outside the compiled skill entirely so `kazam agl load` regenerating
    /// the skill never touches cached data. Zero or more - a spec can
    /// declare its own inline (unique to it) and/or pull in shared ones via
    /// `import` from a fragment (every spec importing that fragment gets
    /// the same named block, so they share one file). A block's `name` is
    /// its file identity: `~/.kazam/agl/cache/<name>.jsonl`. Two blocks
    /// landing on the same name with different fields is a hard error.
    pub cache: Vec<CacheBlock>,
    pub invariants: Vec<InvariantRule>,
    pub flow: Vec<StateNode>,
    pub branches: HashMap<String, BranchBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TypedParam {
    pub name: String,
    pub data_type: DataType,
}

/// `cache <name> { field: type, ... }` - the typed shape of one JSONL file
/// at `~/.kazam/agl/cache/<name>.jsonl`. See `AglSpec::cache`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CacheBlock {
    pub name: String,
    pub fields: Vec<TypedParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DataType {
    String,
    Int,
    Bool,
    List(Box<DataType>),
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum InvariantRule {
    DenyWithoutGate {
        action: String,
        target: String,
        required_gate: String,
    },
    DenyConstraint {
        action: String,
        target: String,
        condition: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StateNode {
    pub name: String,
    pub action: StateAction,
    pub transition: TransitionTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StateAction {
    Call { function: String, args: Vec<ArgRef> },
    Map { function: String, iterable: String },
    Evaluate { expression: String },
    Gate { gate_name: String },
}

/// A `call(...)` argument: either a bare-ident variable reference
/// (`customer`, resolved against the spec's own `in:`/local names at
/// runtime) or a quoted string literal (`"https://..."`, config data with
/// nowhere else to live now that lexer comments never reach the AST — see
/// `.agl` files noting kz-700a). Both round-trip through `render_agl_source`
/// distinctly, unlike a comment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ArgRef {
    Var(String),
    Literal(String),
}

impl ArgRef {
    /// The inner text either way, for callers that only need to pattern
    /// match against it (the invariant checker's word/substring scans) and
    /// don't care whether it came from an ident or a string literal.
    pub fn text(&self) -> &str {
        match self {
            ArgRef::Var(s) => s,
            ArgRef::Literal(s) => s,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TransitionTarget {
    Next,
    Branch(String),
    Goto(String),
    Terminate(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BranchBlock {
    pub state_name: String,
    pub cases: Vec<BranchCase>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BranchCase {
    pub condition: String,
    pub target: TransitionTarget,
}
