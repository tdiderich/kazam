//! Deterministic file outlines: one `L<line> <kind> <name>` entry per
//! top-level symbol or heading. Regex per language, no parser dependency.
//! Cheap enough to run on every changed file during a scan, and gives
//! `ctx research` line numbers to cite so agents Read ranges, not whole files.

use regex::Regex;
use std::sync::OnceLock;

const MAX_ENTRIES: usize = 80;
const MAX_NAME: usize = 70;

fn lang_for(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "rs" => "rs",
        "py" => "py",
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => "ts",
        "go" => "go",
        "sh" | "bash" | "zsh" => "sh",
        "md" | "mdx" => "md",
        "yaml" | "yml" | "agl" => "yaml",
        "sql" => "sql",
        _ => return None,
    })
}

fn pattern(lang: &str) -> &'static Regex {
    static RS: OnceLock<Regex> = OnceLock::new();
    static PY: OnceLock<Regex> = OnceLock::new();
    static TS: OnceLock<Regex> = OnceLock::new();
    static GO: OnceLock<Regex> = OnceLock::new();
    static SH: OnceLock<Regex> = OnceLock::new();
    static MD: OnceLock<Regex> = OnceLock::new();
    static YAML: OnceLock<Regex> = OnceLock::new();
    static SQL: OnceLock<Regex> = OnceLock::new();
    let (cell, src) = match lang {
        "rs" => (
            &RS,
            r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(fn|struct|enum|trait|impl|mod|type|const|static|macro_rules!)\s+([A-Za-z_][\w<>:, ]*)",
        ),
        "py" => (&PY, r"^\s*(def|class|async def)\s+(\w+)"),
        "ts" => (
            &TS,
            r"^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?(function|class|interface|type|enum|const)\s+(\w+)",
        ),
        "go" => (&GO, r"^(func|type)\s+(\(?[\w\s\*]*\)?\s*\w+)"),
        "sh" => (&SH, r"^\s*(function\s+)?(\w+)\s*\(\)\s*\{"),
        "md" => (&MD, r"^(#{1,3})\s+(.+)"),
        "yaml" => (&YAML, r"^([A-Za-z_][\w-]*):"),
        _ => (
            &SQL,
            r"(?i)^\s*(CREATE\s+(?:OR\s+REPLACE\s+)?(?:TABLE|VIEW|FUNCTION|INDEX|MATERIALIZED VIEW))\s+(\S+)",
        ),
    };
    cell.get_or_init(|| Regex::new(src).expect("outline regex"))
}

pub fn outline(ext: &str, text: &str) -> Vec<String> {
    let Some(lang) = lang_for(ext) else {
        return vec![];
    };
    let rx = pattern(lang);
    let mut out = Vec::new();
    let mut in_fence = false;
    for (i, line) in text.lines().enumerate() {
        if lang == "md" && line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if let Some(c) = rx.captures(line) {
            let parts: Vec<&str> = c
                .iter()
                .skip(1)
                .flatten()
                .map(|m| m.as_str().trim())
                .filter(|s| !s.is_empty())
                .collect();
            let mut name = parts.join(" ");
            if name.len() > MAX_NAME {
                let cut = (0..=MAX_NAME)
                    .rev()
                    .find(|&n| name.is_char_boundary(n))
                    .unwrap_or(0);
                name.truncate(cut);
            }
            out.push(format!("L{} {}", i + 1, name));
            if out.len() >= MAX_ENTRIES {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_symbols_with_line_numbers() {
        let src = "use x;\n\npub fn scan(p: &Path) {}\nstruct Foo;\npub(crate) enum Bar {}\n";
        assert_eq!(
            outline("rs", src),
            vec!["L3 fn scan", "L4 struct Foo", "L5 enum Bar"]
        );
    }

    #[test]
    fn markdown_skips_fenced_code() {
        let src = "# Title\n```\n# not a heading\n```\n## Section\n";
        assert_eq!(outline("md", src), vec!["L1 # Title", "L5 ## Section"]);
    }

    #[test]
    fn yaml_top_level_keys_only() {
        let src = "title: x\ncomponents:\n  - type: header\n";
        assert_eq!(outline("yaml", src), vec!["L1 title", "L2 components"]);
    }

    #[test]
    fn unknown_extension_is_empty() {
        assert!(outline("png", "whatever").is_empty());
    }
}
