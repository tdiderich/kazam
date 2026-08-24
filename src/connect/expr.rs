//! Expression parser + evaluator for the `derive` aggregation verb.
//!
//! Grammar is deliberately tiny (see `connectors/CONNECT_SPEC.md`): arithmetic
//! over named values (`typed / total * 100`) and aggregate functions
//! (`sum(asset_count) where bucket in [critical, high]`, `avg(.service_count)`).
//! No variables, no user-defined functions - a fixed grammar, tokenized with
//! `winnow` and parsed with a small hand-rolled recursive-descent parser.

use anyhow::{bail, Result};
use winnow::combinator::alt;
use winnow::error::ModalResult;
use winnow::token::{literal, take_while};
use winnow::Parser;

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Field(String),
    Ident(String),
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn number(input: &mut &str) -> ModalResult<Tok> {
    let s: &str = take_while(1.., |c: char| c.is_ascii_digit() || c == '.').parse_next(input)?;
    s.parse::<f64>()
        .map(Tok::Num)
        .map_err(|_| winnow::error::ErrMode::Backtrack(winnow::error::ContextError::new()))
}

fn field(input: &mut &str) -> ModalResult<Tok> {
    let _ = literal(".").parse_next(input)?;
    let s: &str = take_while(0.., |c: char| is_ident_char(c) || c == '.').parse_next(input)?;
    Ok(Tok::Field(format!(".{}", s)))
}

fn ident(input: &mut &str) -> ModalResult<Tok> {
    let s: &str = take_while(1.., is_ident_char).parse_next(input)?;
    Ok(Tok::Ident(s.to_string()))
}

fn punct(input: &mut &str) -> ModalResult<Tok> {
    alt((
        alt((
            literal("(").value(Tok::LParen),
            literal(")").value(Tok::RParen),
            literal("[").value(Tok::LBracket),
            literal("]").value(Tok::RBracket),
            literal(",").value(Tok::Comma),
        )),
        alt((
            literal("+").value(Tok::Plus),
            literal("-").value(Tok::Minus),
            literal("*").value(Tok::Star),
            literal("/").value(Tok::Slash),
            literal("%").value(Tok::Percent),
        )),
    ))
    .parse_next(input)
}

fn one_tok(input: &mut &str) -> ModalResult<Tok> {
    // number/field before punct/ident so "-3" tokenizes leading '-' as an
    // operator (handled by the parser), and ".foo" doesn't get swallowed by
    // a bare ident rule.
    alt((number, field, punct, ident)).parse_next(input)
}

fn tokenize(src: &str) -> Result<Vec<Tok>> {
    let mut input = src;
    let mut toks = Vec::new();
    loop {
        input = input.trim_start();
        if input.is_empty() {
            break;
        }
        match one_tok(&mut input) {
            Ok(t) => toks.push(t),
            Err(_) => bail!("cannot tokenize derive expr near: {:?}", input),
        }
    }
    Ok(toks)
}

#[derive(Debug, Clone)]
pub enum CallArg {
    Field(String),
    Ident(String),
}

#[derive(Debug, Clone)]
pub struct BucketCond {
    pub values: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Num(f64),
    Ident(String),
    Field(String),
    BinOp(Box<Expr>, char, Box<Expr>),
    Call(String, Box<CallArg>, Option<BucketCond>),
}

/// True if `e` references a raw record field (`.foo`) outside of an
/// aggregate function call. Aggregate calls scan the whole dataset
/// themselves, so a `.field` argument there doesn't make the overall
/// expression "per-row" - only a bare field reference does.
pub fn has_bare_field_ref(e: &Expr) -> bool {
    match e {
        Expr::Field(_) => true,
        Expr::BinOp(l, _, r) => has_bare_field_ref(l) || has_bare_field_ref(r),
        Expr::Num(_) | Expr::Ident(_) | Expr::Call(..) => false,
    }
}

struct P {
    toks: Vec<Tok>,
    pos: usize,
}

impl P {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn bump(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }
    fn expect(&mut self, want: &Tok) -> Result<()> {
        match self.bump() {
            Some(t) if &t == want => Ok(()),
            other => bail!("expected {:?}, got {:?}", want, other),
        }
    }

    fn parse_expr(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_term()?;
        loop {
            match self.peek() {
                Some(Tok::Plus) => {
                    self.bump();
                    let rhs = self.parse_term()?;
                    lhs = Expr::BinOp(Box::new(lhs), '+', Box::new(rhs));
                }
                Some(Tok::Minus) => {
                    self.bump();
                    let rhs = self.parse_term()?;
                    lhs = Expr::BinOp(Box::new(lhs), '-', Box::new(rhs));
                }
                _ => break,
            }
        }
        Ok(lhs)
    }

    fn parse_term(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_factor()?;
        loop {
            match self.peek() {
                Some(Tok::Star) => {
                    self.bump();
                    let rhs = self.parse_factor()?;
                    lhs = Expr::BinOp(Box::new(lhs), '*', Box::new(rhs));
                }
                Some(Tok::Slash) => {
                    self.bump();
                    let rhs = self.parse_factor()?;
                    lhs = Expr::BinOp(Box::new(lhs), '/', Box::new(rhs));
                }
                Some(Tok::Percent) => {
                    self.bump();
                    let rhs = self.parse_factor()?;
                    lhs = Expr::BinOp(Box::new(lhs), '%', Box::new(rhs));
                }
                _ => break,
            }
        }
        Ok(lhs)
    }

    fn parse_factor(&mut self) -> Result<Expr> {
        match self.bump() {
            Some(Tok::Num(n)) => Ok(Expr::Num(n)),
            Some(Tok::Field(f)) => Ok(Expr::Field(f)),
            Some(Tok::Minus) => {
                let e = self.parse_factor()?;
                Ok(Expr::BinOp(Box::new(Expr::Num(0.0)), '-', Box::new(e)))
            }
            Some(Tok::LParen) => {
                let e = self.parse_expr()?;
                self.expect(&Tok::RParen)?;
                Ok(e)
            }
            Some(Tok::Ident(name)) => {
                if self.peek() == Some(&Tok::LParen) {
                    self.bump();
                    let arg = match self.bump() {
                        Some(Tok::Field(f)) => CallArg::Field(f),
                        Some(Tok::Ident(n)) => CallArg::Ident(n),
                        other => bail!("bad call argument: {:?}", other),
                    };
                    self.expect(&Tok::RParen)?;
                    let cond = self.parse_optional_bucket_cond()?;
                    Ok(Expr::Call(name, Box::new(arg), cond))
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            other => bail!("unexpected token in derive expr: {:?}", other),
        }
    }

    /// `where bucket in [a, b, c]` - the only conditional form aggregate
    /// calls support.
    fn parse_optional_bucket_cond(&mut self) -> Result<Option<BucketCond>> {
        if self.peek() != Some(&Tok::Ident("where".to_string())) {
            return Ok(None);
        }
        self.bump(); // where
        match self.bump() {
            Some(Tok::Ident(s)) if s == "bucket" => {}
            other => bail!("expected 'bucket' after 'where', got {:?}", other),
        }
        match self.bump() {
            Some(Tok::Ident(s)) if s == "in" => {}
            other => bail!("expected 'in' after 'bucket', got {:?}", other),
        }
        self.expect(&Tok::LBracket)?;
        let mut values = Vec::new();
        loop {
            match self.bump() {
                Some(Tok::Ident(v)) => values.push(v),
                other => bail!("bad value in bucket condition list: {:?}", other),
            }
            match self.peek() {
                Some(Tok::Comma) => {
                    self.bump();
                }
                _ => break,
            }
        }
        self.expect(&Tok::RBracket)?;
        Ok(Some(BucketCond { values }))
    }
}

pub fn parse(src: &str) -> Result<Expr> {
    let toks = tokenize(src)?;
    let mut p = P { toks, pos: 0 };
    let e = p.parse_expr()?;
    if p.pos != p.toks.len() {
        bail!("trailing tokens after derive expr '{}': {:?}", src, &p.toks[p.pos..]);
    }
    Ok(e)
}

pub trait ExprCtx {
    fn get_ident(&self, name: &str) -> Option<f64>;
    fn get_field(&self, path: &str) -> Option<f64>;
    fn call(&self, func: &str, arg: &CallArg, cond: &Option<BucketCond>) -> f64;
}

pub fn eval(e: &Expr, ctx: &dyn ExprCtx) -> f64 {
    match e {
        Expr::Num(n) => *n,
        Expr::Ident(name) => ctx.get_ident(name).unwrap_or(0.0),
        Expr::Field(path) => ctx.get_field(path).unwrap_or(0.0),
        Expr::BinOp(l, op, r) => {
            let a = eval(l, ctx);
            let b = eval(r, ctx);
            match op {
                '+' => a + b,
                '-' => a - b,
                '*' => a * b,
                '/' => {
                    if b != 0.0 {
                        a / b
                    } else {
                        0.0
                    }
                }
                '%' => {
                    if b != 0.0 {
                        a % b
                    } else {
                        0.0
                    }
                }
                _ => 0.0,
            }
        }
        Expr::Call(func, arg, cond) => ctx.call(func, arg, cond),
    }
}

pub fn apply_func(func: &str, values: &[f64]) -> f64 {
    match func {
        "sum" => values.iter().sum(),
        "count" => values.len() as f64,
        "min" => values.iter().cloned().fold(f64::INFINITY, f64::min),
        "max" => values.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        "avg" => {
            if values.is_empty() {
                0.0
            } else {
                values.iter().sum::<f64>() / values.len() as f64
            }
        }
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeCtx;
    impl ExprCtx for FakeCtx {
        fn get_ident(&self, name: &str) -> Option<f64> {
            match name {
                "typed" => Some(40.0),
                "total" => Some(50.0),
                _ => None,
            }
        }
        fn get_field(&self, _: &str) -> Option<f64> {
            None
        }
        fn call(&self, _: &str, _: &CallArg, _: &Option<BucketCond>) -> f64 {
            0.0
        }
    }

    #[test]
    fn parses_and_evaluates_simple_arithmetic() {
        let e = parse("typed / total * 100").unwrap();
        assert!((eval(&e, &FakeCtx) - 80.0).abs() < 1e-9);
    }

    #[test]
    fn detects_bare_field_refs() {
        let e = parse(".vulnerability_count / .service_count").unwrap();
        assert!(has_bare_field_ref(&e));
        let e2 = parse("avg(.service_count)").unwrap();
        assert!(!has_bare_field_ref(&e2));
    }

    #[test]
    fn parses_aggregate_with_bucket_condition() {
        let e = parse("sum(asset_count) where bucket in [critical, high]").unwrap();
        match e {
            Expr::Call(f, arg, Some(cond)) => {
                assert_eq!(f, "sum");
                assert!(matches!(*arg, CallArg::Ident(ref n) if n == "asset_count"));
                assert_eq!(cond.values, vec!["critical", "high"]);
            }
            other => panic!("unexpected parse result: {:?}", other),
        }
    }
}
