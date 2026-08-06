//! System-prompt compiler: renders a parsed `AglSpec` into a token-dense
//! XML/Markdown block meant for direct injection into an agent's context
//! window (Claude Code, Cursor, or any LLM runtime).

use super::ast::{AglSpec, ArgRef, DataType, InvariantRule, StateAction, TransitionTarget};

fn render_type(dt: &DataType) -> String {
    match dt {
        DataType::String => "str".to_string(),
        DataType::Int => "int".to_string(),
        DataType::Bool => "bool".to_string(),
        DataType::List(inner) => format!("list[{}]", render_type(inner)),
        DataType::Custom(name) => name.clone(),
    }
}

fn render_arg(arg: &ArgRef) -> String {
    match arg {
        ArgRef::Var(name) => name.clone(),
        // No escaping - the lexer's string_lit has none either (it scans
        // to the next raw '"'), so a literal containing '"' can't round-trip
        // either direction. Fine for the short config strings this is for.
        ArgRef::Literal(text) => format!("\"{text}\""),
    }
}

fn render_action(action: &StateAction) -> String {
    match action {
        StateAction::Call { function, args } => {
            let rendered: Vec<String> = args.iter().map(render_arg).collect();
            format!("call({function}, {})", rendered.join(", "))
        }
        StateAction::Map { function, iterable } => format!("map({function}, {iterable})"),
        StateAction::Evaluate { expression } => format!("evaluate({expression})"),
        StateAction::Gate { gate_name } => format!("gate({gate_name})"),
    }
}

fn render_target(target: &TransitionTarget) -> String {
    match target {
        TransitionTarget::Next => "next".to_string(),
        TransitionTarget::Branch(name) => format!("branch({name})"),
        TransitionTarget::Goto(name) => name.clone(),
        TransitionTarget::Terminate(msg) => format!("TERMINATE(\"{msg}\")"),
    }
}

fn render_invariant(rule: &InvariantRule) -> String {
    match rule {
        InvariantRule::DenyWithoutGate {
            action,
            target,
            required_gate,
        } => format!("DENY {action}({target}) WITHOUT gate({required_gate})"),
        InvariantRule::DenyConstraint {
            action,
            target,
            condition,
        } => format!("DENY {action}({target}) WHERE {condition}"),
    }
}

/// Render `spec` as a compact `<agent_spec>` block: I/O contract, invariants,
/// and the flow graph, followed by a short execution contract telling the
/// agent how to treat gates, denials, and termination.
pub fn to_prompt(spec: &AglSpec) -> String {
    let mut out = String::new();

    out.push_str(&format!("<agent_spec name=\"{}\">\n", spec.name));

    out.push_str("<io>\n");
    if !spec.inputs.is_empty() {
        let params = spec
            .inputs
            .iter()
            .map(|p| format!("{}:{}", p.name, render_type(&p.data_type)))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("in: {params}\n"));
    }
    if !spec.outputs.is_empty() {
        let params = spec
            .outputs
            .iter()
            .map(|p| format!("{}:{}", p.name, render_type(&p.data_type)))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("out: {params}\n"));
    }
    out.push_str("</io>\n");

    if !spec.invariants.is_empty() {
        out.push_str("<invariants>\n");
        for rule in &spec.invariants {
            out.push_str(&format!("- {}\n", render_invariant(rule)));
        }
        out.push_str("</invariants>\n");
    }

    let initial = spec.flow.first().map(|s| s.name.as_str()).unwrap_or("");
    out.push_str(&format!("<flow initial=\"{initial}\">\n"));
    for state in &spec.flow {
        out.push_str(&format!(
            "{}: {} => {}\n",
            state.name,
            render_action(&state.action),
            render_target(&state.transition)
        ));
        if let TransitionTarget::Branch(name) = &state.transition {
            if let Some(block) = spec.branches.get(name) {
                for case in &block.cases {
                    out.push_str(&format!(
                        "  if {} => {}\n",
                        case.condition,
                        render_target(&case.target)
                    ));
                }
            }
        }
    }
    out.push_str("</flow>\n");

    out.push_str(
        "<execution_contract>\n\
         Execute this finite-state program exactly as specified — do not skip, reorder, or invent states.\n\
         1. Run the current state's action, then follow its `=>` transition.\n\
         2. Before any action matching a `WITHOUT gate(...)` invariant, stop and obtain that gate's \
         approval before proceeding — never perform the action first.\n\
         3. Never perform an action a `DENY` rule forbids, even if a later step seems to need it.\n\
         4. On a branch, evaluate its conditions in order and follow the first that matches.\n\
         5. On `TERMINATE(\"msg\")`, stop immediately and return msg as the result.\n\
         </execution_contract>\n",
    );

    out.push_str("</agent_spec>\n");
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
    fn renders_expected_sections() {
        let parsed = parse(SAMPLE).unwrap();
        let prompt = to_prompt(&parsed.spec);
        assert!(prompt.contains("<agent_spec name=\"MeetingPrep\">"));
        assert!(prompt.contains("in: calendar_event:str, slack_channels:list[str]"));
        assert!(prompt.contains("out: agenda_update:str"));
        assert!(prompt.contains("DENY write(calendar) WITHOUT gate(human_approval)"));
        assert!(prompt.contains("DENY fetch(slack) WHERE channel NOT IN slack_channels"));
        assert!(prompt.contains("<flow initial=\"FETCH_CALENDAR\">"));
        assert!(prompt.contains("FETCH_CALENDAR: call(GoogleCalendar.get, calendar_event) => next"));
        assert!(prompt
            .contains("DIFF_AGENDA: evaluate(slack_data vs calendar_data) => branch(DIFF_AGENDA)"));
        assert!(prompt.contains("if no_diff => TERMINATE(\"Already up to date\")"));
        assert!(prompt.contains("if has_diff => PROPOSE_UPDATE"));
        assert!(prompt
            .contains("EXECUTE_WRITE: call(GoogleCalendar.update, agenda) => TERMINATE(\"Done\")"));
        assert!(prompt.contains("<execution_contract>"));
        assert!(prompt.ends_with("</agent_spec>\n"));
    }
}
