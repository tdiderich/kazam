//! Lexer for `.agl` source — turns raw text into a flat token stream.
//!
//! Token recognition uses `winnow` combinators; whitespace/comment skipping
//! is plain string slicing since it carries no grammar meaning of its own.

use winnow::combinator::alt;
use winnow::error::ModalResult;
use winnow::token::{literal, take_while};
use winnow::Parser;

#[derive(Debug, Clone, PartialEq)]
pub enum TokKind {
    Ident(String),
    Str(String),
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Colon,
    Comma,
    Arrow,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tok {
    pub kind: TokKind,
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub message: String,
    pub offset: usize,
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '.' || c == '-'
}

/// `spec`, `FETCH_CALENDAR`, `GoogleCalendar.get`, `technical-success-hub`,
/// `NOT`, `no_diff`, ... — every bare word in the grammar, keywords
/// included; the parser decides what a given ident means from context.
///
/// `-` is a valid ident character (real tool identifiers like
/// `mcp__technical-success-hub__write_page` are kebab-cased), but it's
/// never absorbed when it's actually the start of an immediately-following
/// `->` arrow — `next->TERMINATE(...)`, with no space, must still lex as
/// `Ident("next")`, `Arrow`, ..., not swallow the arrow's `-` into the
/// identifier and choke on a stray `>`. `take_while` can't express that
/// one-token lookahead, so this scans by hand instead.
fn ident(input: &mut &str) -> ModalResult<String> {
    let start: char = winnow::token::one_of(is_ident_start).parse_next(input)?;
    let mut end = 0;
    for (i, c) in input.char_indices() {
        if !is_ident_char(c) {
            break;
        }
        if c == '-' && input[i + 1..].starts_with('>') {
            break;
        }
        end = i + c.len_utf8();
    }
    let rest = &input[..end];
    *input = &input[end..];
    Ok(format!("{start}{rest}"))
}

/// `"..."` — no escape sequences; `.agl` strings are short human messages.
fn string_lit(input: &mut &str) -> ModalResult<String> {
    let _ = literal("\"").parse_next(input)?;
    let body: &str = take_while(0.., |c| c != '"').parse_next(input)?;
    let _ = literal("\"").parse_next(input)?;
    Ok(body.to_string())
}

fn one_token(input: &mut &str) -> ModalResult<TokKind> {
    let punct = alt((
        literal("->").value(TokKind::Arrow),
        literal("{").value(TokKind::LBrace),
        literal("}").value(TokKind::RBrace),
        literal("(").value(TokKind::LParen),
        literal(")").value(TokKind::RParen),
        literal("[").value(TokKind::LBracket),
        literal("]").value(TokKind::RBracket),
        literal(":").value(TokKind::Colon),
        literal(",").value(TokKind::Comma),
    ));
    alt((
        punct,
        string_lit.map(TokKind::Str),
        ident.map(TokKind::Ident),
    ))
    .parse_next(input)
}

/// Strip whitespace and `//` line comments from the front of `input`.
fn skip_trivia(input: &mut &str) {
    loop {
        let start_len = input.len();
        *input = input.trim_start_matches(|c: char| c.is_whitespace());
        if let Some(rest) = input.strip_prefix("//") {
            let cut = rest.find('\n').unwrap_or(rest.len());
            *input = &rest[cut..];
        }
        if input.len() == start_len {
            break;
        }
    }
}

pub fn tokenize(src: &str) -> Result<Vec<Tok>, LexError> {
    let mut input = src;
    let mut toks = Vec::new();
    loop {
        skip_trivia(&mut input);
        if input.is_empty() {
            break;
        }
        let offset = src.len() - input.len();
        let before = input;
        match one_token(&mut input) {
            Ok(kind) => toks.push(Tok { kind, offset }),
            Err(_) => {
                let bad = before.chars().next().unwrap_or('?');
                return Err(LexError {
                    message: format!("unexpected character {:?}", bad),
                    offset,
                });
            }
        }
    }
    Ok(toks)
}

/// Convert a byte offset into a 1-indexed (line, col) pair for diagnostics.
pub fn line_col(src: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, c) in src.char_indices() {
        if i >= offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_punctuation_and_idents() {
        let toks = tokenize("spec Foo { in: x: str }").unwrap();
        assert_eq!(
            toks.iter().map(|t| t.kind.clone()).collect::<Vec<_>>(),
            vec![
                TokKind::Ident("spec".into()),
                TokKind::Ident("Foo".into()),
                TokKind::LBrace,
                TokKind::Ident("in".into()),
                TokKind::Colon,
                TokKind::Ident("x".into()),
                TokKind::Colon,
                TokKind::Ident("str".into()),
                TokKind::RBrace,
            ]
        );
    }

    #[test]
    fn tokenizes_arrow_and_dotted_ident() {
        let toks = tokenize("state X -> call(GoogleCalendar.get, y) -> next").unwrap();
        assert!(toks
            .iter()
            .any(|t| t.kind == TokKind::Ident("GoogleCalendar.get".into())));
        assert_eq!(toks.iter().filter(|t| t.kind == TokKind::Arrow).count(), 2);
    }

    #[test]
    fn skips_comments() {
        let toks = tokenize("// a comment\nspec Foo {} // trailing").unwrap();
        assert_eq!(
            toks.iter().map(|t| t.kind.clone()).collect::<Vec<_>>(),
            vec![
                TokKind::Ident("spec".into()),
                TokKind::Ident("Foo".into()),
                TokKind::LBrace,
                TokKind::RBrace,
            ]
        );
    }

    #[test]
    fn parses_string_literal() {
        let toks = tokenize(r#"TERMINATE("Already up to date")"#).unwrap();
        assert_eq!(toks[2].kind, TokKind::Str("Already up to date".to_string()));
    }

    #[test]
    fn hyphenated_idents_lex_as_a_single_token() {
        let toks = tokenize("call(mcp__technical-success-hub__write_page, x)").unwrap();
        assert!(toks
            .iter()
            .any(|t| t.kind == TokKind::Ident("mcp__technical-success-hub__write_page".into())));
    }

    #[test]
    fn no_space_arrow_after_an_ident_is_not_swallowed_by_the_hyphen_rule() {
        let toks = tokenize(r#"next->TERMINATE("done")"#).unwrap();
        assert_eq!(
            toks.iter().map(|t| t.kind.clone()).collect::<Vec<_>>(),
            vec![
                TokKind::Ident("next".into()),
                TokKind::Arrow,
                TokKind::Ident("TERMINATE".into()),
                TokKind::LParen,
                TokKind::Str("done".into()),
                TokKind::RParen,
            ]
        );
    }

    #[test]
    fn rejects_unknown_character() {
        let err = tokenize("spec Foo { @ }").unwrap_err();
        assert_eq!(err.offset, 11);
    }

    #[test]
    fn line_col_tracks_newlines() {
        let src = "a\nbb\nccc";
        assert_eq!(line_col(src, 0), (1, 1));
        assert_eq!(line_col(src, 2), (2, 1));
        assert_eq!(line_col(src, 7), (3, 3));
    }
}
