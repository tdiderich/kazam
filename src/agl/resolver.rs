//! Resolves `import "..."` lines into the `InvariantRule`s they pull in.
//!
//! An imported file ("fragment") holds only a top-level `invariant { ... }`
//! block — see `parser::parse_fragment`. Fragments may themselves `import`
//! other fragments, so resolution is recursive; cycles are detected by
//! tracking canonicalized paths on the current resolution chain.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};

use super::ast::InvariantRule;
use super::parser;

/// Resolve `imports` (as declared by the file at `spec_path`) into the full,
/// transitively-flattened set of `InvariantRule`s they contribute.
pub fn resolve_imports(spec_path: &Path, imports: &[String]) -> Result<Vec<InvariantRule>> {
    let mut chain = Vec::new();
    let mut rules = Vec::new();
    for import in imports {
        resolve_one(spec_path, import, &mut chain, &mut rules)?;
    }
    Ok(rules)
}

/// Resolve a single `import` string relative to `from_path` (the file that
/// declared it), appending its rules (and recursively, its own imports'
/// rules) to `rules`. `chain` holds the canonicalized paths of every
/// fragment currently being resolved, in order, for cycle detection.
fn resolve_one(
    from_path: &Path,
    import: &str,
    chain: &mut Vec<PathBuf>,
    rules: &mut Vec<InvariantRule>,
) -> Result<()> {
    let fragment_path = locate_fragment(from_path, import)?;
    let canonical = fragment_path.canonicalize().with_context(|| {
        format!(
            "failed to resolve import {:?} (found at {})",
            import,
            fragment_path.display()
        )
    })?;

    if let Some(pos) = chain.iter().position(|p| p == &canonical) {
        let mut names: Vec<String> = chain[pos..].iter().map(|p| display_name(p)).collect();
        names.push(display_name(&canonical));
        bail!("import cycle: {}", names.join(" -> "));
    }

    let src = std::fs::read_to_string(&canonical)
        .with_context(|| format!("failed to read imported fragment {}", canonical.display()))?;
    let fragment =
        parser::parse_fragment(&src).map_err(|e| anyhow!("{}: {e}", canonical.display()))?;

    chain.push(canonical.clone());
    for nested_import in &fragment.imports {
        resolve_one(&canonical, nested_import, chain, rules)?;
    }
    chain.pop();

    rules.extend(fragment.invariants);
    Ok(())
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

/// Resolution order: (1) relative to the importing file's own directory,
/// (2) `~/.kazam/agl/shared/<path>`.
fn locate_fragment(from_path: &Path, import: &str) -> Result<PathBuf> {
    let base_dir = from_path.parent().unwrap_or_else(|| Path::new("."));
    let relative = base_dir.join(import);
    if relative.is_file() {
        return Ok(relative);
    }

    let shared = home_dir()?
        .join(".kazam")
        .join("agl")
        .join("shared")
        .join(import);
    if shared.is_file() {
        return Ok(shared);
    }

    bail!(
        "could not resolve import \"{import}\" — checked {} and {}",
        relative.display(),
        shared.display()
    );
}

fn home_dir() -> Result<PathBuf> {
    match std::env::var_os("HOME") {
        Some(h) => Ok(PathBuf::from(h)),
        None => bail!("HOME is not set — cannot resolve ~/.kazam/agl/shared imports"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kazam-agl-resolver-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn resolves_a_simple_local_fragment() {
        let dir = tempdir();
        let fragment_path = dir.join("human_approval.agl");
        fs::write(
            &fragment_path,
            r#"invariant {
                deny: write(hubspot) without gate(human_approval)
            }"#,
        )
        .unwrap();
        let spec_path = dir.join("spec.agl");

        let rules = resolve_imports(&spec_path, &["human_approval.agl".to_string()]).unwrap();
        assert_eq!(rules.len(), 1);
        assert!(
            matches!(&rules[0], InvariantRule::DenyWithoutGate { target, .. } if target == "hubspot")
        );
    }

    #[test]
    fn missing_fragment_errors_clearly() {
        let dir = tempdir();
        let spec_path = dir.join("spec.agl");
        let err = resolve_imports(&spec_path, &["nope.agl".to_string()]).unwrap_err();
        assert!(err.to_string().contains("could not resolve import"));
    }

    #[test]
    fn falls_back_to_the_shared_hub_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let shared_dir = tmp.path().join(".kazam").join("agl").join("shared");
        fs::create_dir_all(&shared_dir).unwrap();
        fs::write(
            shared_dir.join("human_approval.agl"),
            r#"invariant {
                deny: write(hubspot) without gate(human_approval)
            }"#,
        )
        .unwrap();

        // Point HOME at a tempdir for this test only; env is process-global,
        // so restore the prior value afterward (same pattern as install.rs).
        let prev_home = std::env::var_os("HOME");
        std::env::set_var("HOME", tmp.path());

        let spec_dir = tempdir(); // a directory with no local fragment
        let spec_path = spec_dir.join("spec.agl");
        let result = resolve_imports(&spec_path, &["human_approval.agl".to_string()]);

        match prev_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }

        let rules = result.unwrap();
        assert_eq!(rules.len(), 1);
    }

    #[test]
    fn detects_import_cycles() {
        let dir = tempdir();
        let a = dir.join("a.agl");
        let b = dir.join("b.agl");
        fs::write(
            &a,
            r#"import "b.agl"
            invariant { deny: write(x) without gate(g) }"#,
        )
        .unwrap();
        fs::write(
            &b,
            r#"import "a.agl"
            invariant { deny: write(y) without gate(g) }"#,
        )
        .unwrap();

        let spec_path = dir.join("spec.agl");
        let err = resolve_imports(&spec_path, &["a.agl".to_string()]).unwrap_err();
        assert!(err.to_string().contains("import cycle"), "{err}");
    }
}
