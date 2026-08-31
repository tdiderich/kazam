//! Conservative minifiers - scan by characters (never bytes) so multi-byte
//! UTF-8 content can't cause panics from mid-char slicing.
//!
//! `minify_html` strips HTML comments and collapses whitespace between tags.
//! `<pre>` and `<textarea>` blocks are preserved verbatim. `<style>` and
//! `<script>` inner contents are passed through `minify_css` / `minify_js`.

pub fn minify_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;

    while !rest.is_empty() {
        // HTML comment
        if let Some(stripped) = rest.strip_prefix("<!--") {
            if let Some(end) = stripped.find("-->") {
                rest = &stripped[end + 3..];
                continue;
            }
            break;
        }

        // Preserved tags
        if let Some(consumed) = consume_preserved(rest, "pre") {
            out.push_str(&rest[..consumed]);
            rest = &rest[consumed..];
            continue;
        }
        if let Some(consumed) = consume_preserved(rest, "textarea") {
            out.push_str(&rest[..consumed]);
            rest = &rest[consumed..];
            continue;
        }

        // <style> - minify inner
        if let Some((consumed, rebuilt)) = minify_tag_inner(rest, "style", minify_css) {
            out.push_str(&rebuilt);
            rest = &rest[consumed..];
            continue;
        }

        // <script> - minify inner
        if let Some((consumed, rebuilt)) = minify_tag_inner(rest, "script", minify_js) {
            out.push_str(&rebuilt);
            rest = &rest[consumed..];
            continue;
        }

        // Whitespace collapse
        let first = rest.chars().next().unwrap();
        if first.is_ascii_whitespace() {
            let ws_len: usize = rest
                .chars()
                .take_while(|c| c.is_ascii_whitespace())
                .map(char::len_utf8)
                .sum();
            let after_ws = &rest[ws_len..];
            let prev_is_tag_close = out.ends_with('>');
            let next_is_tag_open = after_ws.starts_with('<');
            if prev_is_tag_close && next_is_tag_open {
                rest = after_ws;
                continue;
            }
            out.push(' ');
            rest = after_ws;
            continue;
        }

        // Copy one char
        out.push(first);
        rest = &rest[first.len_utf8()..];
    }

    out
}

/// If `rest` begins with `<tag ...>`, find the matching `</tag>` and return the
/// number of bytes to copy verbatim (including both tags). Returns None if this
/// isn't actually the start of that tag.
fn consume_preserved(rest: &str, tag: &str) -> Option<usize> {
    if !starts_tag_ci(rest, tag) {
        return None;
    }
    // kazam emits lowercase tags, so search for the literal lowercase close.
    // Using rest.find() directly avoids to_lowercase() which can change byte
    // offsets for multi-byte characters and invalidate the returned position.
    let close = format!("</{}>", tag);
    let pos = rest.find(&close)?;
    Some(pos + close.len())
}

/// If `rest` begins with `<tag ...>`, replace its inner content with
/// `minifier(inner)`. Returns (bytes_consumed, rebuilt_tag_html).
fn minify_tag_inner(
    rest: &str,
    tag: &str,
    minifier: fn(&str) -> String,
) -> Option<(usize, String)> {
    if !starts_tag_ci(rest, tag) {
        return None;
    }
    let open_end = rest.find('>')?;
    let close = format!("</{}>", tag);
    let inner = &rest[open_end + 1..];
    let inner_pos = inner.find(&close)?;
    let inner_content = &inner[..inner_pos];
    let total = open_end + 1 + inner_pos + close.len();

    let mut rebuilt = String::with_capacity(total);
    rebuilt.push_str(&rest[..=open_end]);
    rebuilt.push_str(&minifier(inner_content));
    rebuilt.push_str(&close);
    Some((total, rebuilt))
}

fn starts_tag_ci(rest: &str, tag: &str) -> bool {
    // Compare at the byte level. Tag names are ASCII, so this never lands
    // inside a multi-byte UTF-8 sequence even when `rest` does.
    let open = format!("<{}", tag);
    let open_bytes = open.as_bytes();
    let rest_bytes = rest.as_bytes();
    if rest_bytes.len() < open_bytes.len() {
        return false;
    }
    for i in 0..open_bytes.len() {
        if !rest_bytes[i].eq_ignore_ascii_case(&open_bytes[i]) {
            return false;
        }
    }
    rest_bytes
        .get(open_bytes.len())
        .map(|c| matches!(*c, b' ' | b'\t' | b'\n' | b'\r' | b'>' | b'/'))
        .unwrap_or(false)
}

// ─── CSS ─────────────────────────────────────────────────────────

pub fn minify_css(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;

    while !rest.is_empty() {
        // /* comment */
        if rest.starts_with("/*") {
            match rest[2..].find("*/") {
                Some(pos) => {
                    rest = &rest[2 + pos + 2..];
                    continue;
                }
                None => break,
            }
        }
        let first = rest.chars().next().unwrap();
        if first.is_ascii_whitespace() {
            let ws_len: usize = rest
                .chars()
                .take_while(|c| c.is_ascii_whitespace())
                .map(char::len_utf8)
                .sum();
            let after_ws = &rest[ws_len..];
            let prev = out.chars().last().unwrap_or(' ');
            let next = after_ws.chars().next().unwrap_or(' ');
            if !out.is_empty() && !is_css_delim(prev) && !is_css_delim(next) {
                out.push(' ');
            }
            rest = after_ws;
            continue;
        }
        if is_css_delim(first) {
            while out.ends_with(' ') {
                out.pop();
            }
            out.push(first);
            rest = &rest[first.len_utf8()..];
            continue;
        }
        out.push(first);
        rest = &rest[first.len_utf8()..];
    }

    out.replace(";}", "}")
}

fn is_css_delim(c: char) -> bool {
    matches!(c, '{' | '}' | ':' | ';' | ',' | '>' | '(' | ')' | '+' | '~')
}

// ─── JS ──────────────────────────────────────────────────────────

pub fn minify_js(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;

    while !rest.is_empty() {
        // /* ... */
        if rest.starts_with("/*") {
            match rest[2..].find("*/") {
                Some(pos) => {
                    rest = &rest[2 + pos + 2..];
                    continue;
                }
                None => break,
            }
        }
        // // ...
        if rest.starts_with("//") {
            match rest.find('\n') {
                Some(pos) => {
                    rest = &rest[pos..];
                    continue;
                }
                None => break,
            }
        }
        // Strings
        let first = rest.chars().next().unwrap();
        if first == '"' || first == '\'' || first == '`' {
            let quote = first;
            let mut len = first.len_utf8();
            let mut iter = rest[len..].char_indices();
            let mut escaped = false;
            for (off, c) in iter.by_ref() {
                len = first.len_utf8() + off + c.len_utf8();
                if escaped {
                    escaped = false;
                    continue;
                }
                if c == '\\' {
                    escaped = true;
                    continue;
                }
                if c == quote {
                    break;
                }
            }
            out.push_str(&rest[..len]);
            rest = &rest[len..];
            continue;
        }
        if first.is_ascii_whitespace() {
            let ws_len: usize = rest
                .chars()
                .take_while(|c| c.is_ascii_whitespace())
                .map(char::len_utf8)
                .sum();
            let after_ws = &rest[ws_len..];
            let prev = out.chars().last().unwrap_or(' ');
            let next = after_ws.chars().next().unwrap_or(' ');
            if is_js_word(prev) && is_js_word(next) {
                out.push(' ');
            }
            rest = after_ws;
            continue;
        }
        out.push(first);
        rest = &rest[first.len_utf8()..];
    }

    out
}

fn is_js_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_strips_comments_and_trailing_semicolon() {
        let css = "/* comment */\n.foo {\n  color: red;\n  margin: 0;\n}\n";
        assert_eq!(minify_css(css), ".foo{color:red;margin:0}");
    }

    #[test]
    fn html_collapses_between_tags() {
        let html = "<div>\n  <p>hi</p>\n</div>";
        assert_eq!(minify_html(html), "<div><p>hi</p></div>");
    }

    #[test]
    fn html_preserves_pre_contents() {
        let html = "<div><pre>\n  indented\n</pre></div>";
        let out = minify_html(html);
        assert!(out.contains("  indented"), "got: {}", out);
    }

    #[test]
    fn html_strips_comments() {
        let html = "<div><!-- drop me --><span>x</span></div>";
        let out = minify_html(html);
        assert!(!out.contains("drop me"));
    }

    #[test]
    fn html_preserves_multibyte() {
        let html = "<title>Hello - World</title>";
        let out = minify_html(html);
        assert!(out.contains("-"), "got: {}", out);
    }

    #[test]
    fn js_strips_line_comments() {
        let js = "// comment\nvar x = 1;\nvar y = 2;";
        let out = minify_js(js);
        assert!(!out.contains("comment"));
        assert!(out.contains("var x=1"));
    }

    #[test]
    fn js_preserves_strings() {
        let js = r#"var s = "  spaces  ";"#;
        let out = minify_js(js);
        assert!(out.contains(r#""  spaces  ""#));
    }

    #[test]
    fn css_preserves_multibyte_in_content() {
        let css = ".x::after { content: ' ↕'; }";
        let out = minify_css(css);
        assert!(out.contains("↕"), "got: {}", out);
    }
}
