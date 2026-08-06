//! End-to-end tests for `kazam agl skill`.

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

// Same fixture as the Feature 1 import tests: the imported fragment supplies
// the missing gate for a write the spec's own flow reaches.
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

fn setup_spec() -> PathBuf {
    write_file(
        "kazam-agl-skill-tests",
        "human_approval.agl",
        HUMAN_APPROVAL_FRAGMENT,
    );
    write_file(
        "kazam-agl-skill-tests",
        "hubspot_sync.agl",
        SPEC_WITH_GATED_WRITE,
    )
}

#[test]
fn compiles_to_claude_target_with_frontmatter_and_primer() {
    let spec_path = setup_spec();
    let output = Command::new(bin())
        .args(["agl", "skill"])
        .arg(&spec_path)
        .args(["--target", "claude"])
        .output()
        .expect("run kazam agl skill --target claude");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("---\n"), "stdout: {stdout}");
    assert!(stdout.contains("name:"), "stdout: {stdout}");
    assert!(
        stdout.contains("execute this graph exactly as written")
            || stdout.contains("Execute this graph exactly as written"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("SYNC_CONTACT"), "stdout: {stdout}");
    assert!(stdout.contains("APPROVE"), "stdout: {stdout}");
}

#[test]
fn compiles_to_cursor_target_without_frontmatter() {
    let spec_path = setup_spec();
    let output = Command::new(bin())
        .args(["agl", "skill"])
        .arg(&spec_path)
        .args(["--target", "cursor"])
        .output()
        .expect("run kazam agl skill --target cursor");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.starts_with("---\n"), "stdout: {stdout}");
    assert!(
        stdout.contains("Execute this graph exactly as written"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("SYNC_CONTACT"), "stdout: {stdout}");
}

#[test]
fn compiles_to_codex_target_under_a_heading() {
    let spec_path = setup_spec();
    let output = Command::new(bin())
        .args(["agl", "skill"])
        .arg(&spec_path)
        .args(["--target", "codex"])
        .output()
        .expect("run kazam agl skill --target codex");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("## HubSpotSync"), "stdout: {stdout}");
    assert!(
        stdout.contains("Execute this graph exactly as written"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("SYNC_CONTACT"), "stdout: {stdout}");
}

#[test]
fn refuses_to_compile_an_invalid_spec() {
    let spec_path = write_file(
        "kazam-agl-skill-tests",
        "invalid.agl",
        r#"spec Broken {
          in: x: str
          out: y: str
          flow {
            state A -> evaluate(x) -> GHOST
          }
        }"#,
    );

    let output = Command::new(bin())
        .args(["agl", "skill"])
        .arg(&spec_path)
        .args(["--target", "claude"])
        .output()
        .expect("run kazam agl skill --target claude");

    assert!(!output.status.success());
}

#[test]
fn writes_to_a_directory_using_the_kebab_cased_spec_name() {
    let spec_path = setup_spec();
    let out_dir = std::env::temp_dir().join("kazam-agl-skill-tests-out");
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).unwrap();

    let output = Command::new(bin())
        .args(["agl", "skill"])
        .arg(&spec_path)
        .args(["--target", "claude"])
        .arg("--out")
        .arg(&out_dir)
        .output()
        .expect("run kazam agl skill --out <dir>");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let written = out_dir.join("hub-spot-sync.md");
    assert!(written.exists(), "expected {}", written.display());
}
