//! AST models for the Agent Graph Language (`.agl`) DSL.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AglSpec {
    pub name: String,
    pub inputs: Vec<TypedParam>,
    pub outputs: Vec<TypedParam>,
    /// One-sentence purpose, used verbatim as the compiled skill's frontmatter
    /// `description:` (what a coding agent matches a request against to pick
    /// this skill). `None` falls back to a generic "Runs the X AGL graph"
    /// placeholder that carries no routing signal — always declare this.
    pub description: Option<String>,
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
    /// Where `kazam agl load`/`kazam agl skill` (with no explicit `-o`) writes
    /// this spec's compiled skill: a directory, absolute or `~`-expanded.
    /// `None` means fall back to `--scope`/`--out` like any other spec. Lets
    /// a spec that's meant to be published into a specific repo (a plugin's
    /// own `skills/` folder, say) declare that once, so `load` puts it there
    /// automatically instead of everything landing in one flat
    /// `.claude/skills/` regardless of where it's actually meant to live. An
    /// explicit CLI `-o`/`--out` still wins over this - the flag is what the
    /// caller typed just now, this is only the spec's own declared default.
    pub publish: Option<String>,
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
    Call {
        function: String,
        args: Vec<ArgRef>,
    },
    Map {
        function: String,
        iterable: String,
    },
    Evaluate {
        expression: String,
    },
    Gate {
        gate_name: String,
    },
    /// `fan(SpecName, iterable)`: run another named spec once per item of
    /// `iterable`, or `fan(SpecName, "5")`: run it up to that many bounded
    /// rounds when there's no pre-existing collection to iterate, just a
    /// cap (a quoted count, same "config data with nowhere else to live"
    /// reasoning as `ArgRef::Literal` on `call()` args). One primitive
    /// covers both composition (call another spec) and bounded looping
    /// (a single-item or counted "iterable") - see
    /// `validator::has_gate_protected_writes`, which treats any spec
    /// containing a `Fan` as gate-protected unconditionally, since the
    /// fanned spec's own gates need the real human in the loop each round.
    Fan {
        spec_name: String,
        iterable: ArgRef,
    },
    /// `watch(CONDITION)`: poll an external condition (CI status, a build
    /// completing) until it resolves. Unlike `gate()`, this isn't waiting
    /// on a human, it's waiting on some other system's state. If the
    /// condition text names a time bound, the compiled skill's executor
    /// stops and reports rather than waiting indefinitely.
    Watch {
        condition: String,
    },
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
