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

#[test]
fn load_writes_an_inline_skill_by_default_no_agent_file() {
    // `run_load` reads $HOME/.kazam/agl/specs, so sandbox HOME entirely -
    // this must never touch the real ~/.kazam/agl on the machine running it.
    let fake_home =
        std::env::temp_dir().join(format!("kazam-agl-load-test-home-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&fake_home);
    let specs_dir = fake_home.join(".kazam").join("agl").join("specs");
    std::fs::create_dir_all(&specs_dir).unwrap();
    std::fs::write(
        specs_dir.join("hub-spot-sync.agl"),
        r#"spec HubSpotSync {
            in: contact_id: str
            out: status: str
            requires: HubSpot.update_contact
            skill: sync-hubspot

            invariant { deny: write(hubspot) without gate(human_approval) }

            flow {
                state APPROVE      -> gate(human_approval)                   -> SYNC_CONTACT
                state SYNC_CONTACT -> call(HubSpot.update_contact, contact_id) -> TERMINATE("done")
            }
        }"#,
    )
    .unwrap();

    let project_dir = std::env::temp_dir().join(format!(
        "kazam-agl-load-test-project-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&project_dir);
    std::fs::create_dir_all(&project_dir).unwrap();

    let output = Command::new(bin())
        .args(["agl", "load", "--out"])
        .arg(&project_dir)
        .env("HOME", &fake_home)
        .output()
        .expect("run kazam agl load");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let agent_path = project_dir.join(".claude/agents/sync-hubspot.md");
    let skill_path = project_dir.join(".claude/skills/sync-hubspot.md");
    assert!(
        !agent_path.exists(),
        "default (non---isolated) load must not write an agent file: {}",
        agent_path.display()
    );
    assert!(skill_path.exists(), "expected {}", skill_path.display());

    // The inline skill carries the whole graph itself, gate included - it's
    // meant to run in the invoking session, not dispatch elsewhere.
    let skill_doc = std::fs::read_to_string(&skill_path).unwrap();
    assert!(skill_doc.contains("name: sync-hubspot"));
    assert!(skill_doc.contains("SYNC_CONTACT"));
    assert!(skill_doc.contains("Execute this graph exactly as written"));
    assert!(!skill_doc.contains("subagent_type"));
}

#[test]
fn load_isolated_refuses_a_spec_with_a_gate_protected_write() {
    let fake_home = std::env::temp_dir().join(format!(
        "kazam-agl-load-test-home-isolated-refuse-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&fake_home);
    let specs_dir = fake_home.join(".kazam").join("agl").join("specs");
    std::fs::create_dir_all(&specs_dir).unwrap();
    std::fs::write(
        specs_dir.join("hub-spot-sync.agl"),
        r#"spec HubSpotSync {
            in: contact_id: str
            out: status: str
            requires: HubSpot.update_contact
            skill: sync-hubspot

            invariant { deny: write(hubspot) without gate(human_approval) }

            flow {
                state APPROVE      -> gate(human_approval)                   -> SYNC_CONTACT
                state SYNC_CONTACT -> call(HubSpot.update_contact, contact_id) -> TERMINATE("done")
            }
        }"#,
    )
    .unwrap();

    let project_dir = std::env::temp_dir().join(format!(
        "kazam-agl-load-test-project-isolated-refuse-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&project_dir);
    std::fs::create_dir_all(&project_dir).unwrap();

    let output = Command::new(bin())
        .args(["agl", "load", "--isolated", "--out"])
        .arg(&project_dir)
        .env("HOME", &fake_home)
        .output()
        .expect("run kazam agl load --isolated");

    // This spec has a gate-protected write and nothing else in the hub -
    // it's the only spec, so refusing it means nothing gets loaded at all.
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("refusing to compile"), "stdout: {stdout}");
    assert!(
        !project_dir.join(".claude/agents/sync-hubspot.md").exists(),
        "must not compile a gated spec to a subagent"
    );
    assert!(!project_dir.join(".claude/skills/sync-hubspot.md").exists());
}

#[test]
fn load_isolated_compiles_agent_and_dispatcher_for_a_read_only_spec() {
    let fake_home = std::env::temp_dir().join(format!(
        "kazam-agl-load-test-home-isolated-ok-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&fake_home);
    let specs_dir = fake_home.join(".kazam").join("agl").join("specs");
    std::fs::create_dir_all(&specs_dir).unwrap();
    // No invariants at all - nothing for has_gate_protected_writes to match,
    // so this is exactly the case --isolated should still allow.
    std::fs::write(
        specs_dir.join("read-only.agl"),
        r#"spec ReadOnlyLookup {
            in: customer: str
            out: summary: str
            requires: HubSpot.get_organization_details
            skill: read-only-lookup

            flow {
                state FETCH -> call(HubSpot.get_organization_details, customer) -> TERMINATE("done")
            }
        }"#,
    )
    .unwrap();

    let project_dir = std::env::temp_dir().join(format!(
        "kazam-agl-load-test-project-isolated-ok-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&project_dir);
    std::fs::create_dir_all(&project_dir).unwrap();

    let output = Command::new(bin())
        .args(["agl", "load", "--isolated", "--out"])
        .arg(&project_dir)
        .env("HOME", &fake_home)
        .output()
        .expect("run kazam agl load --isolated");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let agent_path = project_dir.join(".claude/agents/read-only-lookup.md");
    let skill_path = project_dir.join(".claude/skills/read-only-lookup.md");
    assert!(agent_path.exists(), "expected {}", agent_path.display());
    assert!(skill_path.exists(), "expected {}", skill_path.display());

    let agent_doc = std::fs::read_to_string(&agent_path).unwrap();
    assert!(agent_doc.contains("name: read-only-lookup"));
    assert!(agent_doc.contains("tools: HubSpot.get_organization_details"));
    assert!(agent_doc.contains("FETCH"));

    let skill_doc = std::fs::read_to_string(&skill_path).unwrap();
    assert!(skill_doc.contains("subagent_type: \"read-only-lookup\""));
    assert!(!skill_doc.contains("FETCH"));
}

#[test]
fn load_skips_an_invalid_spec_but_still_loads_the_valid_ones() {
    let fake_home = std::env::temp_dir().join(format!(
        "kazam-agl-load-test-home-mixed-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&fake_home);
    let specs_dir = fake_home.join(".kazam").join("agl").join("specs");
    std::fs::create_dir_all(&specs_dir).unwrap();

    // Invalid: SYNC reachable without its required gate.
    std::fs::write(
        specs_dir.join("broken.agl"),
        r#"spec Broken {
            in: x: str
            out: y: str
            invariant { deny: write(hubspot) without gate(human_approval) }
            flow { state SYNC -> call(HubSpot.update_contact, x) -> TERMINATE("done") }
        }"#,
    )
    .unwrap();
    std::fs::write(
        specs_dir.join("fine.agl"),
        r#"spec Fine {
            in: x: str
            out: y: str
            flow { state A -> evaluate(x) -> TERMINATE("done") }
        }"#,
    )
    .unwrap();

    let project_dir = std::env::temp_dir().join(format!(
        "kazam-agl-load-test-project-mixed-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&project_dir);
    std::fs::create_dir_all(&project_dir).unwrap();

    let output = Command::new(bin())
        .args(["agl", "load", "--out"])
        .arg(&project_dir)
        .env("HOME", &fake_home)
        .output()
        .expect("run kazam agl load");

    assert!(output.status.success());
    assert!(project_dir.join(".claude/skills/fine.md").exists());
    assert!(!project_dir.join(".claude/skills/broken.md").exists());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("skipped 1 spec"), "stdout: {stdout}");
}
