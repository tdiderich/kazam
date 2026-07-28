use std::path::Path;

use anyhow::{Context, Result};

pub fn run(path: &Path) -> Result<()> {
    let path = std::fs::canonicalize(path)
        .with_context(|| format!("cannot resolve: {}", path.display()))?;

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "md" | "yaml" | "yml" | "json" => {}
        _ => anyhow::bail!(
            "unsupported format '.{ext}' — kazam show supports .md, .yaml, .yml, .json"
        ),
    }

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read: {}", path.display()))?;

    let file_name = path.file_name().unwrap_or_default().to_string_lossy();

    // Header
    println!(
        "\x1b[36m━━━ {} \x1b[2m({})\x1b[0m\x1b[36m ━━━\x1b[0m",
        file_name, ext
    );
    println!();

    match ext.as_str() {
        "md" => show_markdown(&content),
        "yaml" | "yml" => show_yaml(&content),
        "json" => show_json(&content),
        _ => print!("{content}"),
    }

    println!();
    Ok(())
}

fn show_markdown(content: &str) {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") {
            println!("\x1b[1;36m{}\x1b[0m", line);
        } else if trimmed.starts_with("## ") {
            println!("\x1b[1;33m{}\x1b[0m", line);
        } else if trimmed.starts_with("### ") {
            println!("\x1b[1;37m{}\x1b[0m", line);
        } else if trimmed.starts_with("#### ") || trimmed.starts_with("##### ") {
            println!("\x1b[1m{}\x1b[0m", line);
        } else if trimmed.starts_with("```") {
            println!("\x1b[2m{}\x1b[0m", line);
        } else if trimmed.starts_with("> ") {
            println!("\x1b[2;33m{}\x1b[0m", line);
        } else if trimmed.starts_with("- [ ] ") {
            println!("{}  \x1b[31m☐\x1b[0m {}", leading_ws(line), &trimmed[6..]);
        } else if trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ") {
            println!("{}  \x1b[32m☑\x1b[0m {}", leading_ws(line), &trimmed[6..]);
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            println!("{}  \x1b[2m•\x1b[0m {}", leading_ws(line), &trimmed[2..]);
        } else if trimmed.starts_with("---") || trimmed.starts_with("***") {
            println!("\x1b[2m────────────────────\x1b[0m");
        } else if trimmed.is_empty() {
            println!();
        } else {
            println!("{line}");
        }
    }
}

fn show_yaml(content: &str) {
    for (i, line) in content.lines().enumerate() {
        let ln = format!("{:>4}", i + 1);
        let trimmed = line.trim();

        if trimmed.starts_with('#') {
            println!("\x1b[2m{ln}\x1b[0m  \x1b[2;3m{line}\x1b[0m");
        } else if let Some(colon_pos) = trimmed.find(':') {
            let key = &trimmed[..colon_pos];
            let rest = &trimmed[colon_pos..];
            println!(
                "\x1b[2m{ln}\x1b[0m  {}\x1b[36m{key}\x1b[0m{}",
                leading_ws(line),
                colorize_yaml_value(rest),
            );
        } else if trimmed.starts_with("- ") {
            println!(
                "\x1b[2m{ln}\x1b[0m  {}\x1b[2m-\x1b[0m {}",
                leading_ws(line),
                colorize_yaml_value(&trimmed[2..]),
            );
        } else {
            println!("\x1b[2m{ln}\x1b[0m  {line}");
        }
    }
}

fn show_json(content: &str) {
    // Try to pretty-print if it's valid JSON
    let display = if let Ok(val) = serde_json::from_str::<serde_json::Value>(content) {
        serde_json::to_string_pretty(&val).unwrap_or_else(|_| content.to_string())
    } else {
        content.to_string()
    };

    for (i, line) in display.lines().enumerate() {
        let ln = format!("{:>4}", i + 1);
        println!("\x1b[2m{ln}\x1b[0m  {}", colorize_json_line(line));
    }
}

fn colorize_yaml_value(s: &str) -> String {
    let val = if s.starts_with(": ") {
        &s[2..]
    } else if s == ":" {
        return "\x1b[2m:\x1b[0m".to_string();
    } else {
        return format!("\x1b[2m{s}\x1b[0m");
    };

    let trimmed = val.trim();
    let colored = if trimmed == "true" || trimmed == "false" {
        format!("\x1b[35m{trimmed}\x1b[0m")
    } else if trimmed == "null" || trimmed == "~" {
        format!("\x1b[31m{trimmed}\x1b[0m")
    } else if trimmed.parse::<f64>().is_ok() && !trimmed.is_empty() {
        format!("\x1b[33m{trimmed}\x1b[0m")
    } else if trimmed.starts_with('"') || trimmed.starts_with('\'') {
        format!("\x1b[32m{trimmed}\x1b[0m")
    } else {
        trimmed.to_string()
    };
    format!("\x1b[2m:\x1b[0m {colored}")
}

fn colorize_json_line(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return line.to_string();
    }

    // Simple colorization by checking patterns
    let ws = leading_ws(line);

    if trimmed.starts_with('"') {
        if let Some(colon_pos) = trimmed.find("\": ") {
            let key = &trimmed[..colon_pos + 1];
            let rest = &trimmed[colon_pos + 1..];
            return format!("{ws}\x1b[36m{key}\x1b[0m{}", colorize_json_value(rest));
        }
        if trimmed.ends_with("\",") || trimmed.ends_with('"') {
            return format!("{ws}\x1b[32m{trimmed}\x1b[0m");
        }
    }

    if trimmed == "true," || trimmed == "false," || trimmed == "true" || trimmed == "false" {
        return format!("{ws}\x1b[35m{trimmed}\x1b[0m");
    }

    if trimmed == "null," || trimmed == "null" {
        return format!("{ws}\x1b[31m{trimmed}\x1b[0m");
    }

    if let Some(stripped) = trimmed.strip_suffix(',') {
        if stripped.parse::<f64>().is_ok() {
            return format!("{ws}\x1b[33m{trimmed}\x1b[0m");
        }
    } else if trimmed.parse::<f64>().is_ok() {
        return format!("{ws}\x1b[33m{trimmed}\x1b[0m");
    }

    line.to_string()
}

fn colorize_json_value(s: &str) -> String {
    let rest = s.strip_prefix(": ").unwrap_or(s);
    let (val, trail) = if rest.ends_with(',') {
        (&rest[..rest.len() - 1], ",")
    } else {
        (rest, "")
    };

    let colored = if val == "true" || val == "false" {
        format!("\x1b[35m{val}\x1b[0m")
    } else if val == "null" {
        format!("\x1b[31m{val}\x1b[0m")
    } else if val.starts_with('"') {
        format!("\x1b[32m{val}\x1b[0m")
    } else if val.parse::<f64>().is_ok() && !val.is_empty() {
        format!("\x1b[33m{val}\x1b[0m")
    } else {
        val.to_string()
    };
    format!(": {colored}{trail}")
}

fn leading_ws(line: &str) -> &str {
    let trimmed = line.trim_start();
    &line[..line.len() - trimmed.len()]
}
