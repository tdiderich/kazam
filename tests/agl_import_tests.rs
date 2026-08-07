//! End-to-end tests for `.agl` `import` resolution (`kazam agl validate`).

use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_kazam"))
}

fn write_file(dir: &str, name: &str, contents: &str) -> PathBuf {
    let d = std::env::temp_dir().join(dir);
    let _ = std::fs::create_dir_all(&d);
    let path = d.join(name);
    std::fs::write(&path, contents).expect("write file");
    path
}

const HUMAN_APPROVAL_FRAGMENT: &str = r#"invariant {
    deny: write(hubspot) without gate(human_approval)
}
"#;

const SPEC_WITH_UNGATED_WRITE: &str = r#"
import "human_approval.agl"

spec HubSpotSync {
  in: contact_id: str
  out: status: str

  flow {
    state SYNC_CONTACT -> call(HubSpot.update_contact, contact_id) -> TERMINATE("done")
  }
}
"#;

const SPEC_WITH_GATED_WRITE: &str = r#"
import "human_approval.agl"

spec HubSpotSync {
  in: contact_id: str
  out: status: str

  flow {
    state APPROVE       -> gate(human_approval)                       -> SYNC_CONTACT
    state SYNC_CONTACT  -> call(HubSpot.update_contact, contact_id)   -> TERMINATE("done")
  }
}
"#;

#[test]
fn imported_invariant_flags_an_ungated_write_reachable_in_the_spec() {
    // Real gap this reproduces: a HubSpot sync state reachable without a
    // gate, where the deny rule lives only in a shared imported fragment —
    // not written inline in the spec itself.
    write_file(
        "kazam-agl-import-tests",
        "human_approval.agl",
        HUMAN_APPROVAL_FRAGMENT,
    );
    let spec_path = write_file(
        "kazam-agl-import-tests",
        "hubspot_sync_ungated.agl",
        SPEC_WITH_UNGATED_WRITE,
    );

    let output = Command::new(bin())
        .args(["agl", "validate"])
        .arg(&spec_path)
        .output()
        .expect("run kazam agl validate");

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("invariant-violation"), "stdout: {stdout}");
}

#[test]
fn missing_import_file_errors_cleanly_not_a_panic() {
    let spec_path = write_file(
        "kazam-agl-import-tests",
        "missing_import.agl",
        r#"
        import "does_not_exist.agl"

        spec Foo {
          in: x: str
          out: y: str
          flow {
            state A -> evaluate(x) -> TERMINATE("done")
          }
        }
        "#,
    );

    let output = Command::new(bin())
        .args(["agl", "validate"])
        .arg(&spec_path)
        .output()
        .expect("run kazam agl validate");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stderr.contains("could not resolve import") || stdout.contains("could not resolve import"),
        "stdout: {stdout}\nstderr: {stderr}"
    );
}

#[test]
fn two_fragments_importing_each_other_error_with_cycle_not_a_hang() {
    write_file(
        "kazam-agl-import-tests",
        "cycle_a.agl",
        r#"import "cycle_b.agl"
        invariant { deny: write(x) without gate(g) }
        "#,
    );
    write_file(
        "kazam-agl-import-tests",
        "cycle_b.agl",
        r#"import "cycle_a.agl"
        invariant { deny: write(y) without gate(g) }
        "#,
    );
    let spec_path = write_file(
        "kazam-agl-import-tests",
        "cycle_spec.agl",
        r#"
        import "cycle_a.agl"

        spec Foo {
          in: x: str
          out: y: str
          flow {
            state A -> evaluate(x) -> TERMINATE("done")
          }
        }
        "#,
    );

    let output = Command::new(bin())
        .args(["agl", "validate"])
        .arg(&spec_path)
        .output()
        .expect("run kazam agl validate");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stderr.contains("import cycle") || stdout.contains("import cycle"),
        "stdout: {stdout}\nstderr: {stderr}"
    );
}

#[test]
fn imported_invariant_passes_once_the_spec_supplies_the_gate() {
    write_file(
        "kazam-agl-import-tests",
        "human_approval.agl",
        HUMAN_APPROVAL_FRAGMENT,
    );
    let spec_path = write_file(
        "kazam-agl-import-tests",
        "hubspot_sync_gated.agl",
        SPEC_WITH_GATED_WRITE,
    );

    let output = Command::new(bin())
        .args(["agl", "validate"])
        .arg(&spec_path)
        .output()
        .expect("run kazam agl validate");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn json_output_reports_resolution_errors() {
    let spec_path = write_file(
        "kazam-agl-import-tests",
        "missing_import_json.agl",
        r#"
        import "does_not_exist_either.agl"

        spec Foo {
          in: x: str
          out: y: str
          flow {
            state A -> evaluate(x) -> TERMINATE("done")
          }
        }
        "#,
    );

    let output = Command::new(bin())
        .args(["agl", "validate", "--json"])
        .arg(&spec_path)
        .output()
        .expect("run kazam agl validate --json");

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON output");
    assert_eq!(parsed["valid"], serde_json::Value::Bool(false));
    assert!(parsed["resolution_error"].is_string(), "stdout: {stdout}");
}
