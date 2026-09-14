//! Shape rules: small declarative checks on a component's YAML that catch
//! layouts agents get wrong on a cold start (too many graph nodes without
//! rows, pipeline stages that overflow, box bodies that run long). Rules
//! live in `schema/components.json` under `guidance.<type>.rules` and in a
//! site's `kazam.yaml` under `shape_rules`. They default to warnings: the
//! page still builds, the author gets told what to fix.
//!
//! Expression language, evaluated against the component as `serde_yaml::Value`:
//!
//! ```text
//! nodes > 6                     array length or number compared to a literal
//! !all(nodes, row)              every element has a truthy field
//! any(nodes, width > 160)       some element satisfies a nested predicate
//! words(body) > 120             word count of a string field
//! stages[*].capabilities > 5    `[*]` maps over an array; a comparison on
//!                               the result is true if ANY element matches
//! sum(stages[*].capabilities)   total length (or value) across the matches
//! a && b, a || b, !a, (a)       booleans
//! has(context)                  field present and non-empty
//! ```

use serde::Deserialize;
use serde_yaml::Value;

use crate::validate::ValidationError;

#[derive(Deserialize, Debug, Clone)]
pub struct ShapeRule {
    /// Component type this rule applies to. Optional inside the schema's
    /// per-component block (implied), required in `kazam.yaml`.
    #[serde(default)]
    pub component: Option<String>,
    /// Expression that, when true, fires the rule.
    #[serde(alias = "deny")]
    pub warn: String,
    /// What to tell the author. Written as an instruction, not a complaint.
    pub say: String,
    /// `warning` (default) or `error`. Schema rules are all warnings.
    #[serde(default)]
    pub severity: Option<String>,
}

// ── Expression AST ────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Num(f64),
    Path(Vec<Seg>),
    Words(Vec<Seg>),
    Sum(Vec<Seg>),
    Has(Vec<Seg>),
    Any(Vec<Seg>, Box<Expr>),
    All(Vec<Seg>, Vec<Seg>),
    Cmp(Box<Expr>, Op, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
enum Seg {
    Key(String),
    Star,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Gt,
    Ge,
    Lt,
    Le,
    Eq,
    Ne,
}

// ── Lexer ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Num(f64),
    Op(Op),
    And,
    Or,
    Not,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Star,
    Dot,
    Comma,
}

fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' | '\n' => i += 1,
            '(' => {
                out.push(Tok::LParen);
                i += 1
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1
            }
            '[' => {
                out.push(Tok::LBracket);
                i += 1
            }
            ']' => {
                out.push(Tok::RBracket);
                i += 1
            }
            '*' => {
                out.push(Tok::Star);
                i += 1
            }
            '.' => {
                out.push(Tok::Dot);
                i += 1
            }
            ',' => {
                out.push(Tok::Comma);
                i += 1
            }
            '&' if chars.get(i + 1) == Some(&'&') => {
                out.push(Tok::And);
                i += 2
            }
            '|' if chars.get(i + 1) == Some(&'|') => {
                out.push(Tok::Or);
                i += 2
            }
            '!' if chars.get(i + 1) == Some(&'=') => {
                out.push(Tok::Op(Op::Ne));
                i += 2
            }
            '!' => {
                out.push(Tok::Not);
                i += 1
            }
            '>' if chars.get(i + 1) == Some(&'=') => {
                out.push(Tok::Op(Op::Ge));
                i += 2
            }
            '>' => {
                out.push(Tok::Op(Op::Gt));
                i += 1
            }
            '<' if chars.get(i + 1) == Some(&'=') => {
                out.push(Tok::Op(Op::Le));
                i += 2
            }
            '<' => {
                out.push(Tok::Op(Op::Lt));
                i += 1
            }
            '=' if chars.get(i + 1) == Some(&'=') => {
                out.push(Tok::Op(Op::Eq));
                i += 2
            }
            c if c.is_ascii_digit() => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                out.push(Tok::Num(
                    s.parse().map_err(|_| format!("bad number '{s}'"))?,
                ));
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                out.push(Tok::Ident(chars[start..i].iter().collect()));
            }
            other => return Err(format!("unexpected character '{other}'")),
        }
    }
    Ok(out)
}

// ── Parser ────────────────────────────────────────────

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }
    fn expect(&mut self, want: Tok) -> Result<(), String> {
        match self.next() {
            Some(t) if t == want => Ok(()),
            other => Err(format!("expected {want:?}, found {other:?}")),
        }
    }

    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_and()?;
        while self.peek() == Some(&Tok::Or) {
            self.next();
            let right = self.parse_and()?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_not()?;
        while self.peek() == Some(&Tok::And) {
            self.next();
            let right = self.parse_not()?;
            left = Expr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> Result<Expr, String> {
        if self.peek() == Some(&Tok::Not) {
            self.next();
            return Ok(Expr::Not(Box::new(self.parse_not()?)));
        }
        self.parse_cmp()
    }

    fn parse_cmp(&mut self) -> Result<Expr, String> {
        let left = self.parse_atom()?;
        if let Some(Tok::Op(op)) = self.peek().cloned() {
            self.next();
            let right = self.parse_atom()?;
            return Ok(Expr::Cmp(Box::new(left), op, Box::new(right)));
        }
        Ok(left)
    }

    fn parse_atom(&mut self) -> Result<Expr, String> {
        match self.next() {
            Some(Tok::LParen) => {
                let e = self.parse_or()?;
                self.expect(Tok::RParen)?;
                Ok(e)
            }
            Some(Tok::Num(n)) => Ok(Expr::Num(n)),
            Some(Tok::Ident(name)) => {
                if self.peek() == Some(&Tok::LParen) {
                    self.next();
                    let e = match name.as_str() {
                        "words" => Expr::Words(self.parse_path()?),
                        "sum" => Expr::Sum(self.parse_path()?),
                        "has" => Expr::Has(self.parse_path()?),
                        "any" => {
                            let list = self.parse_path()?;
                            self.expect(Tok::Comma)?;
                            let pred = self.parse_or()?;
                            Expr::Any(list, Box::new(pred))
                        }
                        "all" => {
                            let list = self.parse_path()?;
                            self.expect(Tok::Comma)?;
                            let field = self.parse_path()?;
                            Expr::All(list, field)
                        }
                        other => return Err(format!("unknown function '{other}'")),
                    };
                    self.expect(Tok::RParen)?;
                    return Ok(e);
                }
                let mut segs = vec![Seg::Key(name)];
                segs.extend(self.parse_path_rest()?);
                Ok(Expr::Path(segs))
            }
            other => Err(format!("unexpected token {other:?}")),
        }
    }

    fn parse_path(&mut self) -> Result<Vec<Seg>, String> {
        match self.next() {
            Some(Tok::Ident(name)) => {
                let mut segs = vec![Seg::Key(name)];
                segs.extend(self.parse_path_rest()?);
                Ok(segs)
            }
            other => Err(format!("expected a field path, found {other:?}")),
        }
    }

    fn parse_path_rest(&mut self) -> Result<Vec<Seg>, String> {
        let mut segs = Vec::new();
        loop {
            match self.peek() {
                Some(Tok::Dot) => {
                    self.next();
                    match self.next() {
                        Some(Tok::Ident(k)) => segs.push(Seg::Key(k)),
                        other => return Err(format!("expected field after '.', found {other:?}")),
                    }
                }
                Some(Tok::LBracket) => {
                    self.next();
                    self.expect(Tok::Star)?;
                    self.expect(Tok::RBracket)?;
                    segs.push(Seg::Star);
                }
                _ => return Ok(segs),
            }
        }
    }
}

fn parse(src: &str) -> Result<Expr, String> {
    let toks = lex(src)?;
    let mut p = Parser { toks, pos: 0 };
    let e = p.parse_or()?;
    if p.pos != p.toks.len() {
        return Err(format!("trailing tokens after expression in '{src}'"));
    }
    Ok(e)
}

// ── Evaluation ────────────────────────────────────────

/// Resolve a path against a value. `[*]` fans out, so the result is always
/// a list of matches (possibly empty).
fn resolve<'a>(v: &'a Value, segs: &[Seg]) -> Vec<&'a Value> {
    let mut cur = vec![v];
    for seg in segs {
        let mut next = Vec::new();
        for item in cur {
            match seg {
                Seg::Key(k) => {
                    if let Some(found) = item.get(k.as_str()) {
                        next.push(found);
                    }
                }
                Seg::Star => {
                    if let Some(arr) = item.as_sequence() {
                        next.extend(arr.iter());
                    }
                }
            }
        }
        cur = next;
    }
    cur
}

fn num_of(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::Sequence(s) => Some(s.len() as f64),
        Value::Mapping(m) => Some(m.len() as f64),
        Value::String(s) => s.trim().parse().ok(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.trim().is_empty(),
        Value::Sequence(s) => !s.is_empty(),
        Value::Mapping(m) => !m.is_empty(),
        Value::Tagged(t) => truthy(&t.value),
    }
}

fn word_count(v: &Value) -> f64 {
    match v {
        Value::String(s) => s.split_whitespace().count() as f64,
        _ => 0.0,
    }
}

/// Numeric candidates for a comparison. A path with `[*]` yields one per
/// match; a plain path yields its length or value; `words()` yields counts.
fn numbers(e: &Expr, v: &Value) -> Vec<f64> {
    match e {
        Expr::Num(n) => vec![*n],
        Expr::Path(segs) => resolve(v, segs).into_iter().filter_map(num_of).collect(),
        Expr::Words(segs) => resolve(v, segs).into_iter().map(word_count).collect(),
        Expr::Sum(segs) => vec![resolve(v, segs).into_iter().filter_map(num_of).sum()],
        other => vec![if eval(other, v) { 1.0 } else { 0.0 }],
    }
}

fn cmp(op: Op, a: f64, b: f64) -> bool {
    match op {
        Op::Gt => a > b,
        Op::Ge => a >= b,
        Op::Lt => a < b,
        Op::Le => a <= b,
        Op::Eq => (a - b).abs() < f64::EPSILON,
        Op::Ne => (a - b).abs() >= f64::EPSILON,
    }
}

fn eval(e: &Expr, v: &Value) -> bool {
    match e {
        Expr::Num(n) => *n != 0.0,
        Expr::Path(segs) => resolve(v, segs).into_iter().any(truthy),
        Expr::Words(segs) => resolve(v, segs).into_iter().any(|x| word_count(x) > 0.0),
        Expr::Sum(segs) => resolve(v, segs).into_iter().filter_map(num_of).sum::<f64>() != 0.0,
        Expr::Has(segs) => resolve(v, segs).into_iter().any(truthy),
        Expr::Any(list, pred) => resolve(v, list)
            .into_iter()
            .flat_map(|x| x.as_sequence().map(|s| s.iter()).into_iter().flatten())
            .any(|item| eval(pred, item)),
        Expr::All(list, field) => {
            let items: Vec<&Value> = resolve(v, list)
                .into_iter()
                .flat_map(|x| x.as_sequence().map(|s| s.iter()).into_iter().flatten())
                .collect();
            !items.is_empty()
                && items
                    .iter()
                    .all(|item| resolve(item, field).into_iter().any(truthy))
        }
        Expr::Cmp(l, op, r) => {
            let rs = numbers(r, v);
            let ls = numbers(l, v);
            // Comparison is true when any left candidate matches any right
            // candidate, which is what `stages[*].capabilities > 5` means.
            ls.iter().any(|a| rs.iter().any(|b| cmp(*op, *a, *b)))
        }
        Expr::And(a, b) => eval(a, v) && eval(b, v),
        Expr::Or(a, b) => eval(a, v) || eval(b, v),
        Expr::Not(a) => !eval(a, v),
    }
}

// ── Public API ────────────────────────────────────────

/// Built-in rules from `schema/components.json`, compiled once.
pub fn builtin() -> &'static [Compiled] {
    static CELL: std::sync::OnceLock<Vec<Compiled>> = std::sync::OnceLock::new();
    CELL.get_or_init(|| compile(&crate::sdk::schema_shape_rules()))
}

/// Built-in rules plus a site's `shape_rules`. Site rules without a
/// `component` are dropped with a warning entry so the author notices.
pub fn rules_for(file: &str, site: &[ShapeRule]) -> (Vec<Compiled>, Vec<ValidationError>) {
    let mut out: Vec<Compiled> = builtin()
        .iter()
        .map(|c| Compiled {
            rule: c.rule.clone(),
            expr: c.expr.clone(),
        })
        .collect();
    let mut problems = Vec::new();
    for r in site {
        if r.component.is_none() {
            problems.push(ValidationError::warning(
                file,
                "kazam.yaml.shape_rules",
                "shape_rule",
                format!("shape rule '{}' has no component: and was skipped", r.warn),
                Some("Add component: <type> to the rule.".into()),
            ));
            continue;
        }
        out.extend(compile(std::slice::from_ref(r)));
    }
    (out, problems)
}

/// Compile once so a bad expression is reported as its own validation
/// error instead of silently never firing.
pub struct Compiled {
    pub rule: ShapeRule,
    expr: Result<Expr, String>,
}

pub fn compile(rules: &[ShapeRule]) -> Vec<Compiled> {
    rules
        .iter()
        .map(|r| Compiled {
            rule: r.clone(),
            expr: parse(&r.warn),
        })
        .collect()
}

/// Run every compiled rule whose `component` matches `component_type`
/// against `value` (the component's YAML). `path` is the YAML keypath used
/// in the returned entries.
pub fn check(
    file: &str,
    path: &str,
    component_type: &str,
    value: &Value,
    rules: &[Compiled],
) -> Vec<ValidationError> {
    let mut out = Vec::new();
    for c in rules {
        if c.rule.component.as_deref() != Some(component_type) {
            continue;
        }
        match &c.expr {
            Err(e) => out.push(ValidationError::warning(
                file,
                path,
                "shape_rule",
                format!("shape rule for {component_type} could not be parsed: {e}"),
                Some(format!("Fix the expression: {}", c.rule.warn)),
            )),
            Ok(expr) => {
                if eval(expr, value) {
                    let mut err = ValidationError::warning(
                        file,
                        path,
                        "shape",
                        format!("{component_type}: {}", c.rule.say),
                        Some(format!("rule: {}", c.rule.warn)),
                    );
                    if c.rule.severity.as_deref() == Some(crate::validate::SEVERITY_ERROR) {
                        err.severity = crate::validate::SEVERITY_ERROR.into();
                    }
                    out.push(err);
                }
            }
        }
    }
    out
}

/// Walk a page's components as YAML, mirroring the paths `validate.rs`
/// uses, and run the rules on each one. Containers recurse: section and
/// box `components`, columns, tabs, accordion items, grid children.
pub fn check_page(file: &str, page: &Value, rules: &[Compiled]) -> Vec<ValidationError> {
    let mut out = Vec::new();
    if let Some(comps) = page.get("components").and_then(Value::as_sequence) {
        walk(file, "components", comps, rules, &mut out);
    }
    if let Some(slides) = page.get("slides").and_then(Value::as_sequence) {
        for (si, slide) in slides.iter().enumerate() {
            if let Some(comps) = slide.get("components").and_then(Value::as_sequence) {
                walk(
                    file,
                    &format!("slides[{si}].components"),
                    comps,
                    rules,
                    &mut out,
                );
            }
        }
    }
    out
}

fn walk(
    file: &str,
    prefix: &str,
    comps: &[Value],
    rules: &[Compiled],
    out: &mut Vec<ValidationError>,
) {
    for (i, c) in comps.iter().enumerate() {
        visit(file, &format!("{prefix}[{i}]"), c, rules, out);
    }
}

fn visit(file: &str, path: &str, c: &Value, rules: &[Compiled], out: &mut Vec<ValidationError>) {
    let Some(ty) = c.get("type").and_then(Value::as_str) else {
        return;
    };
    out.extend(check(file, path, ty, c, rules));
    if let Some(inner) = c.get("components").and_then(Value::as_sequence) {
        walk(file, &format!("{path}.components"), inner, rules, out);
    }
    if let Some(cols) = c.get("columns").and_then(Value::as_sequence) {
        for (ci, col) in cols.iter().enumerate() {
            if let Some(inner) = col.as_sequence() {
                walk(file, &format!("{path}.columns[{ci}]"), inner, rules, out);
            }
        }
    }
    if let Some(tabs) = c.get("tabs").and_then(Value::as_sequence) {
        for (ti, tab) in tabs.iter().enumerate() {
            if let Some(inner) = tab.get("components").and_then(Value::as_sequence) {
                walk(
                    file,
                    &format!("{path}.tabs[{ti}].components"),
                    inner,
                    rules,
                    out,
                );
            }
        }
    }
    if let Some(items) = c.get("items").and_then(Value::as_sequence) {
        for (ii, item) in items.iter().enumerate() {
            if let Some(inner) = item.get("components").and_then(Value::as_sequence) {
                walk(
                    file,
                    &format!("{path}.items[{ii}].components"),
                    inner,
                    rules,
                    out,
                );
            }
        }
    }
    if let Some(children) = c.get("children").and_then(Value::as_sequence) {
        for (ci, child) in children.iter().enumerate() {
            if let Some(comp) = child.get("component") {
                visit(
                    file,
                    &format!("{path}.children[{ci}].component"),
                    comp,
                    rules,
                    out,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(yaml: &str) -> Value {
        serde_yaml::from_str(yaml).unwrap()
    }

    fn fires(expr: &str, yaml: &str) -> bool {
        let e = parse(expr).unwrap_or_else(|err| panic!("{expr}: {err}"));
        eval(&e, &v(yaml))
    }

    #[test]
    fn array_length_comparison() {
        assert!(fires("nodes > 6", "nodes: [1,2,3,4,5,6,7]"));
        assert!(!fires("nodes > 6", "nodes: [1,2,3]"));
        assert!(!fires("nodes > 6", "title: x"));
    }

    #[test]
    fn all_and_any_over_lists() {
        let yaml = "nodes:\n  - {id: a, row: 1}\n  - {id: b}\n";
        assert!(!fires("all(nodes, row)", yaml));
        assert!(fires("!all(nodes, row)", yaml));
        assert!(fires("any(nodes, row > 0)", yaml));
        assert!(!fires("any(nodes, width > 160)", yaml));
        assert!(fires("any(nodes, width > 160)", "nodes: [{width: 200}]"));
    }

    #[test]
    fn star_paths_and_words() {
        let yaml = "stages:\n  - capabilities: [1,2,3,4,5,6]\n  - capabilities: [1]\nbody: one two three four\n";
        assert!(fires("stages[*].capabilities > 5", yaml));
        assert!(!fires("stages[*].capabilities > 6", yaml));
        assert!(fires("words(body) > 3", yaml));
        assert!(!fires("words(body) > 4", yaml));
        assert!(fires("has(body) && !has(context)", yaml));
    }

    #[test]
    fn sum_over_star_paths() {
        let yaml = "stages:\n  - capabilities: [1,2,3]\n  - capabilities: [1,2,3]\n  - capabilities: [1,2,3]\n";
        assert!(fires("sum(stages[*].capabilities) > 8", yaml));
        assert!(!fires("sum(stages[*].capabilities) > 9", yaml));
        assert!(fires(
            "!has(height) && stages == 3 && sum(stages[*].capabilities) > 5",
            yaml
        ));
    }

    #[test]
    fn combined_graph_rule() {
        let expr = "nodes > 6 && !all(nodes, row)";
        let seven_no_rows = "nodes: [{id: a},{id: b},{id: c},{id: d},{id: e},{id: f},{id: g}]";
        let seven_rows = "nodes: [{id: a, row: 1},{id: b, row: 1},{id: c, row: 1},{id: d, row: 2},{id: e, row: 2},{id: f, row: 2},{id: g, row: 3}]";
        assert!(fires(expr, seven_no_rows));
        assert!(!fires(expr, seven_rows));
        assert!(!fires(expr, "nodes: [{id: a},{id: b}]"));
    }

    #[test]
    fn parse_errors_surface_as_rule_warnings() {
        let rules = compile(&[ShapeRule {
            component: Some("graph".into()),
            warn: "nodes >".into(),
            say: "x".into(),
            severity: None,
        }]);
        let out = check("f", "components[0]", "graph", &v("type: graph"), &rules);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].error_type, "shape_rule");
        assert!(!out[0].is_error());
    }

    #[test]
    fn check_page_walks_grid_children_and_box_components() {
        let rules = compile(&[ShapeRule {
            component: Some("markdown".into()),
            warn: "words(body) > 2".into(),
            say: "too long".into(),
            severity: None,
        }]);
        let page = v(
            "components:\n  - type: grid\n    columns: 1\n    children:\n      - component:\n          type: box\n          title: t\n          components:\n            - type: markdown\n              body: a b c d\n  - type: markdown\n    body: short\n",
        );
        let out = check_page("f", &page, &rules);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].path,
            "components[0].children[0].component.components[0]"
        );
        assert_eq!(out[0].severity, "warning");
    }
}
