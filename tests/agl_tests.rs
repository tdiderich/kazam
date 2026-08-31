//! End-to-end tests for `kazam agl validate` / `kazam agl export`.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_kazam"))
}

fn write_spec(name: &str, contents: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("kazam-agl-tests");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(name);
    std::fs::write(&path, contents).expect("write spec");
    path
}

const VALID_SPEC: &str = r#"
spec MeetingPrep {
  in:  calendar_event: str, slack_channels: list[str]
  out: agenda_update: str

  invariant {
    deny: write(calendar) without gate(human_approval)
    deny: fetch(slack) where channel NOT IN slack_channels
  }

  flow {
    state FETCH_CALENDAR  -> call(GoogleCalendar.get, calendar_event) -> next
    state SCAN_SLACK      -> map(Slack.read, slack_channels)           -> next
    state DIFF_AGENDA     -> evaluate(slack_data vs calendar_data)    -> branch

    branch DIFF_AGENDA {
      if no_diff -> TERMINATE("Already up to date")
      if has_diff -> PROPOSE_UPDATE
    }

    state PROPOSE_UPDATE  -> gate(human_approval)                     -> EXECUTE_WRITE
    state EXECUTE_WRITE   -> call(GoogleCalendar.update, agenda)      -> TERMINATE("Done")
  }
}
"#;

const CIRCULAR_SPEC: &str = r#"
spec Loopy {
  in: x: str
  out: y: str

  flow {
    state A -> evaluate(x) -> B
    state B -> evaluate(x) -> C
    state C -> evaluate(x) -> A
  }
}
"#;

const BROKEN_REFERENCES_SPEC: &str = r#"
spec Broken {
  in: x: str
  out: y: str

  flow {
    state A -> evaluate(x) -> GHOST_STATE
    state B -> evaluate(x) -> branch

    branch B {
      if some_condition -> NOWHERE
    }
  }
}
"#;

#[test]
fn validate_accepts_the_canonical_spec() {
    let path = write_spec("valid.agl", VALID_SPEC);
    let output = Command::new(bin())
        .args(["agl", "validate"])
        .arg(&path)
        .output()
        .expect("run kazam agl validate");
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("valid"), "stdout: {stdout}");
}

#[test]
fn validate_reports_circular_non_terminating_branch() {
    let path = write_spec("circular.agl", CIRCULAR_SPEC);
    let output = Command::new(bin())
        .args(["agl", "validate"])
        .arg(&path)
        .output()
        .expect("run kazam agl validate");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("non-terminating-cycle"), "stdout: {stdout}");
}

#[test]
fn validate_reports_broken_references() {
    let path = write_spec("broken.agl", BROKEN_REFERENCES_SPEC);
    let output = Command::new(bin())
        .args(["agl", "validate"])
        .arg(&path)
        .output()
        .expect("run kazam agl validate");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("undefined-goto-target"), "stdout: {stdout}");
    assert!(stdout.contains("undefined-goto-target") || stdout.contains("GHOST_STATE"));
    assert!(stdout.contains("NOWHERE"), "stdout: {stdout}");
}

#[test]
fn validate_json_output_is_well_formed() {
    let path = write_spec("valid_json.agl", VALID_SPEC);
    let output = Command::new(bin())
        .args(["agl", "validate", "--json"])
        .arg(&path)
        .output()
        .expect("run kazam agl validate --json");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON output");
    assert_eq!(parsed["valid"], serde_json::Value::Bool(true));
    assert!(parsed["diagnostics"].is_array());
}

#[test]
fn validate_json_output_reports_parse_errors() {
    let path = write_spec("unparseable.agl", "spec { totally not agl");
    let output = Command::new(bin())
        .args(["agl", "validate", "--json"])
        .arg(&path)
        .output()
        .expect("run kazam agl validate --json");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON output");
    assert_eq!(parsed["valid"], serde_json::Value::Bool(false));
    assert!(parsed["parse_error"]["line"].is_number());
}

#[test]
fn export_renders_prompt_block_to_stdout() {
    let path = write_spec("export.agl", VALID_SPEC);
    let output = Command::new(bin())
        .args(["agl", "export"])
        .arg(&path)
        .output()
        .expect("run kazam agl export");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("<agent_spec name=\"MeetingPrep\">"));
    assert!(stdout.contains("<execution_contract>"));
    assert!(stdout.contains("gate(human_approval)"));
    assert!(stdout.contains("</agent_spec>"));
}

#[test]
fn export_writes_to_file_with_out_flag() {
    let path = write_spec("export_to_file.agl", VALID_SPEC);
    let dir = std::env::temp_dir().join("kazam-agl-tests");
    let out_path = dir.join("export_to_file.prompt.txt");
    let _ = std::fs::remove_file(&out_path);

    let output = Command::new(bin())
        .args(["agl", "export"])
        .arg(&path)
        .arg("--out")
        .arg(&out_path)
        .output()
        .expect("run kazam agl export --out");
    assert!(output.status.success());

    let written = std::fs::read_to_string(&out_path).expect("read exported prompt");
    assert!(written.contains("<agent_spec name=\"MeetingPrep\">"));
}

#[test]
fn export_rejects_unsupported_format() {
    let path = write_spec("export_bad_format.agl", VALID_SPEC);
    let output = Command::new(bin())
        .args(["agl", "export"])
        .arg(&path)
        .arg("--format")
        .arg("yaml")
        .output()
        .expect("run kazam agl export --format yaml");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unsupported export format"),
        "stderr: {stderr}"
    );
}

#[test]
fn validate_reports_missing_file() {
    let output = Command::new(bin())
        .args(["agl", "validate"])
        .arg("/nonexistent/path/to/spec.agl")
        .output()
        .expect("run kazam agl validate");
    assert!(!output.status.success());
}

#[test]
fn validate_tools_flag_warns_on_unlisted_function() {
    let path = write_spec("tools_missing.agl", VALID_SPEC);
    let manifest_path = std::env::temp_dir()
        .join("kazam-agl-tests")
        .join("manifest_missing.json");
    std::fs::write(&manifest_path, r#"["TechnicalSuccessHub.read_page"]"#).unwrap();

    let output = Command::new(bin())
        .args(["agl", "validate"])
        .arg(&path)
        .arg("--tools")
        .arg(&manifest_path)
        .output()
        .expect("run kazam agl validate --tools");

    // The spec itself is otherwise valid - an unlisted-tool warning alone
    // must not flip exit status.
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("undefined-tool-binding"),
        "stdout: {stdout}"
    );
}

#[test]
fn validate_tools_flag_silent_when_manifest_covers_everything() {
    let path = write_spec("tools_complete.agl", VALID_SPEC);
    let manifest_path = std::env::temp_dir()
        .join("kazam-agl-tests")
        .join("manifest_complete.json");
    std::fs::write(
        &manifest_path,
        r#"["GoogleCalendar.get", "Slack.read", "GoogleCalendar.update"]"#,
    )
    .unwrap();

    let output = Command::new(bin())
        .args(["agl", "validate"])
        .arg(&path)
        .arg("--tools")
        .arg(&manifest_path)
        .output()
        .expect("run kazam agl validate --tools");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("undefined-tool-binding"),
        "stdout: {stdout}"
    );
}

#[test]
fn validate_without_tools_flag_never_emits_tool_binding_warnings() {
    let path = write_spec("tools_absent.agl", VALID_SPEC);
    let output = Command::new(bin())
        .args(["agl", "validate"])
        .arg(&path)
        .output()
        .expect("run kazam agl validate");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("undefined-tool-binding"),
        "stdout: {stdout}"
    );
}

#[test]
fn flow_prints_an_ascii_diagram_of_the_graph() {
    let path = write_spec("flow.agl", VALID_SPEC);
    let output = Command::new(bin())
        .args(["agl", "flow"])
        .arg(&path)
        .output()
        .expect("run kazam agl flow");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("FETCH_CALENDAR"), "stdout: {stdout}");
    assert!(stdout.contains("PROPOSE_UPDATE"), "stdout: {stdout}");
}
