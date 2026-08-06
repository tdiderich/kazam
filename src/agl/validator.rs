//! Static graph analyzer for a parsed `.agl` spec: reachability, terminal
//! completeness, branch integrity, and invariant soundness.

use std::collections::{HashMap, HashSet};

use super::ast::{AglSpec, BranchBlock, InvariantRule, StateAction, StateNode, TransitionTarget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    /// Name of the state, branch, or `<spec>` this diagnostic is about.
    pub location: String,
    /// Source line, when known (states carry these; synthetic issues don't).
    pub line: Option<usize>,
}

impl Diagnostic {
    fn error(code: &'static str, message: impl Into<String>, location: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Error,
            code,
            message: message.into(),
            location: location.into(),
            line: None,
        }
    }

    fn warning(
        code: &'static str,
        message: impl Into<String>,
        location: impl Into<String>,
    ) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            code,
            message: message.into(),
            location: location.into(),
            line: None,
        }
    }

    fn with_line(mut self, line: Option<usize>) -> Self {
        self.line = line;
        self
    }
}

pub fn has_errors(diags: &[Diagnostic]) -> bool {
    diags.iter().any(|d| d.severity == Severity::Error)
}

pub fn validate(spec: &AglSpec, state_lines: &HashMap<String, usize>) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    if spec.flow.is_empty() {
        diags.push(Diagnostic::error(
            "empty-flow",
            "spec has no states in its flow block",
            "<spec>",
        ));
        return diags;
    }

    let by_name: HashMap<&str, usize> = spec
        .flow
        .iter()
        .enumerate()
        .map(|(i, s)| (s.name.as_str(), i))
        .collect();

    check_duplicate_states(spec, &mut diags, state_lines);
    check_reference_integrity(spec, &by_name, &mut diags, state_lines);

    let initial = spec.flow[0].name.clone();
    let (visited, reachable_branches) =
        reachable_set(&initial, &by_name, &spec.flow, &spec.branches);

    check_unreachable_states(spec, &visited, &initial, &mut diags, state_lines);
    check_cycles(
        &initial,
        &by_name,
        &spec.flow,
        &spec.branches,
        &mut diags,
        state_lines,
    );
    check_branch_integrity(spec, &by_name, &reachable_branches, &mut diags);
    check_invariant_soundness(spec, &initial, &by_name, &mut diags, state_lines);
    check_tool_dependencies(spec, &mut diags, state_lines);

    diags
}

/// Cross-checks the spec's own `requires:` declaration against what the
/// flow actually calls. Only runs when `requires` is non-empty — specs
/// written before this field existed declare nothing and get no warnings,
/// so this is a strictly opt-in check exercised by authoring a `requires:`
/// line, not a flag.
///
/// This consistency matters beyond linting: `kazam agl skill` renders
/// `requires` into a preflight instruction ("confirm these tools are
/// available before executing any state"). An incomplete `requires` list
/// makes that preflight check incomplete too, so catching the gap here is
/// what makes the compiled skill's preflight trustworthy.
fn check_tool_dependencies(
    spec: &AglSpec,
    diags: &mut Vec<Diagnostic>,
    state_lines: &HashMap<String, usize>,
) {
    if spec.requires.is_empty() {
        return;
    }

    let declared: HashSet<String> = spec.requires.iter().map(|s| s.to_lowercase()).collect();
    let mut used: HashSet<String> = HashSet::new();

    for state in &spec.flow {
        let function = match &state.action {
            StateAction::Call { function, .. } => function,
            StateAction::Map { function, .. } => function,
            _ => continue,
        };
        let lower = function.to_lowercase();
        used.insert(lower.clone());
        if !declared.contains(&lower) {
            diags.push(
                Diagnostic::warning(
                    "undeclared-tool-dependency",
                    format!(
                        "state '{}' calls '{function}', which is not listed in the spec's requires: line",
                        state.name
                    ),
                    state.name.clone(),
                )
                .with_line(state_lines.get(&state.name).copied()),
            );
        }
    }

    for tool in &spec.requires {
        if !used.contains(&tool.to_lowercase()) {
            diags.push(Diagnostic::warning(
                "unused-tool-dependency",
                format!("'{tool}' is listed in requires: but no state in the flow calls it"),
                "<spec>",
            ));
        }
    }
}

/// Opt-in tool-name existence check: for every `call(...)`/`map(...)` in the
/// flow, warn if its function string isn't present in `manifest` (an
/// exact/case-insensitive match against a flat list of dotted
/// `Server.method` names). This is deliberately thin — a name-existence
/// check only, not schema validation; the manifest is hand-maintained and
/// has no notion of a server's actual tool/argument schema. Only called
/// when the caller has an explicit `--tools` manifest, so it never changes
/// behavior for callers that don't opt in.
pub fn check_tool_bindings(
    spec: &AglSpec,
    manifest: &HashSet<String>,
    state_lines: &HashMap<String, usize>,
) -> Vec<Diagnostic> {
    let normalized: HashSet<String> = manifest.iter().map(|s| s.to_lowercase()).collect();
    let mut diags = Vec::new();
    for state in &spec.flow {
        let function = match &state.action {
            StateAction::Call { function, .. } => function,
            StateAction::Map { function, .. } => function,
            _ => continue,
        };
        if !normalized.contains(&function.to_lowercase()) {
            diags.push(
                Diagnostic::warning(
                    "undefined-tool-binding",
                    format!(
                        "state '{}' calls '{function}', which is not listed in the tool manifest",
                        state.name
                    ),
                    state.name.clone(),
                )
                .with_line(state_lines.get(&state.name).copied()),
            );
        }
    }
    diags
}

fn check_duplicate_states(
    spec: &AglSpec,
    diags: &mut Vec<Diagnostic>,
    lines: &HashMap<String, usize>,
) {
    let mut seen = HashSet::new();
    for s in &spec.flow {
        if !seen.insert(s.name.as_str()) {
            diags.push(
                Diagnostic::error(
                    "duplicate-state",
                    format!("state '{}' is defined more than once", s.name),
                    s.name.clone(),
                )
                .with_line(lines.get(&s.name).copied()),
            );
        }
    }
}

/// Every transition target and branch-case target must resolve to something
/// real, independent of whether the graph walk ever reaches it.
fn check_reference_integrity(
    spec: &AglSpec,
    by_name: &HashMap<&str, usize>,
    diags: &mut Vec<Diagnostic>,
    lines: &HashMap<String, usize>,
) {
    for (idx, state) in spec.flow.iter().enumerate() {
        check_target(
            &state.transition,
            idx,
            spec,
            by_name,
            &state.name,
            lines.get(&state.name).copied(),
            diags,
        );
    }
    for block in spec.branches.values() {
        let idx = by_name.get(block.state_name.as_str()).copied();
        for case in &block.cases {
            match idx {
                Some(i) => check_target(
                    &case.target,
                    i,
                    spec,
                    by_name,
                    &block.state_name,
                    lines.get(&block.state_name).copied(),
                    diags,
                ),
                None => {
                    // Orphaned branch — already reported by check_branch_integrity.
                }
            }
        }
    }
}

fn check_target(
    target: &TransitionTarget,
    idx: usize,
    spec: &AglSpec,
    by_name: &HashMap<&str, usize>,
    from: &str,
    line: Option<usize>,
    diags: &mut Vec<Diagnostic>,
) {
    match target {
        TransitionTarget::Next => {
            if idx + 1 >= spec.flow.len() {
                diags.push(
                    Diagnostic::error(
                        "dangling-next",
                        format!(
                            "state '{from}' transitions to 'next' but is the last state in flow"
                        ),
                        from,
                    )
                    .with_line(line),
                );
            }
        }
        TransitionTarget::Goto(name) => {
            if !by_name.contains_key(name.as_str()) {
                diags.push(
                    Diagnostic::error(
                        "undefined-goto-target",
                        format!("state '{from}' transitions to undefined state '{name}'"),
                        from,
                    )
                    .with_line(line),
                );
            }
        }
        TransitionTarget::Terminate(_) => {}
        TransitionTarget::Branch(name) => {
            if !spec.branches.contains_key(name) {
                diags.push(
                    Diagnostic::error(
                        "undefined-branch",
                        format!("state '{from}' transitions to undefined branch '{name}'"),
                        from,
                    )
                    .with_line(line),
                );
            }
        }
    }
}

/// Pure graph successors (no diagnostics) — used by reachability, cycle
/// detection, and invariant path analysis alike.
fn successors(
    target: &TransitionTarget,
    idx: usize,
    flow: &[StateNode],
    by_name: &HashMap<&str, usize>,
    branches: &HashMap<String, BranchBlock>,
) -> Vec<String> {
    match target {
        TransitionTarget::Next => {
            if idx + 1 < flow.len() {
                vec![flow[idx + 1].name.clone()]
            } else {
                vec![]
            }
        }
        TransitionTarget::Goto(name) => {
            if by_name.contains_key(name.as_str()) {
                vec![name.clone()]
            } else {
                vec![]
            }
        }
        TransitionTarget::Terminate(_) => vec![],
        TransitionTarget::Branch(name) => match branches.get(name) {
            None => vec![],
            Some(block) => block
                .cases
                .iter()
                .flat_map(|case| successors(&case.target, idx, flow, by_name, branches))
                .collect(),
        },
    }
}

fn reachable_set(
    initial: &str,
    by_name: &HashMap<&str, usize>,
    flow: &[StateNode],
    branches: &HashMap<String, BranchBlock>,
) -> (HashSet<String>, HashSet<String>) {
    let mut visited = HashSet::new();
    let mut reachable_branches = HashSet::new();
    let mut stack = vec![initial.to_string()];
    while let Some(name) = stack.pop() {
        if !visited.insert(name.clone()) {
            continue;
        }
        let Some(&idx) = by_name.get(name.as_str()) else {
            continue;
        };
        let node = &flow[idx];
        if let TransitionTarget::Branch(b) = &node.transition {
            reachable_branches.insert(b.clone());
        }
        for next in successors(&node.transition, idx, flow, by_name, branches) {
            if !visited.contains(&next) {
                stack.push(next);
            }
        }
    }
    (visited, reachable_branches)
}

fn check_unreachable_states(
    spec: &AglSpec,
    visited: &HashSet<String>,
    initial: &str,
    diags: &mut Vec<Diagnostic>,
    lines: &HashMap<String, usize>,
) {
    for state in &spec.flow {
        if !visited.contains(state.name.as_str()) {
            diags.push(
                Diagnostic::error(
                    "unreachable-state",
                    format!(
                        "state '{}' is never reached from the initial state '{initial}'",
                        state.name
                    ),
                    state.name.clone(),
                )
                .with_line(lines.get(&state.name).copied()),
            );
        }
    }
}

/// DFS with an on-path visited set: a repeat visit to a node still on the
/// current path is a cycle that never reaches `Terminate`.
fn check_cycles(
    initial: &str,
    by_name: &HashMap<&str, usize>,
    flow: &[StateNode],
    branches: &HashMap<String, BranchBlock>,
    diags: &mut Vec<Diagnostic>,
    lines: &HashMap<String, usize>,
) {
    let mut on_path = HashSet::new();
    let mut done = HashSet::new();
    let mut found_cycles = HashSet::new();
    dfs_cycles(
        initial,
        by_name,
        flow,
        branches,
        &mut on_path,
        &mut done,
        &mut found_cycles,
    );
    for name in found_cycles {
        diags.push(
            Diagnostic::error(
                "non-terminating-cycle",
                format!(
                    "state '{name}' is part of a cycle that never reaches TERMINATE — every path must terminate"
                ),
                name.clone(),
            )
            .with_line(lines.get(&name).copied()),
        );
    }
}

fn dfs_cycles(
    name: &str,
    by_name: &HashMap<&str, usize>,
    flow: &[StateNode],
    branches: &HashMap<String, BranchBlock>,
    on_path: &mut HashSet<String>,
    done: &mut HashSet<String>,
    found_cycles: &mut HashSet<String>,
) {
    if on_path.contains(name) {
        found_cycles.insert(name.to_string());
        return;
    }
    if done.contains(name) {
        return;
    }
    let Some(&idx) = by_name.get(name) else {
        return;
    };
    on_path.insert(name.to_string());
    let node = &flow[idx];
    for next in successors(&node.transition, idx, flow, by_name, branches) {
        dfs_cycles(&next, by_name, flow, branches, on_path, done, found_cycles);
    }
    on_path.remove(name);
    done.insert(name.to_string());
}

fn is_fallback_condition(condition: &str) -> bool {
    matches!(
        condition.trim().to_lowercase().as_str(),
        "else" | "default" | "otherwise" | "true" | "_"
    )
}

fn check_branch_integrity(
    spec: &AglSpec,
    by_name: &HashMap<&str, usize>,
    reachable_branches: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    for (key, block) in &spec.branches {
        if !by_name.contains_key(block.state_name.as_str()) {
            diags.push(Diagnostic::error(
                "branch-orphaned",
                format!(
                    "branch '{key}' is keyed to state '{}' which does not exist",
                    block.state_name
                ),
                key.clone(),
            ));
        }
        if !reachable_branches.contains(key) {
            diags.push(Diagnostic::warning(
                "branch-unreferenced",
                format!("branch '{key}' is never reached by any state's transition"),
                key.clone(),
            ));
        }
        if block.cases.is_empty() {
            diags.push(Diagnostic::error(
                "branch-empty",
                format!("branch '{key}' has no cases"),
                key.clone(),
            ));
        } else if block.cases.len() == 1 && !is_fallback_condition(&block.cases[0].condition) {
            diags.push(Diagnostic::warning(
                "branch-not-exhaustive",
                format!(
                    "branch '{key}' only handles condition '{}' with no fallback case — \
                     unmatched inputs will dead-end at runtime",
                    block.cases[0].condition
                ),
                key.clone(),
            ));
        }
    }
}

// Real API method names rarely say "write" - Linear's are save_issue /
// save_customer_need, HubSpot's tool is manage_crm_objects, Notion-style
// APIs use upsert/patch/put. Found by converting two real workflows to
// AGL back to back and getting a silent non-match both times: the write
// happened, the invariant just never recognized it as a write.
const WRITE_SYNONYMS: &[&str] = &[
    "write", "update", "set", "create", "delete", "remove", "modify", "save", "manage", "sync",
    "publish", "insert", "upsert", "patch", "put", "post",
];
const READ_SYNONYMS: &[&str] = &["fetch", "get", "read", "list", "scan", "query"];

fn action_matches(rule_action: &str, function: &str) -> bool {
    let action = rule_action.to_lowercase();
    let function = function.to_lowercase();
    if function.contains(&action) {
        return true;
    }
    let synonyms: &[&str] = if WRITE_SYNONYMS.contains(&action.as_str()) {
        WRITE_SYNONYMS
    } else if READ_SYNONYMS.contains(&action.as_str()) {
        READ_SYNONYMS
    } else {
        &[]
    };
    synonyms.iter().any(|s| function.contains(s))
}

fn target_matches(rule_target: &str, haystack: &str) -> bool {
    haystack
        .to_lowercase()
        .contains(&rule_target.to_lowercase())
}

/// Read-only graph context threaded through the path-search helpers below,
/// bundled into one reference so the recursive DFS doesn't need a long
/// parameter list for what's really a single piece of state.
struct Graph<'a> {
    by_name: &'a HashMap<&'a str, usize>,
    flow: &'a [StateNode],
    branches: &'a HashMap<String, BranchBlock>,
}

/// DFS from `start` to `target`, tracking whether a `gate(gate_name)` state
/// was seen on the way. Returns true as soon as one path is found that
/// reaches `target` without having passed the required gate.
fn any_path_missing_gate(start: &str, target: &str, gate_name: &str, graph: &Graph) -> bool {
    fn dfs(
        current: &str,
        target: &str,
        gate_name: &str,
        gate_seen: bool,
        graph: &Graph,
        path: &mut HashSet<String>,
    ) -> bool {
        if current == target {
            return !gate_seen;
        }
        if !path.insert(current.to_string()) {
            return false; // cycle guard; reported separately
        }
        let result = match graph.by_name.get(current) {
            None => false,
            Some(&idx) => {
                let node = &graph.flow[idx];
                let gate_seen_here = gate_seen
                    || matches!(&node.action, StateAction::Gate { gate_name: g } if g == gate_name);
                successors(
                    &node.transition,
                    idx,
                    graph.flow,
                    graph.by_name,
                    graph.branches,
                )
                .into_iter()
                .any(|next| dfs(&next, target, gate_name, gate_seen_here, graph, path))
            }
        };
        path.remove(current);
        result
    }
    let mut path = HashSet::new();
    dfs(start, target, gate_name, false, graph, &mut path)
}

fn check_invariant_soundness(
    spec: &AglSpec,
    initial: &str,
    by_name: &HashMap<&str, usize>,
    diags: &mut Vec<Diagnostic>,
    lines: &HashMap<String, usize>,
) {
    for rule in &spec.invariants {
        match rule {
            InvariantRule::DenyWithoutGate {
                action,
                target,
                required_gate,
            } => {
                let gate_exists = spec.flow.iter().any(
                    |s| matches!(&s.action, StateAction::Gate { gate_name } if gate_name == required_gate),
                );
                if !gate_exists {
                    diags.push(Diagnostic::warning(
                        "invariant-gate-undefined",
                        format!(
                            "invariant requires gate '{required_gate}' but no state defines gate({required_gate})"
                        ),
                        "<spec>",
                    ));
                }

                for state in &spec.flow {
                    let (function, haystack) = match &state.action {
                        StateAction::Call { function, args } => {
                            (function.clone(), format!("{function} {}", args.join(" ")))
                        }
                        StateAction::Map { function, iterable } => {
                            (function.clone(), format!("{function} {iterable}"))
                        }
                        _ => continue,
                    };
                    if !action_matches(action, &function) || !target_matches(target, &haystack) {
                        continue;
                    }
                    let graph = Graph {
                        by_name,
                        flow: &spec.flow,
                        branches: &spec.branches,
                    };
                    if any_path_missing_gate(initial, &state.name, required_gate, &graph) {
                        diags.push(
                            Diagnostic::error(
                                "invariant-violation",
                                format!(
                                    "state '{}' performs {action}({target})-like action '{function}' \
                                     reachable without first passing gate({required_gate})",
                                    state.name
                                ),
                                state.name.clone(),
                            )
                            .with_line(lines.get(&state.name).copied()),
                        );
                    }
                }
            }
            InvariantRule::DenyConstraint {
                action,
                target,
                condition,
            } => {
                let referenced_input = spec
                    .inputs
                    .iter()
                    .find(|p| condition.split_whitespace().any(|w| w == p.name));

                let Some(input) = referenced_input else {
                    continue;
                };

                for state in &spec.flow {
                    let (function, args_text): (String, String) = match &state.action {
                        StateAction::Call { function, args } => (function.clone(), args.join(" ")),
                        StateAction::Map { function, iterable } => {
                            (function.clone(), iterable.clone())
                        }
                        _ => continue,
                    };
                    if !action_matches(action, &function) || !target_matches(target, &function) {
                        continue;
                    }
                    if !args_text.split_whitespace().any(|w| w == input.name) {
                        diags.push(
                            Diagnostic::warning(
                                "invariant-constraint-unchecked",
                                format!(
                                    "state '{}' calls '{function}' but doesn't pass '{}' — \
                                     cannot statically confirm constraint '{condition}' holds",
                                    state.name, input.name
                                ),
                                state.name.clone(),
                            )
                            .with_line(lines.get(&state.name).copied()),
                        );
                    }
                }
            }
        }
    }
}

pub fn format_pretty(diags: &[Diagnostic]) -> String {
    if diags.is_empty() {
        return "✓ valid: no issues found".to_string();
    }
    let mut out = String::new();
    let errors = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    let warnings = diags.len() - errors;
    out.push_str(&format!("{errors} error(s), {warnings} warning(s)\n"));
    for d in diags {
        let sev = match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warn ",
        };
        match d.line {
            Some(line) => out.push_str(&format!(
                "  [{sev}] {} (line {line}, {}): {}\n",
                d.code, d.location, d.message
            )),
            None => out.push_str(&format!(
                "  [{sev}] {} ({}): {}\n",
                d.code, d.location, d.message
            )),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agl::parser::parse;

    const SAMPLE: &str = r#"
    spec MeetingPrep {
      in:  calendar_event: str, slack_channels: list[str]
      out: agenda_update: str

      invariant {
        deny: write(calendar) without gate(human_approval)
        deny: fetch(slack) where channel NOT IN slack_channels
      }

      flow {
        state FETCH_CALENDAR  -> call(GoogleCalendar.get, calendar_event) -> next
        state SCAN_SLACK      -> map(Slack.read, slack_channels)           -> next
        state DIFF_AGENDA     -> evaluate(slack_data vs calendar_data)    -> branch

        branch DIFF_AGENDA {
          if no_diff -> TERMINATE("Already up to date")
          if has_diff -> PROPOSE_UPDATE
        }

        state PROPOSE_UPDATE  -> gate(human_approval)                     -> EXECUTE_WRITE
        state EXECUTE_WRITE   -> call(GoogleCalendar.update, agenda)      -> TERMINATE("Done")
      }
    }
    "#;

    #[test]
    fn canonical_sample_has_no_errors() {
        let parsed = parse(SAMPLE).unwrap();
        let diags = validate(&parsed.spec, &parsed.state_lines);
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:#?}");
    }

    #[test]
    fn detects_unreachable_state() {
        let src = r#"spec Foo {
            in: x: str
            out: y: str
            flow {
                state A -> evaluate(x) -> TERMINATE("done")
                state B -> evaluate(x) -> TERMINATE("done")
            }
        }"#;
        let parsed = parse(src).unwrap();
        let diags = validate(&parsed.spec, &parsed.state_lines);
        assert!(diags
            .iter()
            .any(|d| d.code == "unreachable-state" && d.location == "B"));
    }

    #[test]
    fn detects_dangling_next() {
        let src = r#"spec Foo {
            in: x: str
            out: y: str
            flow {
                state A -> evaluate(x) -> next
            }
        }"#;
        let parsed = parse(src).unwrap();
        let diags = validate(&parsed.spec, &parsed.state_lines);
        assert!(diags.iter().any(|d| d.code == "dangling-next"));
    }

    #[test]
    fn detects_undefined_goto_target() {
        let src = r#"spec Foo {
            in: x: str
            out: y: str
            flow {
                state A -> evaluate(x) -> GHOST
            }
        }"#;
        let parsed = parse(src).unwrap();
        let diags = validate(&parsed.spec, &parsed.state_lines);
        assert!(diags.iter().any(|d| d.code == "undefined-goto-target"));
    }

    #[test]
    fn detects_undefined_branch() {
        let src = r#"spec Foo {
            in: x: str
            out: y: str
            flow {
                state A -> evaluate(x) -> branch
            }
        }"#;
        let parsed = parse(src).unwrap();
        let diags = validate(&parsed.spec, &parsed.state_lines);
        assert!(diags.iter().any(|d| d.code == "undefined-branch"));
    }

    #[test]
    fn detects_non_terminating_cycle() {
        let src = r#"spec Foo {
            in: x: str
            out: y: str
            flow {
                state A -> evaluate(x) -> B
                state B -> evaluate(x) -> A
            }
        }"#;
        let parsed = parse(src).unwrap();
        let diags = validate(&parsed.spec, &parsed.state_lines);
        assert!(diags.iter().any(|d| d.code == "non-terminating-cycle"));
    }

    #[test]
    fn detects_invariant_violation_missing_gate() {
        let src = r#"spec Foo {
            in: calendar_event: str
            out: agenda_update: str
            invariant {
                deny: write(calendar) without gate(human_approval)
            }
            flow {
                state WRITE_IT -> call(GoogleCalendar.update, calendar_event) -> TERMINATE("done")
            }
        }"#;
        let parsed = parse(src).unwrap();
        let diags = validate(&parsed.spec, &parsed.state_lines);
        assert!(diags.iter().any(|d| d.code == "invariant-violation"));
    }

    #[test]
    fn detects_missing_gate_on_a_real_linear_save_verb() {
        // Linear's actual tool names are save_issue / save_customer_need -
        // "save" wasn't in WRITE_SYNONYMS, so this write went unrecognized
        // until it was added. Found converting workflow-feature-request-bug.
        let src = r#"spec Foo {
            in: x: str
            out: y: str
            invariant {
                deny: write(linear) without gate(human_approval)
            }
            flow {
                state FILE_IT -> call(Linear.save_issue, x) -> TERMINATE("done")
            }
        }"#;
        let parsed = parse(src).unwrap();
        let diags = validate(&parsed.spec, &parsed.state_lines);
        assert!(
            diags.iter().any(|d| d.code == "invariant-violation"),
            "{diags:?}"
        );
    }

    #[test]
    fn detects_missing_gate_on_a_real_hubspot_manage_verb() {
        // HubSpot's real MCP tool is manage_crm_objects (generic, covers
        // both reads and writes) - "manage" wasn't in WRITE_SYNONYMS either.
        let src = r#"spec Foo {
            in: x: str
            out: y: str
            invariant {
                deny: write(hubspot) without gate(human_approval)
            }
            flow {
                state SYNC_IT -> call(HubSpot.manage_crm_objects, x) -> TERMINATE("done")
            }
        }"#;
        let parsed = parse(src).unwrap();
        let diags = validate(&parsed.spec, &parsed.state_lines);
        assert!(
            diags.iter().any(|d| d.code == "invariant-violation"),
            "{diags:?}"
        );
    }

    #[test]
    fn branch_missing_fallback_warns() {
        let src = r#"spec Foo {
            in: x: str
            out: y: str
            flow {
                state A -> evaluate(x) -> branch
                branch A {
                    if cond -> TERMINATE("done")
                }
            }
        }"#;
        let parsed = parse(src).unwrap();
        let diags = validate(&parsed.spec, &parsed.state_lines);
        assert!(diags.iter().any(|d| d.code == "branch-not-exhaustive"));
    }

    const TOOL_SPEC: &str = r#"spec Foo {
        in: x: str
        out: y: str
        flow {
            state A -> call(HubSpot.update_contact, x) -> TERMINATE("done")
        }
    }"#;

    #[test]
    fn undefined_tool_binding_warns_when_manifest_missing_function() {
        let parsed = parse(TOOL_SPEC).unwrap();
        let manifest: HashSet<String> = ["TechnicalSuccessHub.read_page".to_string()]
            .into_iter()
            .collect();
        let diags = check_tool_bindings(&parsed.spec, &manifest, &parsed.state_lines);
        assert!(diags
            .iter()
            .any(|d| d.code == "undefined-tool-binding" && d.severity == Severity::Warning));
    }

    #[test]
    fn undefined_tool_binding_silent_when_manifest_covers_everything() {
        let parsed = parse(TOOL_SPEC).unwrap();
        let manifest: HashSet<String> =
            ["HubSpot.update_contact".to_string()].into_iter().collect();
        let diags = check_tool_bindings(&parsed.spec, &manifest, &parsed.state_lines);
        assert!(diags.is_empty());
    }

    #[test]
    fn undefined_tool_binding_warning_never_flips_exit_status() {
        let parsed = parse(TOOL_SPEC).unwrap();
        let manifest: HashSet<String> = HashSet::new();
        let diags = check_tool_bindings(&parsed.spec, &manifest, &parsed.state_lines);
        assert!(!diags.is_empty());
        assert!(!has_errors(&diags));
    }

    #[test]
    fn undeclared_tool_dependency_warns_when_requires_is_incomplete() {
        let src = r#"spec Foo {
            in: x: str
            out: y: str
            requires: TechnicalSuccessHub.write_page

            flow {
                state A -> call(HubSpot.update_contact, x) -> TERMINATE("done")
            }
        }"#;
        let parsed = parse(src).unwrap();
        let diags = validate(&parsed.spec, &parsed.state_lines);
        assert!(
            diags.iter().any(|d| d.code == "undeclared-tool-dependency"),
            "{diags:?}"
        );
        assert!(!has_errors(&diags));
    }

    #[test]
    fn unused_tool_dependency_warns_when_requires_lists_a_never_called_tool() {
        let src = r#"spec Foo {
            in: x: str
            out: y: str
            requires: HubSpot.update_contact, TechnicalSuccessHub.write_page

            flow {
                state A -> call(HubSpot.update_contact, x) -> TERMINATE("done")
            }
        }"#;
        let parsed = parse(src).unwrap();
        let diags = validate(&parsed.spec, &parsed.state_lines);
        assert!(
            diags.iter().any(|d| d.code == "unused-tool-dependency"),
            "{diags:?}"
        );
    }

    #[test]
    fn tool_dependency_checks_are_silent_when_requires_is_fully_covered() {
        let src = r#"spec Foo {
            in: x: str
            out: y: str
            requires: HubSpot.update_contact

            flow {
                state A -> call(HubSpot.update_contact, x) -> TERMINATE("done")
            }
        }"#;
        let parsed = parse(src).unwrap();
        let diags = validate(&parsed.spec, &parsed.state_lines);
        assert!(!diags
            .iter()
            .any(|d| d.code == "undeclared-tool-dependency" || d.code == "unused-tool-dependency"));
    }

    #[test]
    fn tool_dependency_checks_are_silent_when_requires_is_absent() {
        // TOOL_SPEC has no `requires:` line at all — the check must not
        // fire just because a call() exists with nothing declared.
        let parsed = parse(TOOL_SPEC).unwrap();
        let diags = validate(&parsed.spec, &parsed.state_lines);
        assert!(!diags
            .iter()
            .any(|d| d.code == "undeclared-tool-dependency" || d.code == "unused-tool-dependency"));
    }
}
