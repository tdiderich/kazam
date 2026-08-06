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
    pub invariants: Vec<InvariantRule>,
    pub flow: Vec<StateNode>,
    pub branches: HashMap<String, BranchBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TypedParam {
    pub name: String,
    pub data_type: DataType,
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
    Call { function: String, args: Vec<String> },
    Map { function: String, iterable: String },
    Evaluate { expression: String },
    Gate { gate_name: String },
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
