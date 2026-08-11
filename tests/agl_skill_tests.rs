//! End-to-end tests for `kazam agl skill`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_kazam"))
}

/// Cargo test runs these in parallel threads. A shared fixed directory name
/// let two tests race on the same file: one truncates it mid-write while
/// another's subprocess reads it, producing "expected 'spec', found end of
/// file" instead of the real assertion failure. Each caller gets its own
/// directory so no two tests ever touch the same path.
fn unique_dir(base: &str) -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    format!(
        "{base}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    )
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
    let dir = unique_dir("kazam-agl-skill-tests");
    write_file(&dir, "human_approval.agl", HUMAN_APPROVAL_FRAGMENT);
    write_file(&dir, "hubspot_sync.agl", SPEC_WITH_GATED_WRITE)
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
        &unique_dir("kazam-agl-skill-tests"),
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
    let written = out_dir.join("hub-spot-sync").join("SKILL.md");
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
    let skill_path = project_dir.join(".claude/skills/sync-hubspot/SKILL.md");
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
    assert!(!project_dir
        .join(".claude/skills/sync-hubspot/SKILL.md")
        .exists());
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
    let skill_path = project_dir.join(".claude/skills/read-only-lookup/SKILL.md");
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
    assert!(project_dir.join(".claude/skills/fine/SKILL.md").exists());
    assert!(!project_dir.join(".claude/skills/broken/SKILL.md").exists());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("skipped 1 spec"), "stdout: {stdout}");
}

#[test]
fn load_embeds_a_real_template_file_referenced_by_an_evaluate_state() {
    let fake_home = std::env::temp_dir().join(format!(
        "kazam-agl-load-test-home-templates-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&fake_home);
    let specs_dir = fake_home.join(".kazam").join("agl").join("specs");
    std::fs::create_dir_all(&specs_dir).unwrap();
    std::fs::write(
        specs_dir.join("call-prep.agl"),
        r#"spec CallPrep {
            in: customer: str
            out: y: bool

            flow {
                state WRITE_SUMMARY -> evaluate(activity_summary_draft vs activity-summary) -> TERMINATE("done")
            }
        }"#,
    )
    .unwrap();

    let templates_dir = fake_home.join(".kazam").join("agl").join("templates");
    std::fs::create_dir_all(&templates_dir).unwrap();
    std::fs::write(
        templates_dir.join("activity-summary.md"),
        "<!--spec-->\n## {Customer} Activity Summary\n\n\
         - **Lead-in**: 5-10 word finding\n\
         <!--samples-->\n## Halcyon Activity Summary\n\n\
         - **Sentiment**: stayed Medium, expanding scope",
    )
    .unwrap();

    let project_dir = std::env::temp_dir().join(format!(
        "kazam-agl-load-test-project-templates-{}",
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

    let skill_doc =
        std::fs::read_to_string(project_dir.join(".claude/skills/call-prep/SKILL.md")).unwrap();
    assert!(skill_doc.contains("## Templates"), "doc: {skill_doc}");
    assert!(skill_doc.contains("### activity-summary"));
    assert!(skill_doc.contains("- **Lead-in**: 5-10 word finding"));
    assert!(skill_doc.contains("**Known-good examples:**"));
    assert!(skill_doc.contains("## Halcyon Activity Summary"));
    assert!(!skill_doc.contains("<!--spec-->"));
    assert!(!skill_doc.contains("<!--samples-->"));
}

// ── publish: spec-declared default destination ─────────────────────

fn spec_with_publish(publish_dir: &Path) -> PathBuf {
    write_file(
        "kazam-agl-publish-tests",
        "published.agl",
        &format!(
            r#"spec PublishedSpec {{
              in: x: str
              out: y: bool
              skill: published-spec
              publish: "{}"

              flow {{
                state ONLY -> evaluate(x) -> TERMINATE("done")
              }}
            }}"#,
            publish_dir.display()
        ),
    )
}

#[test]
fn skill_uses_publish_field_when_no_explicit_out() {
    let publish_dir = std::env::temp_dir().join("kazam-agl-publish-dest");
    let _ = std::fs::remove_dir_all(&publish_dir);
    std::fs::create_dir_all(&publish_dir).unwrap();

    // A fake HOME so the spec genuinely lives under it, same as the real
    // ~/.kazam/agl/specs/ convention - otherwise there's nothing for
    // display_path_with_tilde to actually strip.
    let fake_home =
        std::env::temp_dir().join(format!("kazam-agl-publish-fakehome-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&fake_home);
    let spec_dir = fake_home.join(".kazam").join("agl").join("specs");
    std::fs::create_dir_all(&spec_dir).unwrap();
    let spec_path = spec_dir.join("published.agl");
    std::fs::write(
        &spec_path,
        format!(
            r#"spec PublishedSpec {{
              in: x: str
              out: y: bool
              skill: published-spec
              publish: "{}"

              flow {{
                state ONLY -> evaluate(x) -> TERMINATE("done")
              }}
            }}"#,
            publish_dir.display()
        ),
    )
    .unwrap();

    let output = Command::new(bin())
        .args(["agl", "skill"])
        .arg(&spec_path)
        .args(["--target", "claude"])
        .env("HOME", &fake_home)
        .output()
        .expect("run kazam agl skill with no -o");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "publish: should redirect output away from stdout"
    );
    let written = publish_dir.join("published-spec").join("SKILL.md");
    assert!(written.exists(), "expected {}", written.display());

    let doc = std::fs::read_to_string(&written).unwrap();
    assert!(
        doc.contains("published from a personal AGL spec"),
        "postflight should credit publish: when it's actually what routed this file: {doc}"
    );
    assert!(
        !doc.contains("-o ~/.claude/skills/"),
        "postflight should not claim the generic -o default when publish: is what really \
         decided the destination: {doc}"
    );
    assert!(
        doc.contains("~/.kazam/agl/specs/published.agl"),
        "postflight should show the ~-relative spec path: {doc}"
    );
    assert!(
        !doc.contains(&fake_home.display().to_string()),
        "postflight must not leak the raw local home directory once publish: sends this file \
         somewhere shared, only the ~-relative form: {doc}"
    );
    assert!(
        doc.contains("github.com/tdiderich/kazam"),
        "postflight should point a reader without the source spec at the kazam repo: {doc}"
    );
}

#[test]
fn explicit_out_overrides_publish_field() {
    let publish_dir = std::env::temp_dir().join("kazam-agl-publish-dest-unused");
    let _ = std::fs::remove_dir_all(&publish_dir);
    std::fs::create_dir_all(&publish_dir).unwrap();
    let spec_path = spec_with_publish(&publish_dir);

    let explicit_dir = std::env::temp_dir().join("kazam-agl-publish-explicit-out");
    let _ = std::fs::remove_dir_all(&explicit_dir);
    std::fs::create_dir_all(&explicit_dir).unwrap();

    let output = Command::new(bin())
        .args(["agl", "skill"])
        .arg(&spec_path)
        .args(["--target", "claude"])
        .arg("--out")
        .arg(&explicit_dir)
        .output()
        .expect("run kazam agl skill --out");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        explicit_dir
            .join("published-spec")
            .join("SKILL.md")
            .exists(),
        "explicit --out should win over publish:"
    );
    assert!(
        !publish_dir.join("published-spec").join("SKILL.md").exists(),
        "publish: should be ignored when --out is given"
    );

    let doc =
        std::fs::read_to_string(explicit_dir.join("published-spec").join("SKILL.md")).unwrap();
    assert!(
        doc.contains("-o ~/.claude/skills/"),
        "postflight should not credit publish: when an explicit --out actually won: {doc}"
    );
}

#[test]
fn load_sends_only_the_declaring_spec_to_its_publish_path() {
    let fake_home = std::env::temp_dir().join(format!(
        "kazam-agl-load-test-home-publish-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&fake_home);
    let specs_dir = fake_home.join(".kazam").join("agl").join("specs");
    std::fs::create_dir_all(&specs_dir).unwrap();

    let publish_dir = std::env::temp_dir().join(format!(
        "kazam-agl-load-publish-dest-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&publish_dir);
    std::fs::create_dir_all(&publish_dir).unwrap();

    std::fs::write(
        specs_dir.join("published.agl"),
        format!(
            r#"spec PublishedSpec {{
              in: x: str
              out: y: bool
              skill: published-spec
              publish: "{}"

              flow {{
                state ONLY -> evaluate(x) -> TERMINATE("done")
              }}
            }}"#,
            publish_dir.display()
        ),
    )
    .unwrap();
    std::fs::write(
        specs_dir.join("plain.agl"),
        r#"spec PlainSpec {
              in: x: str
              out: y: bool
              skill: plain-spec

              flow {
                state ONLY -> evaluate(x) -> TERMINATE("done")
              }
            }"#,
    )
    .unwrap();

    let output = Command::new(bin())
        .args(["agl", "load"])
        .env("HOME", &fake_home)
        .output()
        .expect("run kazam agl load");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        publish_dir.join("published-spec").join("SKILL.md").exists(),
        "spec with publish: should land in its declared directory"
    );
    assert!(
        fake_home
            .join(".claude/skills/plain-spec/SKILL.md")
            .exists(),
        "spec without publish: should still land under the default --scope"
    );
    assert!(
        !fake_home.join(".claude/skills/published-spec").exists(),
        "spec with publish: should not also land under the default --scope"
    );
}
