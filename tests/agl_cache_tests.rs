//! End-to-end tests for `kazam agl cache-migrate`.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_kazam"))
}

/// `run_cache_migrate` reads $HOME/.kazam/agl/cache, so every test sandboxes
/// HOME entirely - this must never touch the real ~/.kazam/agl on the
/// machine running it.
fn fake_home(suffix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "kazam-agl-cache-test-home-{suffix}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn migrate_adds_a_missing_field_with_a_type_default() {
    let home = fake_home("add-field");
    let specs_dir = home.join(".kazam").join("agl").join("specs");
    std::fs::create_dir_all(&specs_dir).unwrap();
    std::fs::write(
        specs_dir.join("call-prep.agl"),
        r#"spec CallPrep {
            in: customer: str
            out: y: bool

            cache slack-lookups {
                customer: str, int_channel: str, verified: bool
            }

            flow {
                state A -> evaluate(customer) -> TERMINATE("done")
            }
        }"#,
    )
    .unwrap();

    let cache_dir = home.join(".kazam").join("agl").join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();
    // Written before `verified` existed - missing that field entirely.
    std::fs::write(
        cache_dir.join("slack-lookups.jsonl"),
        "{\"customer\":\"halcyon\",\"int_channel\":\"C0AB1EP6HQA\"}\n\
         {\"customer\":\"cohere\",\"int_channel\":\"C0A6V9U2TDX\"}\n",
    )
    .unwrap();

    let output = Command::new(bin())
        .args(["agl", "cache-migrate"])
        .arg(specs_dir.join("call-prep.agl"))
        .env("HOME", &home)
        .output()
        .expect("run kazam agl cache-migrate");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("migrated 2 of 2"), "stdout: {stdout}");

    let migrated = std::fs::read_to_string(cache_dir.join("slack-lookups.jsonl")).unwrap();
    let lines: Vec<&str> = migrated.lines().collect();
    assert_eq!(lines.len(), 2);
    let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(first["customer"], "halcyon");
    assert_eq!(first["int_channel"], "C0AB1EP6HQA");
    assert_eq!(first["verified"], false);
}

#[test]
fn migrate_is_a_no_op_when_fields_already_match() {
    let home = fake_home("no-op");
    let specs_dir = home.join(".kazam").join("agl").join("specs");
    std::fs::create_dir_all(&specs_dir).unwrap();
    std::fs::write(
        specs_dir.join("call-prep.agl"),
        r#"spec CallPrep {
            in: customer: str
            out: y: bool

            cache slack-lookups {
                customer: str, int_channel: str
            }

            flow {
                state A -> evaluate(customer) -> TERMINATE("done")
            }
        }"#,
    )
    .unwrap();

    let cache_dir = home.join(".kazam").join("agl").join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::write(
        cache_dir.join("slack-lookups.jsonl"),
        "{\"customer\":\"halcyon\",\"int_channel\":\"C0AB1EP6HQA\"}\n",
    )
    .unwrap();

    let output = Command::new(bin())
        .args(["agl", "cache-migrate"])
        .arg(specs_dir.join("call-prep.agl"))
        .env("HOME", &home)
        .output()
        .expect("run kazam agl cache-migrate");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("nothing to migrate"), "stdout: {stdout}");
}

#[test]
fn migrate_requires_name_when_a_spec_declares_multiple_caches() {
    let home = fake_home("multi");
    let specs_dir = home.join(".kazam").join("agl").join("specs");
    std::fs::create_dir_all(&specs_dir).unwrap();
    std::fs::write(
        specs_dir.join("call-prep.agl"),
        r#"spec CallPrep {
            in: customer: str
            out: y: bool

            cache slack-lookups {
                customer: str
            }
            cache call-prep-timestamps {
                customer: str, last_call_date: str
            }

            flow {
                state A -> evaluate(customer) -> TERMINATE("done")
            }
        }"#,
    )
    .unwrap();

    let output = Command::new(bin())
        .args(["agl", "cache-migrate"])
        .arg(specs_dir.join("call-prep.agl"))
        .env("HOME", &home)
        .output()
        .expect("run kazam agl cache-migrate");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--name"), "stderr: {stderr}");

    let cache_dir = home.join(".kazam").join("agl").join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::write(
        cache_dir.join("call-prep-timestamps.jsonl"),
        "{\"customer\":\"halcyon\"}\n",
    )
    .unwrap();

    let output = Command::new(bin())
        .args(["agl", "cache-migrate"])
        .arg(specs_dir.join("call-prep.agl"))
        .arg("--name")
        .arg("call-prep-timestamps")
        .env("HOME", &home)
        .output()
        .expect("run kazam agl cache-migrate --name");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("migrated 1 of 1"), "stdout: {stdout}");
}

#[test]
fn migrate_reports_cleanly_when_no_cache_file_exists_yet() {
    let home = fake_home("no-file");
    let specs_dir = home.join(".kazam").join("agl").join("specs");
    std::fs::create_dir_all(&specs_dir).unwrap();
    std::fs::write(
        specs_dir.join("call-prep.agl"),
        r#"spec CallPrep {
            in: customer: str
            out: y: bool

            cache slack-lookups {
                customer: str
            }

            flow {
                state A -> evaluate(customer) -> TERMINATE("done")
            }
        }"#,
    )
    .unwrap();

    let output = Command::new(bin())
        .args(["agl", "cache-migrate"])
        .arg(specs_dir.join("call-prep.agl"))
        .env("HOME", &home)
        .output()
        .expect("run kazam agl cache-migrate");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("nothing to migrate"), "stdout: {stdout}");
}
