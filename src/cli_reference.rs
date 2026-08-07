//! Generates the CLI command reference from clap's own `--help` metadata,
//! and keeps README.md's embedded copy of it in sync. The reference isn't
//! hand-maintained: it's derived straight from doc comments on `Command`
//! and its nested `Subcommand` enums in `main.rs`, so drift between the CLI
//! and its docs shows up as a diff, not a stale README nobody notices.

use anyhow::{bail, Context, Result};
use clap::CommandFactory;

use crate::Cli;

const START_MARKER: &str = "<!-- CLI_REFERENCE:START -->";
const END_MARKER: &str = "<!-- CLI_REFERENCE:END -->";

/// Walks the full `clap::Command` tree and renders it as Markdown. Public
/// so both the no-flag print path and `write_or_check`'s diff path share
/// one source of truth for what "the reference" actually is.
pub fn generate() -> String {
    let root = Cli::command();
    let mut out = String::new();
    walk(&root, "kazam", &mut out);
    out
}

fn walk(cmd: &clap::Command, path: &str, out: &mut String) {
    let depth = path.split(' ').count();
    let heading = "#".repeat((depth + 2).min(6));
    out.push_str(&format!("{heading} `{path}`\n\n"));

    if let Some(about) = cmd.get_about() {
        out.push_str(&about.to_string());
        out.push_str("\n\n");
    }

    let positionals: Vec<&clap::Arg> = cmd
        .get_positionals()
        .filter(|a| a.get_id() != "command")
        .collect();
    if !positionals.is_empty() {
        for arg in &positionals {
            let help = arg
                .get_help()
                .map(|h| h.to_string())
                .unwrap_or_else(|| "*(undocumented)*".to_string());
            out.push_str(&format!("- `{}` - {help}\n", arg.get_id()));
        }
        out.push('\n');
    }

    let flags: Vec<&clap::Arg> = cmd
        .get_arguments()
        .filter(|a| !a.is_positional())
        .filter(|a| !matches!(a.get_id().as_str(), "help" | "version"))
        .collect();
    if !flags.is_empty() {
        out.push_str("| Flag | Default | Description |\n");
        out.push_str("|---|---|---|\n");
        for arg in &flags {
            let mut names = Vec::new();
            if let Some(l) = arg.get_long() {
                names.push(format!("--{l}"));
            }
            if let Some(s) = arg.get_short() {
                names.push(format!("-{s}"));
            }
            let name = names.join(", ");
            let default = arg
                .get_default_values()
                .first()
                .map(|v| format!("`{}`", v.to_string_lossy()))
                .unwrap_or_default();
            let help = arg
                .get_help()
                .map(|h| h.to_string())
                .unwrap_or_else(|| "*(undocumented)*".to_string());
            out.push_str(&format!("| `{name}` | {default} | {help} |\n"));
        }
        out.push('\n');
    }

    for sub in cmd.get_subcommands() {
        if sub.get_name() == "help" {
            continue;
        }
        let child_path = format!("{path} {}", sub.get_name());
        walk(sub, &child_path, out);
    }
}

/// `--write` patches README.md's marked block in place. `--check` compares
/// without writing and exits 1 on drift, for CI. Neither flag just prints
/// `generate()`'s output, useful for eyeballing or piping.
pub fn write_or_check(write: bool, check: bool) -> Result<()> {
    if !write && !check {
        print!("{}", generate());
        return Ok(());
    }

    let readme_path = "README.md";
    let readme = std::fs::read_to_string(readme_path)
        .with_context(|| format!("failed to read {readme_path}"))?;

    let start = readme.find(START_MARKER).with_context(|| {
        format!("{readme_path} is missing {START_MARKER} - add it where the reference should live")
    })?;
    let end = readme.find(END_MARKER).with_context(|| {
        format!("{readme_path} is missing {END_MARKER} - add it where the reference should live")
    })?;
    if end < start {
        bail!("{END_MARKER} appears before {START_MARKER} in {readme_path}");
    }

    let current_block = &readme[start + START_MARKER.len()..end];
    let generated = generate();
    let expected_block = format!("\n\n{generated}\n");

    if check {
        if current_block == expected_block {
            println!("README.md's CLI reference is up to date");
            return Ok(());
        }
        bail!(
            "README.md's CLI reference is stale - run `kazam cli-reference --write` and commit the diff"
        );
    }

    let new_readme = format!(
        "{}{}{}{}{}",
        &readme[..start],
        START_MARKER,
        expected_block,
        END_MARKER,
        &readme[end + END_MARKER.len()..]
    );
    std::fs::write(readme_path, new_readme)
        .with_context(|| format!("failed to write {readme_path}"))?;
    println!("README.md's CLI reference updated");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_includes_every_top_level_command() {
        let out = generate();
        for name in ["build", "dev", "init", "track", "ctx", "cli-reference"] {
            assert!(
                out.contains(&format!("`kazam {name}`")),
                "missing {name} in generated reference"
            );
        }
    }

    #[test]
    fn generate_recurses_into_nested_subcommands() {
        let out = generate();
        assert!(out.contains("`kazam wish list`"));
        assert!(out.contains("`kazam theme css`"));
    }

    #[test]
    fn generate_skips_the_auto_added_help_subcommand_and_flags() {
        let out = generate();
        assert!(!out.contains("`kazam help`"));
        assert!(!out.contains("| `--help"));
        assert!(!out.contains("| `--version"));
    }

    #[test]
    fn generate_is_deterministic() {
        assert_eq!(generate(), generate());
    }
}
