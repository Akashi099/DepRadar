//! End-to-end tests for the `depradar` binary (and the deprecated `scan`
//! alias). Every scan runs with `--offline` so no test touches the network.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;

fn depradar() -> Command {
    Command::cargo_bin("depradar").unwrap()
}

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(relative)
}

/// Parses the whole of stdout as one JSON document, which fails if anything
/// else (logs, a second document) was printed alongside it.
fn stdout_json(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "stdout is not a single JSON document ({err}):\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

#[test]
fn name_output_matches_the_legacy_scan_binary() {
    let new = depradar().args(["name", "expresss"]).output().unwrap();
    let old = Command::cargo_bin("scan")
        .unwrap()
        .arg("expresss")
        .output()
        .unwrap();

    assert!(new.status.success());
    assert!(old.status.success());
    assert_eq!(
        String::from_utf8_lossy(&new.stdout),
        String::from_utf8_lossy(&old.stdout)
    );
    assert!(String::from_utf8_lossy(&new.stdout).contains("likely typosquat"));
    assert!(String::from_utf8_lossy(&old.stderr).contains("deprecated"));
}

#[test]
fn help_text_matches_snapshot() {
    let output = depradar().arg("--help").output().unwrap();
    assert!(output.status.success());
    let actual = String::from_utf8_lossy(&output.stdout)
        .replace("depradar.exe", "depradar")
        .replace("\r\n", "\n");
    let expected = include_str!("snapshots/depradar-help.txt").replace("\r\n", "\n");
    assert_eq!(
        actual, expected,
        "help text changed; if that's intended, regenerate the snapshot with \
         `cargo run --bin depradar -- --help > tests/snapshots/depradar-help.txt`"
    );
}

#[test]
fn name_json_is_one_document() {
    let output = depradar()
        .args(["--format", "json", "name", "expresss"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let json = stdout_json(&output);
    assert_eq!(json["candidate"], "expresss");
    assert_eq!(json["likely_typosquat"], true);
    assert_eq!(json["distance_match"]["name"], "express");
    assert_eq!(json["distance_match"]["distance"], 1);
}

#[test]
fn scan_dir_json_groups_findings_by_lockfile() {
    let output = depradar()
        .args(["scan", "--offline", "--format", "json"])
        .arg(fixture("npm-basic"))
        .output()
        .unwrap();
    // A typosquat was found.
    assert_eq!(output.status.code(), Some(1));

    let json = stdout_json(&output);
    let lockfiles = json["lockfiles"].as_array().unwrap();
    assert_eq!(lockfiles.len(), 1);
    assert_eq!(lockfiles[0]["path"], "package-lock.json");
    let findings = lockfiles[0]["findings"].as_array().unwrap();
    assert!(
        findings
            .iter()
            .any(|f| f["package"] == "expresss" && f["kind"] == "typosquat"),
        "{findings:?}"
    );
    assert_eq!(json["errors"], Value::Array(Vec::new()));
}

#[test]
fn scan_file_text_reports_findings() {
    let output = depradar()
        .args(["scan", "--offline"])
        .arg(fixture("pypi-basic/requirements.txt"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("reqeusts"), "{stdout}");
}

#[test]
fn scan_error_in_json_mode_still_prints_one_document() {
    let output = depradar()
        .args([
            "--format",
            "json",
            "scan",
            "--offline",
            "does/not/exist.lock",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(stdout_json(&output)["error"].is_string());
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("error: "));
}

#[test]
fn missing_subcommand_is_a_usage_error() {
    let output = depradar().output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn name_reads_newline_separated_names_from_stdin() {
    let output = depradar()
        .args(["name", "-"])
        .write_stdin("expresss\n# a comment\n\nreact\nlodahs\n")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("expresss: likely typosquat"), "{stdout}");
    assert!(
        stdout.contains("react: no typosquat signal detected"),
        "{stdout}"
    );
    assert!(stdout.contains("lodahs:"), "{stdout}");
    assert!(!stdout.contains("a comment"), "{stdout}");
}

#[test]
fn name_stdin_json_is_one_array_document() {
    let output = depradar()
        .args(["--format", "json", "name", "-"])
        .write_stdin("expresss\nreact\nlodahs\n")
        .output()
        .unwrap();

    assert!(output.status.success());
    let json = stdout_json(&output);
    let results = json
        .as_array()
        .expect("stdin JSON output should be an array");
    assert_eq!(results.len(), 3);
    assert_eq!(results[0]["candidate"], "expresss");
    assert_eq!(results[1]["candidate"], "react");
    assert_eq!(results[2]["candidate"], "lodahs");
}

// ── depradar.toml config file tests ──────────────────────────────────────────

#[test]
fn config_file_format_json_is_used_when_no_flag() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("depradar.toml"), r#"format = "json""#).unwrap();

    let output = depradar()
        .current_dir(dir.path())
        .args(["scan", "--offline"])
        .arg(fixture("npm-basic"))
        .output()
        .unwrap();

    // Should emit JSON even though --format was not passed on the CLI.
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert!(json["lockfiles"].is_array(), "expected JSON output shape");
}

#[test]
fn cli_format_flag_overrides_config_file() {
    let dir = tempfile::tempdir().unwrap();
    // Config file says json, but the CLI flag says text.
    std::fs::write(dir.path().join("depradar.toml"), r#"format = "json""#).unwrap();

    let output = depradar()
        .current_dir(dir.path())
        .args(["--format", "text", "scan", "--offline"])
        .arg(fixture("npm-basic"))
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Text output should NOT be valid JSON at the top level.
    assert!(
        serde_json::from_str::<Value>(&stdout).is_err() || !stdout.contains(r#""lockfiles""#),
        "expected text output, got JSON: {stdout}"
    );
}

#[test]
fn config_file_fail_on_none_exits_zero_even_with_findings() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("depradar.toml"), r#"fail_on = "none""#).unwrap();

    let output = depradar()
        .current_dir(dir.path())
        .args(["scan", "--offline"])
        .arg(fixture("npm-basic"))
        .output()
        .unwrap();

    // npm-basic has a typosquat finding, but fail_on = none → exit 0.
    assert_eq!(
        output.status.code(),
        Some(0),
        "fail_on=none should always exit 0"
    );
}

#[test]
fn cli_fail_on_flag_overrides_config_file_none() {
    let dir = tempfile::tempdir().unwrap();
    // Config says none (never fail), but CLI says high → should fail.
    std::fs::write(dir.path().join("depradar.toml"), r#"fail_on = "none""#).unwrap();

    let output = depradar()
        .current_dir(dir.path())
        .args(["--fail-on", "high", "scan", "--offline"])
        .arg(fixture("npm-basic"))
        .output()
        .unwrap();

    // npm-basic contains a typosquat (rated High), so exit code should be 1.
    assert_eq!(
        output.status.code(),
        Some(1),
        "--fail-on high should override config fail_on=none"
    );
}

#[test]
fn config_file_exclude_skips_matching_paths() {
    let dir = tempfile::tempdir().unwrap();
    // Exclude the npm-basic fixture entirely.
    let fixture_path = fixture("npm-basic");
    let pattern = fixture_path.to_string_lossy().replace('\\', "/");
    std::fs::write(
        dir.path().join("depradar.toml"),
        format!(r#"exclude = ["{pattern}"]"#),
    )
    .unwrap();

    let output = depradar()
        .current_dir(dir.path())
        .args(["scan", "--offline"])
        .arg(&fixture_path)
        .output()
        .unwrap();

    // Excluded path → no findings → exit 0.
    assert_eq!(
        output.status.code(),
        Some(0),
        "excluded path should produce no findings"
    );
}

#[test]
fn cli_exclude_flag_overrides_config_file() {
    let dir = tempfile::tempdir().unwrap();
    // Config excludes everything with "nonexistent" — should not affect the scan.
    std::fs::write(
        dir.path().join("depradar.toml"),
        r#"exclude = ["nonexistent/**"]"#,
    )
    .unwrap();

    let output = depradar()
        .current_dir(dir.path())
        // Provide --exclude that matches nothing — scan should still find the typosquat.
        .args(["scan", "--offline", "--exclude", "nothing/**"])
        .arg(fixture("npm-basic"))
        .output()
        .unwrap();

    // npm-basic has a typosquat finding, so exit code should be 1 (default fail_on=high).
    assert_eq!(
        output.status.code(),
        Some(1),
        "scan should find typosquat when exclude doesn't match"
    );
}

#[test]
fn missing_config_file_uses_defaults() {
    // No depradar.toml → all defaults: format=text, fail_on=high.
    let output = depradar()
        .args(["scan", "--offline"])
        .arg(fixture("npm-basic"))
        .output()
        .unwrap();

    // Default fail_on=high, npm-basic has a typosquat (high severity) → exit 1.
    assert_eq!(output.status.code(), Some(1));
    // Default format=text → stdout is not JSON.
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        serde_json::from_str::<Value>(&stdout).is_err() || !stdout.trim_start().starts_with('{'),
        "expected text format by default"
    );
}

#[test]
fn invalid_config_file_exits_with_code_2() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("depradar.toml"), "[[invalid{{{").unwrap();

    let output = depradar()
        .current_dir(dir.path())
        .args(["scan", "--offline"])
        .arg(fixture("npm-basic"))
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(2),
        "invalid depradar.toml should exit 2"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("depradar.toml"),
        "error message should mention depradar.toml: {stderr}"
    );
}
