//! Recursive-descent parser: token stream (from `lexer`) -> `AglSpec` AST.
//!
//! `-> branch` (no argument) always resolves to `Branch(<enclosing state's
//! name>)`: the grammar keys a `branch NAME { ... }` block to the state of
//! the same name, so the state itself supplies the branch's identity.

use std::collections::HashMap;
use std::fmt;

use super::ast::{
    AglSpec, ArgRef, BranchBlock, BranchCase, CacheBlock, DataType, InvariantRule, StateAction,
    StateNode, TransitionTarget, TypedParam,
};
use super::lexer::{self, LexError, Tok, TokKind};

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}:{}: {}", self.line, self.col, self.message)
    }
}

impl std::error::Error for ParseError {}

/// A successfully parsed spec plus the source line each state was declared
/// on, so the validator can point at real locations in diagnostics.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    pub spec: AglSpec,
    pub state_lines: HashMap<String, usize>,
    /// Raw `import "..."` strings declared before the `spec` keyword, in
    /// source order. Resolving these into `InvariantRule`s is the resolver
    /// module's job, not the parser's — the parser just records what was
    /// asked for.
    pub imports: Vec<String>,
}

/// An importable fragment file: zero or more leading `import` lines, then
/// zero or more named `cache { ... }` blocks, then an optional top-level
/// `invariant { ... }` block. No `spec` wrapper, no `in`/`out`.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedFragment {
    pub invariants: Vec<InvariantRule>,
    pub cache: Vec<CacheBlock>,
    pub imports: Vec<String>,
}

struct Cursor<'a> {
    toks: &'a [Tok],
    pos: usize,
    src: &'a str,
}

impl<'a> Cursor<'a> {
    fn new(toks: &'a [Tok], src: &'a str) -> Self {
        Cursor { toks, pos: 0, src }
    }

    fn peek(&self) -> Option<&TokKind> {
        self.toks.get(self.pos).map(|t| &t.kind)
    }

    fn offset(&self) -> usize {
        self.toks
            .get(self.pos)
            .map(|t| t.offset)
            .unwrap_or(self.src.len())
    }

    fn err(&self, message: impl Into<String>) -> ParseError {
        let (line, col) = lexer::line_col(self.src, self.offset());
        ParseError {
            message: message.into(),
            line,
            col,
        }
    }

    fn advance(&mut self) -> Option<TokKind> {
        let tok = self.toks.get(self.pos).map(|t| t.kind.clone());
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn expect_punct(&mut self, kind: &TokKind, what: &str) -> Result<(), ParseError> {
        match self.peek() {
            Some(k) if k == kind => {
                self.advance();
                Ok(())
            }
            Some(other) => Err(self.err(format!("expected {what}, found {other:?}"))),
            None => Err(self.err(format!("expected {what}, found end of file"))),
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        match self.peek().cloned() {
            Some(TokKind::Ident(s)) => {
                self.advance();
                Ok(s)
            }
            Some(other) => Err(self.err(format!("expected identifier, found {other:?}"))),
            None => Err(self.err("expected identifier, found end of file")),
        }
    }

    fn expect_string(&mut self) -> Result<String, ParseError> {
        match self.peek().cloned() {
            Some(TokKind::Str(s)) => {
                self.advance();
                Ok(s)
            }
            Some(other) => Err(self.err(format!("expected string literal, found {other:?}"))),
            None => Err(self.err("expected string literal, found end of file")),
        }
    }

    /// Consume a specific bare keyword, e.g. `expect_kw("spec")`.
    fn expect_kw(&mut self, kw: &str) -> Result<(), ParseError> {
        match self.peek().cloned() {
            Some(TokKind::Ident(s)) if s == kw => {
                self.advance();
                Ok(())
            }
            Some(other) => Err(self.err(format!("expected '{kw}', found {other:?}"))),
            None => Err(self.err(format!("expected '{kw}', found end of file"))),
        }
    }

    fn at_punct(&self, kind: &TokKind) -> bool {
        self.peek() == Some(kind)
    }
}

pub fn parse(src: &str) -> Result<Parsed, ParseError> {
    let toks = lex(src)?;
    let mut cur = Cursor::new(&toks, src);
    let imports = parse_import_lines(&mut cur)?;
    let mut parsed = parse_spec(&mut cur)?;
    parsed.imports = imports;
    if cur.peek().is_some() {
        return Err(cur.err("unexpected trailing content after spec"));
    }
    Ok(parsed)
}

/// Parse an importable fragment file: leading `import` lines, then zero or
/// more named `cache { ... }` blocks, then an optional `invariant { ... }`
/// block, in that order, nothing else.
pub fn parse_fragment(src: &str) -> Result<ParsedFragment, ParseError> {
    let toks = lex(src)?;
    let mut cur = Cursor::new(&toks, src);
    let imports = parse_import_lines(&mut cur)?;
    let cache = parse_cache_blocks(&mut cur)?;
    let invariants = if matches!(cur.peek(), Some(TokKind::Ident(s)) if s == "invariant") {
        parse_invariant_block(&mut cur)?
    } else {
        Vec::new()
    };
    if cur.peek().is_some() {
        return Err(cur.err("unexpected trailing content in fragment"));
    }
    Ok(ParsedFragment {
        invariants,
        cache,
        imports,
    })
}

fn lex(src: &str) -> Result<Vec<Tok>, ParseError> {
    lexer::tokenize(src).map_err(|e: LexError| {
        let (line, col) = lexer::line_col(src, e.offset);
        ParseError {
            message: e.message,
            line,
            col,
        }
    })
}

/// Zero or more `import "path/to/fragment.agl"` lines. Shared by both the
/// top-level spec parser and the fragment parser so nested imports use
/// identical syntax.
fn parse_import_lines(cur: &mut Cursor) -> Result<Vec<String>, ParseError> {
    let mut imports = Vec::new();
    while matches!(cur.peek(), Some(TokKind::Ident(s)) if s == "import") {
        cur.expect_kw("import")?;
        imports.push(cur.expect_string()?);
    }
    Ok(imports)
}

fn parse_spec(cur: &mut Cursor) -> Result<Parsed, ParseError> {
    cur.expect_kw("spec")?;
    let name = cur.expect_ident()?;
    cur.expect_punct(&TokKind::LBrace, "'{'")?;

    cur.expect_kw("in")?;
    cur.expect_punct(&TokKind::Colon, "':'")?;
    let inputs = parse_param_list(cur)?;

    cur.expect_kw("out")?;
    cur.expect_punct(&TokKind::Colon, "':'")?;
    let outputs = parse_param_list(cur)?;

    let description = if matches!(cur.peek(), Some(TokKind::Ident(s)) if s == "description") {
        cur.expect_kw("description")?;
        cur.expect_punct(&TokKind::Colon, "':'")?;
        Some(cur.expect_string()?)
    } else {
        None
    };

    let requires = if matches!(cur.peek(), Some(TokKind::Ident(s)) if s == "requires") {
        cur.expect_kw("requires")?;
        cur.expect_punct(&TokKind::Colon, "':'")?;
        parse_dotted_name_list(cur)?
    } else {
        Vec::new()
    };

    let skill = if matches!(cur.peek(), Some(TokKind::Ident(s)) if s == "skill") {
        cur.expect_kw("skill")?;
        cur.expect_punct(&TokKind::Colon, "':'")?;
        Some(cur.expect_ident()?)
    } else {
        None
    };

    let cache = parse_cache_blocks(cur)?;

    let invariants = if matches!(cur.peek(), Some(TokKind::Ident(s)) if s == "invariant") {
        parse_invariant_block(cur)?
    } else {
        Vec::new()
    };

    let (flow, branches, state_lines) = parse_flow_block(cur)?;

    cur.expect_punct(&TokKind::RBrace, "'}'")?;

    Ok(Parsed {
        spec: AglSpec {
            name,
            inputs,
            outputs,
            description,
            requires,
            skill,
            cache,
            invariants,
            flow,
            branches,
        },
        state_lines,
        imports: Vec::new(),
    })
}

/// Zero or more named `cache NAME { field: type, ... }` blocks. Shared by
/// the spec parser and the fragment parser so both use identical syntax -
/// a spec can declare its own inline, a fragment declares one to share.
fn parse_cache_blocks(cur: &mut Cursor) -> Result<Vec<CacheBlock>, ParseError> {
    let mut blocks = Vec::new();
    while matches!(cur.peek(), Some(TokKind::Ident(s)) if s == "cache") {
        cur.expect_kw("cache")?;
        let name = cur.expect_ident()?;
        cur.expect_punct(&TokKind::LBrace, "'{'")?;
        let fields = parse_param_list(cur)?;
        cur.expect_punct(&TokKind::RBrace, "'}'")?;
        blocks.push(CacheBlock { name, fields });
    }
    Ok(blocks)
}

/// Comma-separated dotted `Server.method` names, e.g. the `requires:` line.
/// Reuses `expect_ident` because the lexer already tokenizes a dotted name
/// like `GoogleCalendar.get` as a single `Ident` (see `call(...)` parsing).
fn parse_dotted_name_list(cur: &mut Cursor) -> Result<Vec<String>, ParseError> {
    let mut names = Vec::new();
    loop {
        names.push(cur.expect_ident()?);
        if cur.at_punct(&TokKind::Comma) {
            cur.advance();
            continue;
        }
        break;
    }
    Ok(names)
}

fn parse_param_list(cur: &mut Cursor) -> Result<Vec<TypedParam>, ParseError> {
    let mut params = Vec::new();
    loop {
        let name = cur.expect_ident()?;
        cur.expect_punct(&TokKind::Colon, "':'")?;
        let data_type = parse_type(cur)?;
        params.push(TypedParam { name, data_type });
        if cur.at_punct(&TokKind::Comma) {
            cur.advance();
            continue;
        }
        break;
    }
    Ok(params)
}

fn parse_type(cur: &mut Cursor) -> Result<DataType, ParseError> {
    let name = cur.expect_ident()?;
    Ok(match name.as_str() {
        "str" => DataType::String,
        "int" => DataType::Int,
        "bool" => DataType::Bool,
        "list" => {
            cur.expect_punct(&TokKind::LBracket, "'['")?;
            let inner = parse_type(cur)?;
            cur.expect_punct(&TokKind::RBracket, "']'")?;
            DataType::List(Box::new(inner))
        }
        other => DataType::Custom(other.to_string()),
    })
}

fn parse_invariant_block(cur: &mut Cursor) -> Result<Vec<InvariantRule>, ParseError> {
    cur.expect_kw("invariant")?;
    cur.expect_punct(&TokKind::LBrace, "'{'")?;
    let mut rules = Vec::new();
    while !cur.at_punct(&TokKind::RBrace) {
        if cur.peek().is_none() {
            return Err(cur.err("unterminated invariant block, expected '}'"));
        }
        cur.expect_kw("deny")?;
        cur.expect_punct(&TokKind::Colon, "':'")?;
        rules.push(parse_deny_rule(cur)?);
    }
    cur.expect_punct(&TokKind::RBrace, "'}'")?;
    Ok(rules)
}

fn parse_deny_rule(cur: &mut Cursor) -> Result<InvariantRule, ParseError> {
    let action = cur.expect_ident()?;
    cur.expect_punct(&TokKind::LParen, "'('")?;
    let target = cur.expect_ident()?;
    cur.expect_punct(&TokKind::RParen, "')'")?;
    let keyword = cur.expect_ident()?;
    match keyword.as_str() {
        "without" => {
            cur.expect_kw("gate")?;
            cur.expect_punct(&TokKind::LParen, "'('")?;
            let required_gate = cur.expect_ident()?;
            cur.expect_punct(&TokKind::RParen, "')'")?;
            Ok(InvariantRule::DenyWithoutGate {
                action,
                target,
                required_gate,
            })
        }
        "where" => {
            let condition = consume_raw_phrase_until_stmt_boundary(cur)?;
            Ok(InvariantRule::DenyConstraint {
                action,
                target,
                condition,
            })
        }
        other => Err(cur.err(format!("expected 'without' or 'where', found '{other}'"))),
    }
}

/// Join bare idents into a raw condition string, stopping at the invariant
/// block's closing brace or the next `deny:` rule.
fn consume_raw_phrase_until_stmt_boundary(cur: &mut Cursor) -> Result<String, ParseError> {
    let mut words = Vec::new();
    loop {
        match cur.peek() {
            Some(TokKind::RBrace) => break,
            Some(TokKind::Ident(s)) if s == "deny" => break,
            Some(TokKind::Ident(_)) => {
                if let Some(TokKind::Ident(s)) = cur.advance() {
                    words.push(s);
                }
            }
            Some(other) => return Err(cur.err(format!("unexpected token {other:?} in condition"))),
            None => return Err(cur.err("unexpected end of file in condition")),
        }
    }
    if words.is_empty() {
        return Err(cur.err("expected a condition after 'where'"));
    }
    Ok(words.join(" "))
}

/// Join bare idents into a raw expression string, stopping at `)`.
fn consume_raw_phrase_until_rparen(cur: &mut Cursor) -> Result<String, ParseError> {
    let mut words = Vec::new();
    loop {
        match cur.peek() {
            Some(TokKind::RParen) => break,
            Some(TokKind::Ident(_)) => {
                if let Some(TokKind::Ident(s)) = cur.advance() {
                    words.push(s);
                }
            }
            Some(other) => return Err(cur.err(format!("unexpected token {other:?} in expression"))),
            None => return Err(cur.err("unexpected end of file in expression")),
        }
    }
    if words.is_empty() {
        return Err(cur.err("expected an expression inside '(...)'"));
    }
    Ok(words.join(" "))
}

/// Join bare idents into a condition string, stopping at `->`.
fn consume_raw_phrase_until_arrow(cur: &mut Cursor) -> Result<String, ParseError> {
    let mut words = Vec::new();
    loop {
        match cur.peek() {
            Some(TokKind::Arrow) => break,
            Some(TokKind::Ident(_)) => {
                if let Some(TokKind::Ident(s)) = cur.advance() {
                    words.push(s);
                }
            }
            Some(other) => return Err(cur.err(format!("unexpected token {other:?} in condition"))),
            None => return Err(cur.err("unexpected end of file in condition")),
        }
    }
    if words.is_empty() {
        return Err(cur.err("expected a condition after 'if'"));
    }
    Ok(words.join(" "))
}

type FlowParts = (
    Vec<StateNode>,
    HashMap<String, BranchBlock>,
    HashMap<String, usize>,
);

fn parse_flow_block(cur: &mut Cursor) -> Result<FlowParts, ParseError> {
    cur.expect_kw("flow")?;
    cur.expect_punct(&TokKind::LBrace, "'{'")?;

    let mut flow = Vec::new();
    let mut branches = HashMap::new();
    let mut state_lines = HashMap::new();

    loop {
        match cur.peek() {
            Some(TokKind::Ident(s)) if s == "state" => {
                let name_offset_line;
                let node = {
                    cur.expect_kw("state")?;
                    let name_offset = cur.offset();
                    name_offset_line = lexer::line_col(cur.src, name_offset).0;
                    let name = cur.expect_ident()?;
                    cur.expect_punct(&TokKind::Arrow, "'->'")?;
                    let action = parse_state_action(cur)?;
                    cur.expect_punct(&TokKind::Arrow, "'->'")?;
                    let transition = parse_transition_target(cur, &name)?;
                    StateNode {
                        name,
                        action,
                        transition,
                    }
                };
                state_lines.insert(node.name.clone(), name_offset_line);
                flow.push(node);
            }
            Some(TokKind::Ident(s)) if s == "branch" => {
                let (key, block) = parse_branch_block(cur)?;
                branches.insert(key, block);
            }
            Some(TokKind::RBrace) => break,
            Some(other) => {
                return Err(cur.err(format!("expected 'state' or 'branch', found {other:?}")))
            }
            None => return Err(cur.err("unterminated flow block, expected '}'")),
        }
    }

    cur.expect_punct(&TokKind::RBrace, "'}'")?;
    Ok((flow, branches, state_lines))
}

fn parse_state_action(cur: &mut Cursor) -> Result<StateAction, ParseError> {
    let kw = cur.expect_ident()?;
    match kw.as_str() {
        "call" => {
            cur.expect_punct(&TokKind::LParen, "'('")?;
            let function = cur.expect_ident()?;
            cur.expect_punct(&TokKind::Comma, "','")?;
            let args = parse_call_args(cur)?;
            cur.expect_punct(&TokKind::RParen, "')'")?;
            Ok(StateAction::Call { function, args })
        }
        "map" => {
            cur.expect_punct(&TokKind::LParen, "'('")?;
            let function = cur.expect_ident()?;
            cur.expect_punct(&TokKind::Comma, "','")?;
            let iterable = cur.expect_ident()?;
            cur.expect_punct(&TokKind::RParen, "')'")?;
            Ok(StateAction::Map { function, iterable })
        }
        "evaluate" => {
            cur.expect_punct(&TokKind::LParen, "'('")?;
            let expression = consume_raw_phrase_until_rparen(cur)?;
            cur.expect_punct(&TokKind::RParen, "')'")?;
            Ok(StateAction::Evaluate { expression })
        }
        "gate" => {
            cur.expect_punct(&TokKind::LParen, "'('")?;
            let gate_name = cur.expect_ident()?;
            cur.expect_punct(&TokKind::RParen, "')'")?;
            Ok(StateAction::Gate { gate_name })
        }
        "fan" => {
            cur.expect_punct(&TokKind::LParen, "'('")?;
            let spec_name = cur.expect_ident()?;
            cur.expect_punct(&TokKind::Comma, "','")?;
            let iterable = parse_one_arg_ref(cur)?;
            cur.expect_punct(&TokKind::RParen, "')'")?;
            Ok(StateAction::Fan {
                spec_name,
                iterable,
            })
        }
        "watch" => {
            cur.expect_punct(&TokKind::LParen, "'('")?;
            let condition = consume_raw_phrase_until_rparen(cur)?;
            cur.expect_punct(&TokKind::RParen, "')'")?;
            Ok(StateAction::Watch { condition })
        }
        other => Err(cur.err(format!(
            "unknown action '{other}', expected one of call/map/evaluate/gate/fan/watch"
        ))),
    }
}

/// `call(...)` arguments: comma-separated, each either a bare ident (a
/// variable reference, `ArgRef::Var`) or a quoted string literal (config
/// data with no other home in the grammar, `ArgRef::Literal`).
fn parse_call_args(cur: &mut Cursor) -> Result<Vec<ArgRef>, ParseError> {
    let mut args = Vec::new();
    if cur.at_punct(&TokKind::RParen) {
        return Ok(args);
    }
    loop {
        args.push(parse_one_arg_ref(cur)?);
        if cur.at_punct(&TokKind::Comma) {
            cur.advance();
            continue;
        }
        break;
    }
    Ok(args)
}

/// One `call()`-style argument: a bare ident (`ArgRef::Var`) or a quoted
/// string literal (`ArgRef::Literal`). Factored out of `parse_call_args` so
/// `fan()`'s single second argument (a collection variable, or a quoted
/// count when there's nothing to iterate over, just a bound) can reuse the
/// exact same Var-vs-Literal rule instead of a parallel implementation.
fn parse_one_arg_ref(cur: &mut Cursor) -> Result<ArgRef, ParseError> {
    if matches!(cur.peek(), Some(TokKind::Str(_))) {
        Ok(ArgRef::Literal(cur.expect_string()?))
    } else {
        Ok(ArgRef::Var(cur.expect_ident()?))
    }
}

fn parse_transition_target(
    cur: &mut Cursor,
    state_name: &str,
) -> Result<TransitionTarget, ParseError> {
    let kw = cur.expect_ident()?;
    Ok(match kw.as_str() {
        "next" => TransitionTarget::Next,
        "branch" => TransitionTarget::Branch(state_name.to_string()),
        "TERMINATE" => {
            cur.expect_punct(&TokKind::LParen, "'('")?;
            let msg = cur.expect_string()?;
            cur.expect_punct(&TokKind::RParen, "')'")?;
            TransitionTarget::Terminate(msg)
        }
        other => TransitionTarget::Goto(other.to_string()),
    })
}

fn parse_branch_block(cur: &mut Cursor) -> Result<(String, BranchBlock), ParseError> {
    cur.expect_kw("branch")?;
    let state_name = cur.expect_ident()?;
    cur.expect_punct(&TokKind::LBrace, "'{'")?;
    let mut cases = Vec::new();
    while !cur.at_punct(&TokKind::RBrace) {
        if cur.peek().is_none() {
            return Err(cur.err("unterminated branch block, expected '}'"));
        }
        cur.expect_kw("if")?;
        let condition = consume_raw_phrase_until_arrow(cur)?;
        cur.expect_punct(&TokKind::Arrow, "'->'")?;
        let target = parse_transition_target(cur, &state_name)?;
        cases.push(BranchCase { condition, target });
    }
    cur.expect_punct(&TokKind::RBrace, "'}'")?;
    Ok((state_name.clone(), BranchBlock { state_name, cases }))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn parses_the_canonical_sample() {
        let parsed = parse(SAMPLE).expect("sample should parse");
        assert_eq!(parsed.spec.name, "MeetingPrep");
        assert_eq!(parsed.spec.inputs.len(), 2);
        assert_eq!(parsed.spec.outputs.len(), 1);
        assert_eq!(parsed.spec.invariants.len(), 2);
        assert_eq!(parsed.spec.flow.len(), 5);
        assert_eq!(parsed.spec.branches.len(), 1);
        assert!(parsed.state_lines.contains_key("FETCH_CALENDAR"));

        assert_eq!(
            parsed.spec.inputs[1].data_type,
            DataType::List(Box::new(DataType::String))
        );

        match &parsed.spec.invariants[0] {
            InvariantRule::DenyWithoutGate {
                action,
                target,
                required_gate,
            } => {
                assert_eq!(action, "write");
                assert_eq!(target, "calendar");
                assert_eq!(required_gate, "human_approval");
            }
            other => panic!("unexpected rule: {other:?}"),
        }

        match &parsed.spec.invariants[1] {
            InvariantRule::DenyConstraint {
                action,
                target,
                condition,
            } => {
                assert_eq!(action, "fetch");
                assert_eq!(target, "slack");
                assert_eq!(condition, "channel NOT IN slack_channels");
            }
            other => panic!("unexpected rule: {other:?}"),
        }

        let diff_state = &parsed.spec.flow[2];
        assert_eq!(diff_state.name, "DIFF_AGENDA");
        assert_eq!(
            diff_state.transition,
            TransitionTarget::Branch("DIFF_AGENDA".into())
        );

        let branch = &parsed.spec.branches["DIFF_AGENDA"];
        assert_eq!(branch.cases.len(), 2);
        assert_eq!(branch.cases[0].condition, "no_diff");
        assert_eq!(
            branch.cases[0].target,
            TransitionTarget::Terminate("Already up to date".into())
        );
        assert_eq!(
            branch.cases[1].target,
            TransitionTarget::Goto("PROPOSE_UPDATE".into())
        );
    }

    #[test]
    fn missing_spec_keyword_errors() {
        let err = parse("Foo { in: out: flow: {} }").unwrap_err();
        assert!(err.message.contains("spec"), "{}", err.message);
        assert_eq!(err.line, 1);
    }

    #[test]
    fn missing_in_block_errors() {
        let src = "spec Foo { out: x: str flow { state A -> gate(g) -> TERMINATE(\"done\") } }";
        let err = parse(src).unwrap_err();
        assert!(err.message.contains("'in'"), "{}", err.message);
    }

    #[test]
    fn missing_flow_block_errors() {
        let src = "spec Foo { in: x: str out: y: str }";
        let err = parse(src).unwrap_err();
        assert!(err.message.contains("'flow'"), "{}", err.message);
    }

    #[test]
    fn unterminated_brace_errors() {
        let src = "spec Foo { in: x: str out: y: str flow { state A -> gate(g) -> TERMINATE(\"d\")";
        let err = parse(src).unwrap_err();
        assert!(
            err.message.contains("end of file") || err.message.contains("unterminated"),
            "{}",
            err.message
        );
    }

    #[test]
    fn malformed_identifier_errors() {
        // Identifiers cannot start with a digit.
        let src =
            "spec 9Bad { in: x: str out: y: str flow { state A -> gate(g) -> TERMINATE(\"d\") } }";
        let err = parse(src).unwrap_err();
        assert!(
            err.message.contains("unexpected character"),
            "{}",
            err.message
        );
    }

    #[test]
    fn unknown_action_kind_errors() {
        let src = "spec Foo { in: x: str out: y: str flow { state A -> yeet(x) -> next } }";
        let err = parse(src).unwrap_err();
        assert!(err.message.contains("unknown action"), "{}", err.message);
    }

    #[test]
    fn branch_target_referencing_missing_key_still_parses() {
        // Parser doesn't validate that branch targets exist — that's the validator's job.
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
        let parsed = parse(src).expect("should parse despite no downstream validation");
        assert_eq!(parsed.spec.branches.len(), 1);
    }

    #[test]
    fn parses_leading_import_lines() {
        let src = r#"
        import "shared/human_approval.agl"
        import "shared/other.agl"
        spec Foo {
            in: x: str
            out: y: str
            flow {
                state A -> evaluate(x) -> TERMINATE("done")
            }
        }"#;
        let parsed = parse(src).expect("should parse with leading imports");
        assert_eq!(
            parsed.imports,
            vec![
                "shared/human_approval.agl".to_string(),
                "shared/other.agl".to_string(),
            ]
        );
    }

    #[test]
    fn spec_without_imports_has_empty_import_list() {
        let parsed = parse(SAMPLE).unwrap();
        assert!(parsed.imports.is_empty());
    }

    #[test]
    fn parses_a_fragment_with_only_an_invariant_block() {
        let src = r#"invariant {
            deny: write(hubspot) without gate(human_approval)
        }"#;
        let fragment = parse_fragment(src).expect("fragment should parse");
        assert_eq!(fragment.invariants.len(), 1);
        assert!(fragment.imports.is_empty());
    }

    #[test]
    fn parses_a_fragment_with_nested_imports() {
        let src = r#"
        import "other.agl"
        invariant {
            deny: write(hubspot) without gate(human_approval)
        }"#;
        let fragment = parse_fragment(src).expect("fragment should parse");
        assert_eq!(fragment.imports, vec!["other.agl".to_string()]);
    }

    #[test]
    fn fragment_rejects_a_spec_wrapper() {
        // invariant is now optional in a fragment (cache-only fragments are
        // valid), so a stray `spec` wrapper surfaces as trailing content
        // rather than a missing-invariant error.
        let err = parse_fragment("spec Foo { in: x: str out: y: str flow {} }").unwrap_err();
        assert!(err.message.contains("trailing content"), "{}", err.message);
    }

    #[test]
    fn call_accepts_a_mix_of_var_and_string_literal_args() {
        let src = r#"
        spec Foo {
            in: x: str
            out: y: str
            flow {
                state A -> call(Bash, x, "https://example.com/api", customer) -> TERMINATE("done")
            }
        }"#;
        let parsed = parse(src).expect("should parse literal + var args");
        let StateAction::Call { args, .. } = &parsed.spec.flow[0].action else {
            panic!("expected a Call action");
        };
        assert_eq!(
            args,
            &vec![
                ArgRef::Var("x".to_string()),
                ArgRef::Literal("https://example.com/api".to_string()),
                ArgRef::Var("customer".to_string()),
            ]
        );
    }

    #[test]
    fn parses_a_requires_line() {
        let src = r#"
        spec Foo {
            in: x: str
            out: y: str
            requires: HubSpot.update_contact, TechnicalSuccessHub.write_page

            flow {
                state A -> evaluate(x) -> TERMINATE("done")
            }
        }"#;
        let parsed = parse(src).expect("should parse with a requires line");
        assert_eq!(
            parsed.spec.requires,
            vec![
                "HubSpot.update_contact".to_string(),
                "TechnicalSuccessHub.write_page".to_string(),
            ]
        );
    }

    #[test]
    fn spec_without_requires_has_empty_requires_list() {
        let parsed = parse(SAMPLE).unwrap();
        assert!(parsed.spec.requires.is_empty());
    }

    #[test]
    fn parses_an_explicit_skill_name() {
        let src = r#"
        spec Foo {
            in: x: str
            out: y: str
            requires: HubSpot.update_contact
            skill: org-chart-sync

            flow {
                state A -> evaluate(x) -> TERMINATE("done")
            }
        }"#;
        let parsed = parse(src).expect("should parse with a skill line");
        assert_eq!(parsed.spec.skill, Some("org-chart-sync".to_string()));
    }

    #[test]
    fn spec_without_skill_line_has_none() {
        let parsed = parse(SAMPLE).unwrap();
        assert_eq!(parsed.spec.skill, None);
    }

    #[test]
    fn parses_multiple_named_cache_blocks() {
        let src = r#"
        spec Foo {
            in: x: str
            out: y: str

            cache slack-lookups {
                customer: str,
                int_channel: str
            }
            cache call-prep-timestamps {
                customer: str,
                last_call_date: str
            }

            flow {
                state A -> evaluate(x) -> TERMINATE("done")
            }
        }"#;
        let parsed = parse(src).expect("should parse two named cache blocks");
        assert_eq!(parsed.spec.cache.len(), 2);
        assert_eq!(parsed.spec.cache[0].name, "slack-lookups");
        assert_eq!(
            parsed.spec.cache[0].fields,
            vec![
                TypedParam {
                    name: "customer".to_string(),
                    data_type: DataType::String
                },
                TypedParam {
                    name: "int_channel".to_string(),
                    data_type: DataType::String
                },
            ]
        );
        assert_eq!(parsed.spec.cache[1].name, "call-prep-timestamps");
    }

    #[test]
    fn spec_without_cache_blocks_has_empty_cache() {
        let parsed = parse(SAMPLE).unwrap();
        assert!(parsed.spec.cache.is_empty());
    }

    #[test]
    fn fragment_can_declare_a_cache_block_with_no_invariant() {
        let src = r#"cache slack-lookups {
            customer: str,
            int_channel: str
        }"#;
        let fragment = parse_fragment(src).expect("cache-only fragment should parse");
        assert_eq!(fragment.cache.len(), 1);
        assert_eq!(fragment.cache[0].name, "slack-lookups");
        assert!(fragment.invariants.is_empty());
    }

    #[test]
    fn fragment_can_declare_both_cache_and_invariant() {
        let src = r#"cache slack-lookups {
            customer: str
        }
        invariant {
            deny: write(hubspot) without gate(human_approval)
        }"#;
        let fragment = parse_fragment(src).expect("cache + invariant fragment should parse");
        assert_eq!(fragment.cache.len(), 1);
        assert_eq!(fragment.invariants.len(), 1);
    }
}
