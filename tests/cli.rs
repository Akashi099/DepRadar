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
