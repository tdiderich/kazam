//! `kazam agl skill` — compiles a validated, import-resolved `.agl` spec
//! into a portable "skill" document for an LLM coding tool (Claude Code,
//! Cursor, Codex): a static primer on how to execute an AGL graph, plus the
//! fully-resolved spec rendered back out in native `.agl` syntax, wrapped
//! per target.
//!
//! This is a different, unrelated export path from `compiler::to_prompt` —
//! that renders a natural-language `<agent_spec>` prompt block; this module
//! renders the spec's actual source syntax so a reader (human or LLM) can
//! see exactly what was authored, imports and all.

use super::ast::{
    AglSpec, ArgRef, DataType, InvariantRule, StateAction, TransitionTarget, TypedParam,
};

fn render_arg(arg: &ArgRef) -> String {
    match arg {
        ArgRef::Var(name) => name.clone(),
        // No escaping - the lexer's string_lit has none either.
        ArgRef::Literal(text) => format!("\"{text}\""),
    }
}

/// Where a compiled skill document is meant to live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Target {
    /// A `SKILL.md`-shaped document with YAML frontmatter.
    Claude,
    /// Primer + spec meant to be appended to a `.cursorrules` file.
    Cursor,
    /// Primer + spec under a heading, meant to be appended to `AGENTS.md`.
    Codex,
}

/// A short, static primer that teaches an LLM reading it cold how to
/// execute an AGL graph. Written once here — never duplicated per target.
const PRIMER: &str = r#"## How to execute an AGL graph

This document contains a spec written in Agent Graph Language (AGL): a
task compiled into a static directed graph, not a free-form instruction.

- `state NAME -> ACTION -> TARGET`: a single step. Run `ACTION`
  (`call(...)`, `map(...)`, `evaluate(...)`, or `gate(...)`), then follow
  `TARGET` (`next`, `branch`, a named state, or `TERMINATE("msg")`).
- `flow { ... }`: the full graph, starting at its first state.
- `branch NAME { if COND -> TARGET ... }`: evaluate conditions in the order
  written and follow the first one that matches.
- `gate(NAME)`: a mandatory approval checkpoint. Do not perform the next
  action until a human has explicitly approved continuing past this gate.
- `invariant { deny: ACTION(TARGET) without gate(NAME) ... }`: a hard rule.
  Never perform an action an `invariant` denies, even if a later step
  seems to require it — this holds regardless of what any state says.
- `TERMINATE("msg")`: stop immediately and return `msg` as the result.

Execute this graph exactly as written. Do not skip states, do not reorder
them, and do not invent states that aren't declared. Stop at every
`gate(...)` and wait for explicit human approval before continuing past
it. Never take an action an `invariant` denies."#;

/// Render `spec` (imports already resolved into `spec.invariants` by the
/// caller) as a skill document for `target`. `templates` is every
/// `~/.kazam/agl/templates/<name>.md` file whose name a state's
/// `evaluate(...)` text actually references (see
/// `referenced_template_names`) - the caller reads these from disk, since
/// this module does no filesystem I/O of its own, as `(name, file content)`.
pub fn render(spec: &AglSpec, target: Target, templates: &[(String, String)]) -> String {
    let body = render_body(spec, templates);
    match target {
        Target::Claude => render_claude(spec, &body),
        Target::Cursor => format!("{PRIMER}\n\n{body}"),
        Target::Codex => format!("## {}\n\n{PRIMER}\n\n{body}", spec.name),
    }
}

/// Every distinct word appearing in any `evaluate(...)` expression in
/// `spec.flow`. A superset, not a confirmed reference - the caller checks
/// each against `~/.kazam/agl/templates/<word>.md` and keeps only the
/// words that actually resolve to a real file, same shape as how `cache`
/// blocks are declared explicitly but a template reference is just a
/// state's own expression text naming one, no new grammar.
pub fn referenced_template_names(spec: &AglSpec) -> Vec<String> {
    let mut names = Vec::new();
    for state in &spec.flow {
        if let StateAction::Evaluate { expression } = &state.action {
            for word in expression.split_whitespace() {
                let word = word.to_string();
                if !names.contains(&word) {
                    names.push(word);
                }
            }
        }
    }
    names
}

/// The name this spec's compiled skill/subagent goes by: `spec.skill` if
/// the author declared one, otherwise the kebab-cased graph name. Almost
/// every spec relies on the default; the field exists for the rare case
/// where the graph's internal name and its public trigger name should
/// differ (versioning, renames, matching an existing skill's slug).
pub fn resolved_skill_name(spec: &AglSpec) -> String {
    spec.skill.clone().unwrap_or_else(|| kebab_case(&spec.name))
}

/// A tool-scoped Claude Code subagent (`.claude/agents/<name>.md`) that
/// executes this graph and nothing else: `tools:` is exactly `requires:`,
/// verbatim, so the harness enforces the same boundary the Preflight
/// section describes rather than relying on the model to self-police it.
/// `requires:` empty means no tool restriction gets written at all - an
/// empty `tools:` line would be worse than none, since clap/Claude Code
/// would read it as "no tools", not "unrestricted".
///
/// Opt-in only (`kazam agl load --isolated`), never the default: a
/// subagent has no way to verify a relayed "approved" came from a real
/// human rather than the orchestrating agent's own paraphrase, so the
/// caller must reject any spec with a gate-protected write before calling
/// this (see `validator::has_gate_protected_writes`) - this function
/// doesn't check that itself, it just renders.
pub fn render_agent_file(spec: &AglSpec, templates: &[(String, String)]) -> String {
    let name = resolved_skill_name(spec);
    let body = render_body(spec, templates);
    let mut out =
        format!("---\nname: {name}\ndescription: \"Runs the {name} AGL graph end to end\"\n");
    if !spec.requires.is_empty() {
        out.push_str(&format!("tools: {}\n", spec.requires.join(", ")));
    }
    out.push_str(&format!("model: sonnet\n---\n\n{PRIMER}\n\n{body}"));
    out
}

/// A thin Claude Code skill (`.claude/skills/<name>.md`) that only routes
/// to the subagent above. Only ever paired with `render_agent_file` under
/// `--isolated`, for specs with no gate-protected write - the default
/// (`render(spec, Target::Claude, templates)`) runs inline instead, which is what
/// everything with an approval gate has to do.
pub fn render_skill_dispatcher(spec: &AglSpec) -> String {
    let name = resolved_skill_name(spec);
    let spec_name = &spec.name;
    format!(
        "---\nname: {name}\ndescription: \"Runs the {name} AGL graph ({spec_name})\"\n---\n\n\
         Dispatch to the `{name}` subagent (Agent tool, subagent_type: \"{name}\") to run this \
         graph. Do not execute the graph's states yourself in this context - the subagent's \
         `tools:` allowlist is what makes the Preflight check in that graph meaningful; running \
         it here instead would just be free-form instructions again.\n"
    )
}

/// The part every target shares once past its own header: preflight (if
/// `requires` is declared), the run-order note, the ASCII flow diagram, and
/// the resolved `.agl` source.
fn render_body(spec: &AglSpec, templates: &[(String, String)]) -> String {
    let mut out = String::new();
    let preflight = render_preflight(spec);
    if !preflight.is_empty() {
        out.push_str(&preflight);
        out.push('\n');
    }
    let cache_section = render_cache(spec);
    if !cache_section.is_empty() {
        out.push_str(&cache_section);
        out.push('\n');
    }
    let templates_section = render_templates(templates);
    if !templates_section.is_empty() {
        out.push_str(&templates_section);
        out.push('\n');
    }
    out.push_str(RUN_ORDER);
    out.push_str("\n\n## Flow\n\n```\n");
    out.push_str(&render_ascii_flow(spec));
    out.push_str("```\n\n```agl\n");
    out.push_str(&render_agl_source(spec));
    out.push_str("```\n");
    out
}

fn render_claude(spec: &AglSpec, body: &str) -> String {
    let name = resolved_skill_name(spec);
    format!(
        "---\nname: {name}\ndescription: \"Runs the {name} AGL graph\"\n---\n\n{PRIMER}\n\n{body}"
    )
}

/// A runtime preflight check, generated from `spec.requires`: before a cold
/// agent executes any state, it should confirm every declared tool is
/// actually available and abort rather than fail mid-graph after some
/// states have already run. Returns an empty string (no section at all)
/// when `requires` is empty, so specs written before this field existed
/// don't get a confusing, empty preflight block.
fn render_preflight(spec: &AglSpec) -> String {
    if spec.requires.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "## Preflight\n\nBefore executing any state in this graph, confirm every tool below is \
         available in your current session/toolset. If any are missing, stop immediately - do \
         not execute any state - and report exactly which tools are missing.\n\n",
    );
    for tool in &spec.requires {
        out.push_str(&format!("- {tool}\n"));
    }
    out
}

/// The path a cache block's runtime data lives at, outside the compiled
/// skill entirely - `~` (not a resolved absolute path), since this string
/// is only ever embedded as instructions for whatever agent executes the
/// graph, and it resolves `~` with its own tools.
pub fn cache_file_path(name: &str) -> String {
    format!("~/.kazam/agl/cache/{name}.jsonl")
}

/// Instructions for each declared cache: where its file lives, its schema,
/// and the check-before-resolve / append-after-resolve convention. Empty
/// string (no section at all) when the spec declares no cache, same
/// convention as `render_preflight`.
fn render_cache(spec: &AglSpec) -> String {
    if spec.cache.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "## Cache\n\nThis graph reads and writes local JSONL caches, outside this skill file \
         entirely - `kazam agl load` regenerating this file never touches them. For each cache \
         below, before doing a lookup it would otherwise repeat, check the file for the most \
         recent line matching what you need (whichever field identifies a record, like \
         `customer`). Use it if found. Otherwise resolve normally, then append a new JSON line \
         with every field you now know.\n\n",
    );
    for block in &spec.cache {
        out.push_str(&format!("### {}\n\n", block.name));
        out.push_str(&format!("File: `{}`\n\n", cache_file_path(&block.name)));
        out.push_str("Fields (each line one JSON object):\n");
        for field in &block.fields {
            out.push_str(&format!(
                "- {}: {}\n",
                field.name,
                render_type(&field.data_type)
            ));
        }
        out.push('\n');
    }
    out
}

/// Splits a template file's raw content on its `<!--samples-->` marker:
/// everything before is the shape/boilerplate to follow, everything after
/// is known-good examples. No marker means the whole file is the shape,
/// no examples section. A leading `<!--spec-->` marker (optional, purely
/// for a human skimming the raw file) is stripped either way.
fn split_template(content: &str) -> (String, Option<String>) {
    if let Some(idx) = content.find("<!--samples-->") {
        let shape = content[..idx].replace("<!--spec-->", "");
        let samples = content[idx + "<!--samples-->".len()..].to_string();
        (shape.trim().to_string(), Some(samples.trim().to_string()))
    } else {
        (content.replace("<!--spec-->", "").trim().to_string(), None)
    }
}

/// Embeds every resolved template the caller found (see
/// `referenced_template_names`) into its own subsection: the shape to
/// follow, plus known-good examples when the file has any. Empty string
/// (no section at all) when nothing was referenced, same convention as
/// `render_preflight`/`render_cache`.
fn render_templates(templates: &[(String, String)]) -> String {
    if templates.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "## Templates\n\nReferenced by name in this graph's evaluate(...) states. Follow the \
         shape exactly; the examples (when present) are known-good outputs to match tone and \
         structure against, not to copy verbatim.\n\n",
    );
    for (name, content) in templates {
        let (shape, samples) = split_template(content);
        out.push_str(&format!("### {name}\n\n{shape}\n\n"));
        if let Some(samples) = samples {
            out.push_str(&format!("**Known-good examples:**\n\n{samples}\n\n"));
        }
    }
    out
}

/// Connective instruction between the (optional) preflight section and the
/// flow diagram: what to do, in order, before running the first state. The
/// diagram itself is purely informational — showing it isn't an approval
/// gate, it's just letting a human see the plan before the agent starts.
/// Only a `gate(...)` inside the graph itself should ever block on approval.
const RUN_ORDER: &str = "## Before you start

1. If a Preflight section is above, confirm every tool listed is available. Stop and report immediately if any are missing - do not execute any state.
2. Show the flow diagram below to the user so they can see what you're about to do.
3. Begin executing the graph. Do not wait for approval to start - only stop where the graph itself defines a `gate(...)`.";

/// A linear, top-to-bottom ASCII rendering of the flow: each state in
/// declaration order, its action, and where it goes next. Branches fan out
/// as an indented case list directly under the state that owns them. Meant
/// to be shown to a human (or printed by a cold agent at the start of a
/// run) as "here's what I'm about to do" — a plan preview, not the graph's
/// actual source syntax (that's `render_agl_source`).
pub fn render_ascii_flow(spec: &AglSpec) -> String {
    let mut out = String::new();
    for (i, state) in spec.flow.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&format!(
            "{}  {}\n",
            state.name,
            render_action(&state.action)
        ));
        match &state.transition {
            TransitionTarget::Branch(name) => match spec.branches.get(name) {
                Some(block) => {
                    let last = block.cases.len().saturating_sub(1);
                    for (j, case) in block.cases.iter().enumerate() {
                        let corner = if j == last { "\u{2514}" } else { "\u{251c}" };
                        out.push_str(&format!(
                            "  {corner}\u{2500} if {} -> {}\n",
                            case.condition,
                            render_target(&case.target)
                        ));
                    }
                }
                None => out.push_str("  \u{2514}\u{2500}> branch (undefined)\n"),
            },
            other => out.push_str(&format!("  \u{2514}\u{2500}> {}\n", render_target(other))),
        }
    }
    out
}

/// `Name`, `NAME`, `name_here` -> `name`, `name`, `name-here`: lowercase,
/// with a `-` inserted before each uppercase letter that follows a
/// lowercase/digit (so `MeetingPrep` -> `meeting-prep`), and underscores
/// normalized to `-`.
pub fn kebab_case(name: &str) -> String {
    let mut out = String::new();
    let mut prev_lower_or_digit = false;
    for c in name.chars() {
        if c == '_' || c == ' ' {
            out.push('-');
            prev_lower_or_digit = false;
            continue;
        }
        if c.is_uppercase() && prev_lower_or_digit {
            out.push('-');
        }
        out.extend(c.to_lowercase());
        prev_lower_or_digit = c.is_lowercase() || c.is_numeric();
    }
    out
}

fn render_type(dt: &DataType) -> String {
    match dt {
        DataType::String => "str".to_string(),
        DataType::Int => "int".to_string(),
        DataType::Bool => "bool".to_string(),
        DataType::List(inner) => format!("list[{}]", render_type(inner)),
        DataType::Custom(name) => name.clone(),
    }
}

fn render_params(params: &[TypedParam]) -> String {
    params
        .iter()
        .map(|p| format!("{}: {}", p.name, render_type(&p.data_type)))
        .collect::<Vec<_>>()
        .join(", ")
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
        TransitionTarget::Branch(_) => "branch".to_string(),
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
        } => format!("deny: {action}({target}) without gate({required_gate})"),
        InvariantRule::DenyConstraint {
            action,
            target,
            condition,
        } => format!("deny: {action}({target}) where {condition}"),
    }
}

/// Pretty-print `spec` back to native `.agl` source. Not the compact
/// `to_prompt` natural-language export — this round-trips the actual
/// grammar so a reader can see precisely what was authored (and, via
/// `render`, what an import pulled in).
pub fn render_agl_source(spec: &AglSpec) -> String {
    let mut out = String::new();
    out.push_str(&format!("spec {} {{\n", spec.name));
    out.push_str(&format!("  in: {}\n", render_params(&spec.inputs)));
    out.push_str(&format!("  out: {}\n", render_params(&spec.outputs)));
    if !spec.requires.is_empty() {
        out.push_str(&format!("  requires: {}\n", spec.requires.join(", ")));
    }
    if let Some(skill_name) = &spec.skill {
        out.push_str(&format!("  skill: {skill_name}\n"));
    }

    for block in &spec.cache {
        out.push_str(&format!(
            "  cache {} {{ {} }}\n",
            block.name,
            render_params(&block.fields)
        ));
    }

    if !spec.invariants.is_empty() {
        out.push_str("\n  invariant {\n");
        for rule in &spec.invariants {
            out.push_str(&format!("    {}\n", render_invariant(rule)));
        }
        out.push_str("  }\n");
    }

    out.push_str("\n  flow {\n");
    for state in &spec.flow {
        out.push_str(&format!(
            "    state {} -> {} -> {}\n",
            state.name,
            render_action(&state.action),
            render_target(&state.transition)
        ));
        if let TransitionTarget::Branch(name) = &state.transition {
            if let Some(block) = spec.branches.get(name) {
                out.push_str(&format!("\n    branch {name} {{\n"));
                for case in &block.cases {
                    out.push_str(&format!(
                        "      if {} -> {}\n",
                        case.condition,
                        render_target(&case.target)
                    ));
                }
                out.push_str("    }\n\n");
            }
        }
    }
    out.push_str("  }\n");
    out.push_str("}\n");
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
      }

      flow {
        state FETCH_CALENDAR  -> call(GoogleCalendar.get, calendar_event) -> next
        state PROPOSE_UPDATE  -> gate(human_approval)                     -> EXECUTE_WRITE
        state EXECUTE_WRITE   -> call(GoogleCalendar.update, agenda)      -> TERMINATE("Done")
      }
    }
    "#;

    #[test]
    fn kebab_cases_pascal_and_snake_names() {
        assert_eq!(kebab_case("MeetingPrep"), "meeting-prep");
        assert_eq!(kebab_case("hubspot_sync"), "hubspot-sync");
        assert_eq!(kebab_case("Already-Kebab"), "already-kebab");
    }

    #[test]
    fn renders_agl_source_that_reparses() {
        let parsed = parse(SAMPLE).unwrap();
        let rendered = render_agl_source(&parsed.spec);
        let reparsed = parse(&rendered).expect("re-rendered source should reparse");
        assert_eq!(reparsed.spec, parsed.spec);
    }

    #[test]
    fn cache_blocks_round_trip_through_render_agl_source() {
        let src = r#"
        spec CallPrep {
            in: customer: str
            out: y: bool

            cache slack-lookups {
                customer: str, int_channel: str
            }
            cache call-prep-timestamps {
                customer: str, last_call_date: str
            }

            flow {
                state A -> evaluate(customer) -> TERMINATE("done")
            }
        }"#;
        let parsed = parse(src).unwrap();
        let rendered = render_agl_source(&parsed.spec);
        assert!(rendered.contains("cache slack-lookups"));
        assert!(rendered.contains("cache call-prep-timestamps"));
        let reparsed = parse(&rendered).expect("re-rendered source should reparse");
        assert_eq!(reparsed.spec, parsed.spec);
    }

    #[test]
    fn compiled_skill_includes_a_cache_section_per_block() {
        let src = r#"
        spec CallPrep {
            in: customer: str
            out: y: bool

            cache slack-lookups {
                customer: str, int_channel: str
            }

            flow {
                state A -> evaluate(customer) -> TERMINATE("done")
            }
        }"#;
        let parsed = parse(src).unwrap();
        let doc = render(&parsed.spec, Target::Claude, &[]);
        assert!(doc.contains("## Cache"));
        assert!(doc.contains("### slack-lookups"));
        assert!(doc.contains("~/.kazam/agl/cache/slack-lookups.jsonl"));
        assert!(doc.contains("- customer: str"));
        assert!(doc.contains("- int_channel: str"));
    }

    #[test]
    fn compiled_skill_omits_cache_section_when_spec_declares_none() {
        let parsed = parse(SAMPLE).unwrap();
        let doc = render(&parsed.spec, Target::Claude, &[]);
        assert!(!doc.contains("## Cache"));
    }

    #[test]
    fn referenced_template_names_returns_every_word_in_every_evaluate() {
        let src = r#"spec Foo {
            in: x: str
            out: y: str
            flow {
                state A -> evaluate(activity_summary_draft vs activity-summary) -> next
                state B -> evaluate(x) -> TERMINATE("done")
            }
        }"#;
        let parsed = parse(src).unwrap();
        let names = referenced_template_names(&parsed.spec);
        assert!(names.contains(&"activity-summary".to_string()));
        assert!(names.contains(&"activity_summary_draft".to_string()));
        assert!(names.contains(&"vs".to_string()));
        assert!(names.contains(&"x".to_string()));
    }

    #[test]
    fn compiled_skill_omits_templates_section_when_none_resolved() {
        let parsed = parse(SAMPLE).unwrap();
        let doc = render(&parsed.spec, Target::Claude, &[]);
        assert!(!doc.contains("## Templates"));
    }

    #[test]
    fn compiled_skill_embeds_a_resolved_template_split_on_samples_marker() {
        let parsed = parse(SAMPLE).unwrap();
        let templates = vec![(
            "activity-summary".to_string(),
            "<!--spec-->\n## {Customer}\n\n- **Lead-in**: summary\n\
             <!--samples-->\n## Halcyon\n\n- **Sentiment**: stayed Medium"
                .to_string(),
        )];
        let doc = render(&parsed.spec, Target::Claude, &templates);
        assert!(doc.contains("## Templates"));
        assert!(doc.contains("### activity-summary"));
        assert!(doc.contains("- **Lead-in**: summary"));
        assert!(doc.contains("**Known-good examples:**"));
        assert!(doc.contains("## Halcyon"));
        assert!(!doc.contains("<!--spec-->"));
        assert!(!doc.contains("<!--samples-->"));
    }

    #[test]
    fn a_string_literal_call_arg_survives_the_compile_round_trip() {
        // The whole point of ArgRef::Literal (kz-700a): a real endpoint the
        // graph needs has nowhere else to live, since comments are lexer
        // trivia and never reach the AST. A literal arg does, and this
        // proves the compiled output actually carries it, not just source.
        let src = r#"
        spec Foo {
            in: customer: str
            out: y: bool
            flow {
                state HIT_API -> call(Bash, customer, "https://example.com/enrich") -> TERMINATE("done")
            }
        }"#;
        let parsed = parse(src).unwrap();
        let doc = render(&parsed.spec, Target::Claude, &[]);
        assert!(
            doc.contains(r#"call(Bash, customer, "https://example.com/enrich")"#),
            "doc: {doc}"
        );
        let rendered = render_agl_source(&parsed.spec);
        let reparsed = parse(&rendered).expect("re-rendered source should reparse");
        assert_eq!(reparsed.spec, parsed.spec);
    }

    #[test]
    fn claude_target_has_frontmatter_and_primer() {
        let parsed = parse(SAMPLE).unwrap();
        let doc = render(&parsed.spec, Target::Claude, &[]);
        assert!(doc.starts_with("---\n"));
        assert!(doc.contains("name: meeting-prep"));
        assert!(doc.contains("description: \"Runs the meeting-prep AGL graph\""));
        assert!(doc.contains("Execute this graph exactly as written"));
        assert!(doc.contains("FETCH_CALENDAR"));
        assert!(doc.contains("```agl"));
    }

    #[test]
    fn cursor_target_has_no_frontmatter() {
        let parsed = parse(SAMPLE).unwrap();
        let doc = render(&parsed.spec, Target::Cursor, &[]);
        assert!(!doc.starts_with("---\n"));
        assert!(doc.contains("Execute this graph exactly as written"));
        assert!(doc.contains("FETCH_CALENDAR"));
    }

    #[test]
    fn codex_target_has_heading() {
        let parsed = parse(SAMPLE).unwrap();
        let doc = render(&parsed.spec, Target::Codex, &[]);
        assert!(doc.starts_with("## MeetingPrep\n"));
        assert!(doc.contains("Execute this graph exactly as written"));
        assert!(doc.contains("FETCH_CALENDAR"));
    }

    const SAMPLE_WITH_REQUIRES: &str = r#"
    spec MeetingPrep {
      in:  calendar_event: str, slack_channels: list[str]
      out: agenda_update: str
      requires: GoogleCalendar.get, GoogleCalendar.update

      invariant {
        deny: write(calendar) without gate(human_approval)
      }

      flow {
        state FETCH_CALENDAR  -> call(GoogleCalendar.get, calendar_event) -> next
        state PROPOSE_UPDATE  -> gate(human_approval)                     -> EXECUTE_WRITE
        state EXECUTE_WRITE   -> call(GoogleCalendar.update, agenda)      -> TERMINATE("Done")
      }
    }
    "#;

    #[test]
    fn renders_agl_source_with_requires_that_reparses() {
        let parsed = parse(SAMPLE_WITH_REQUIRES).unwrap();
        let rendered = render_agl_source(&parsed.spec);
        assert!(rendered.contains("requires: GoogleCalendar.get, GoogleCalendar.update"));
        let reparsed = parse(&rendered).expect("re-rendered source should reparse");
        assert_eq!(reparsed.spec, parsed.spec);
    }

    #[test]
    fn skill_with_requires_includes_preflight_section() {
        let parsed = parse(SAMPLE_WITH_REQUIRES).unwrap();
        let doc = render(&parsed.spec, Target::Claude, &[]);
        assert!(doc.contains("## Preflight"));
        assert!(doc.contains("- GoogleCalendar.get"));
        assert!(doc.contains("- GoogleCalendar.update"));
        assert!(doc.contains("stop immediately"));
    }

    #[test]
    fn skill_without_requires_omits_preflight_section() {
        let parsed = parse(SAMPLE).unwrap();
        let doc = render(&parsed.spec, Target::Claude, &[]);
        assert!(!doc.contains("## Preflight"));
    }

    #[test]
    fn skill_always_includes_flow_diagram_and_run_order() {
        let parsed = parse(SAMPLE).unwrap();
        let doc = render(&parsed.spec, Target::Claude, &[]);
        assert!(doc.contains("## Before you start"));
        assert!(doc.contains("## Flow"));
        assert!(doc.contains("Do not wait for approval to start"));
    }

    #[test]
    fn sections_appear_in_order_preflight_then_run_order_then_flow_then_source() {
        let parsed = parse(SAMPLE_WITH_REQUIRES).unwrap();
        let doc = render(&parsed.spec, Target::Claude, &[]);
        let preflight = doc.find("## Preflight").expect("preflight section");
        let run_order = doc.find("## Before you start").expect("run-order section");
        let flow = doc.find("## Flow").expect("flow section");
        let source = doc.find("```agl").expect("agl source block");
        assert!(preflight < run_order);
        assert!(run_order < flow);
        assert!(flow < source);
    }

    #[test]
    fn ascii_flow_shows_states_transitions_and_branch_cases() {
        let src = r#"spec Foo {
            in: x: str
            out: y: str
            flow {
                state A -> evaluate(x) -> branch
                branch A {
                    if cond_one -> B
                    if cond_two -> TERMINATE("done")
                }
                state B -> call(Server.method, x) -> TERMINATE("also done")
            }
        }"#;
        let parsed = parse(src).unwrap();
        let diagram = render_ascii_flow(&parsed.spec);
        assert!(diagram.contains("A  evaluate(x)"));
        assert!(diagram.contains("if cond_one -> B"));
        assert!(diagram.contains("if cond_two -> TERMINATE(\"done\")"));
        assert!(diagram.contains("B  call(Server.method, x)"));
    }

    #[test]
    fn resolved_skill_name_defaults_to_kebab_case_of_the_graph_name() {
        let parsed = parse(SAMPLE).unwrap();
        assert_eq!(resolved_skill_name(&parsed.spec), "meeting-prep");
    }

    #[test]
    fn resolved_skill_name_honors_an_explicit_skill_line() {
        let src = r#"spec Foo {
            in: x: str
            out: y: str
            skill: org-chart-sync
            flow {
                state A -> evaluate(x) -> TERMINATE("done")
            }
        }"#;
        let parsed = parse(src).unwrap();
        assert_eq!(resolved_skill_name(&parsed.spec), "org-chart-sync");
    }

    #[test]
    fn skill_line_round_trips_through_render_agl_source() {
        let src = r#"spec Foo {
            in: x: str
            out: y: str
            skill: org-chart-sync
            flow {
                state A -> evaluate(x) -> TERMINATE("done")
            }
        }"#;
        let parsed = parse(src).unwrap();
        let rendered = render_agl_source(&parsed.spec);
        assert!(rendered.contains("skill: org-chart-sync"));
        let reparsed = parse(&rendered).expect("re-rendered source should reparse");
        assert_eq!(reparsed.spec, parsed.spec);
    }

    #[test]
    fn agent_file_has_frontmatter_tools_from_requires_and_the_graph_body() {
        let doc = render_agent_file(&parse(SAMPLE_WITH_REQUIRES).unwrap().spec, &[]);
        assert!(doc.starts_with("---\n"));
        assert!(doc.contains("name: meeting-prep"));
        assert!(doc.contains("tools: GoogleCalendar.get, GoogleCalendar.update"));
        assert!(doc.contains("model: sonnet"));
        assert!(doc.contains("## Preflight"));
        assert!(doc.contains("FETCH_CALENDAR"));
    }

    #[test]
    fn agent_file_omits_tools_line_when_requires_is_empty() {
        let doc = render_agent_file(&parse(SAMPLE).unwrap().spec, &[]);
        assert!(!doc.contains("tools:"));
    }

    #[test]
    fn skill_dispatcher_routes_to_the_subagent_and_does_not_inline_the_graph() {
        let doc = render_skill_dispatcher(&parse(SAMPLE).unwrap().spec);
        assert!(doc.starts_with("---\n"));
        assert!(doc.contains("name: meeting-prep"));
        assert!(doc.contains("subagent_type: \"meeting-prep\""));
        assert!(!doc.contains("## Flow"));
        assert!(!doc.contains("```agl"));
    }
}
