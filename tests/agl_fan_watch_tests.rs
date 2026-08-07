//! Integration tests for `fan()` and `watch()`: the `--isolated` refusal
//! `fan()` triggers via `validator::has_gate_protected_writes`, and the
//! `load`-time warning for a `fan()` target that doesn't resolve to a real
//! spec file. Both sandbox `$HOME` entirely - never touch the real
//! `~/.kazam/agl` on the machine running these.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_kazam"))
}

fn fresh_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "kazam-agl-fan-watch-{label}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn load_isolated_refuses_a_spec_containing_a_bare_fan() {
    let fake_home = fresh_dir("home-isolated-refuse");
    let specs_dir = fake_home.join(".kazam").join("agl").join("specs");
    std::fs::create_dir_all(&specs_dir).unwrap();
    // The fan target doesn't need to exist for the --isolated refusal to
    // fire - has_gate_protected_writes is conservative on the presence of
    // fan() alone, it never resolves the target spec.
    std::fs::write(
        specs_dir.join("deal-monitor.agl"),
        r#"spec DealMonitor {
            in: targets: str
            out: y: bool
            skill: deal-monitor

            flow {
                state SCAN -> fan(WorkflowDeal, targets) -> TERMINATE("done")
            }
        }"#,
    )
    .unwrap();

    let project_dir = fresh_dir("project-isolated-refuse");
    let output = Command::new(bin())
        .args(["agl", "load", "--isolated", "--out"])
        .arg(&project_dir)
        .env("HOME", &fake_home)
        .output()
        .expect("run kazam agl load --isolated");

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("refusing to compile"), "stdout: {stdout}");
    assert!(!project_dir.join(".claude/agents/deal-monitor.md").exists());
}

#[test]
fn load_inline_compiles_a_spec_containing_fan_and_watch_fine() {
    let fake_home = fresh_dir("home-inline-fan-watch");
    let specs_dir = fake_home.join(".kazam").join("agl").join("specs");
    std::fs::create_dir_all(&specs_dir).unwrap();
    std::fs::write(
        specs_dir.join("release.agl"),
        r#"spec Release {
            in: repo: str
            out: y: bool
            skill: release

            flow {
                state BUILD -> watch(ci status for repo is green) -> next
                state FANOUT -> fan(WorkflowDeal, "3")             -> TERMINATE("done")
            }
        }"#,
    )
    .unwrap();

    let project_dir = fresh_dir("project-inline-fan-watch");
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
    let skill_path = project_dir.join(".claude/skills/release/SKILL.md");
    assert!(skill_path.exists());
    let doc = std::fs::read_to_string(&skill_path).unwrap();
    assert!(doc.contains("watch(ci status for repo is green)"));
    assert!(doc.contains(r#"fan(WorkflowDeal, "3")"#));
}

#[test]
fn load_warns_on_a_fan_target_that_does_not_resolve_to_a_real_spec() {
    let fake_home = fresh_dir("home-missing-fan-target");
    let specs_dir = fake_home.join(".kazam").join("agl").join("specs");
    std::fs::create_dir_all(&specs_dir).unwrap();
    std::fs::write(
        specs_dir.join("deal-monitor.agl"),
        r#"spec DealMonitor {
            in: targets: str
            out: y: bool
            skill: deal-monitor

            flow {
                state SCAN -> fan(WorkflowDeal, targets) -> TERMINATE("done")
            }
        }"#,
    )
    .unwrap();
    // Deliberately no workflow-deal.agl written - WorkflowDeal never
    // resolves, so this should warn, not fail, and the skill still compiles
    // (inline mode doesn't hit the --isolated refusal from the test above).

    let project_dir = fresh_dir("project-missing-fan-target");
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
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("warnings on"), "stdout: {stdout}");
    assert!(stdout.contains("WorkflowDeal"), "stdout: {stdout}");
    assert!(
        project_dir
            .join(".claude/skills/deal-monitor/SKILL.md")
            .exists(),
        "a missing fan target should warn, not block compiling the skill"
    );
}
