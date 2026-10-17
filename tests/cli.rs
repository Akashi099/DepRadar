//! End-to-end tests for the `depradar` binary. Every scan runs with
//! `--offline` so no test depends on the OSV API; the npm-basic fixture
//! still yields one finding (the `expresss` typosquat, rated high).

use assert_cmd::Command;

fn fixture(path: &str) -> String {
    format!("{}/tests/fixtures/{path}", env!("CARGO_MANIFEST_DIR"))
}

fn depradar() -> Command {
    let mut cmd = Command::cargo_bin("depradar").unwrap();
    cmd.env_remove("NO_COLOR");
    cmd
}

fn stdout(cmd: &mut Command) -> String {
    String::from_utf8(cmd.output().unwrap().stdout).unwrap()
}

#[test]
fn no_color_env_strips_ansi_codes_even_with_color_always() {
    let out = stdout(
        depradar()
            .env("NO_COLOR", "1")
            .args(["--offline", "--color", "always"])
            .arg(fixture("npm-basic/package-lock.json")),
    );

    assert!(out.contains("[HIGH] expresss"), "{out}");
    assert!(!out.contains('\u{1b}'), "ANSI escape in output: {out:?}");
}

#[test]
fn color_always_colors_severity_labels() {
    let out = stdout(
        depradar()
            .args(["--offline", "--color", "always"])
            .arg(fixture("npm-basic/package-lock.json")),
    );

    assert!(out.contains("\u{1b}["), "expected ANSI codes: {out:?}");
    assert!(out.contains("expresss"), "{out}");
}

#[test]
fn piped_output_is_uncolored_by_default() {
    let out = stdout(
        depradar()
            .arg("--offline")
            .arg(fixture("npm-basic/package-lock.json")),
    );

    assert!(out.contains("[HIGH] expresss"), "{out}");
    assert!(!out.contains('\u{1b}'), "ANSI escape in output: {out:?}");
}

#[test]
fn quiet_prints_only_findings_and_the_summary_line() {
    let out = stdout(
        depradar()
            .args(["--offline", "--quiet"])
            .arg(fixture("npm-basic/package-lock.json")),
    );

    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 2, "{out}");
    assert!(lines[0].starts_with("[HIGH] expresss@1.0.0"), "{out}");
    assert!(lines[1].starts_with("Summary: 1 typosquat(s)"), "{out}");
    assert!(!out.contains("Scanned "), "{out}");
    assert!(!out.contains("Warnings"), "{out}");
}

#[test]
fn verbose_lists_dependencies_and_stage_timings() {
    let out = stdout(
        depradar()
            .args(["--offline", "--verbose"])
            .arg(fixture("npm-basic/package-lock.json")),
    );

    assert!(out.contains("Dependencies checked (4):"), "{out}");
    for dependency in ["express@4.21.2", "expresss@1.0.0", "lodash@4.17.4"] {
        assert!(out.contains(&format!("  - {dependency} [")), "{dependency}: {out}");
    }
    assert!(out.contains("  - zzz-unversioned-internal-fixture ["), "{out}");
    assert!(out.contains("Timings:"), "{out}");
    assert!(out.contains("  - parse: "), "{out}");
    assert!(out.contains("  - typosquat: "), "{out}");
    assert!(out.contains("  - total: "), "{out}");
    assert!(out.contains("Summary: "), "{out}");
}

#[test]
fn quiet_and_verbose_together_is_a_usage_error() {
    depradar()
        .args(["--offline", "--quiet", "--verbose"])
        .arg(fixture("npm-basic/package-lock.json"))
        .assert()
        .code(2);
}
