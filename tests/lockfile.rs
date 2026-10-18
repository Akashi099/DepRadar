//! Fixture tests for the lockfile parsers.

use std::path::PathBuf;

use deprader::lockfile::parse_lockfile;
use deprader::model::Dependency;

fn fixture(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(path)
}

/// `(name, version, direct)` for each parsed dependency, in parse order.
fn relations(dependencies: &[Dependency]) -> Vec<(&str, Option<&str>, bool)> {
    dependencies
        .iter()
        .map(|d| (d.name.as_str(), d.version.as_deref(), d.direct))
        .collect()
}

#[test]
fn npm_marks_root_dependencies_direct_and_nested_installs_transitive() {
    let parsed = parse_lockfile(&fixture("npm-basic/package-lock.json")).unwrap();

    assert_eq!(
        relations(&parsed.dependencies),
        vec![
            ("express", Some("4.21.2"), true),
            // Same name as a direct dependency, but this nested copy is the
            // one express pulled in.
            ("lodash", Some("4.17.4"), false),
            ("expresss", Some("1.0.0"), true),
            ("lodash", Some("4.17.4"), true),
            ("zzz-unversioned-internal-fixture", None, false),
        ]
    );
}

#[test]
fn requirements_txt_dependencies_are_all_direct() {
    let parsed = parse_lockfile(&fixture("pypi-basic/requirements.txt")).unwrap();

    assert_eq!(
        relations(&parsed.dependencies),
        vec![
            ("flask", None, true),
            ("requests", Some("2.19.0"), true),
            ("reqeusts", Some("1.0.0"), true),
        ]
    );
    assert_eq!(parsed.warnings.len(), 1);
}

/// `(name, version, path)` for each parsed dependency, in parse order.
fn paths(dependencies: &[Dependency]) -> Vec<(&str, Option<&str>, String)> {
    dependencies
        .iter()
        .map(|d| (d.name.as_str(), d.version.as_deref(), d.path.join(" > ")))
        .collect()
}

#[test]
fn npm_paths_follow_the_shortest_chain_and_node_resolution() {
    let parsed = parse_lockfile(&fixture("npm-chain/package-lock.json")).unwrap();

    assert_eq!(
        paths(&parsed.dependencies),
        vec![
            (
                "deep-a",
                Some("1.0.0"),
                "my-app > tiny-helper > deep-a".to_string()
            ),
            (
                "deep-b",
                Some("1.0.0"),
                "my-app > tiny-helper > deep-a > deep-b".to_string()
            ),
            // Reachable via deep-b too, but webpack's chain is shorter.
            (
                "loader-kit",
                Some("1.0.0"),
                "my-app > webpack > loader-kit".to_string()
            ),
            // loader-kit gets its own nested copy of old-lib...
            (
                "old-lib",
                Some("1.0.0"),
                "my-app > webpack > loader-kit > old-lib".to_string()
            ),
            // ...while tiny-helper resolves the hoisted one.
            (
                "old-lib",
                Some("2.0.0"),
                "my-app > tiny-helper > old-lib".to_string()
            ),
            (
                "tiny-helper",
                Some("1.0.0"),
                "my-app > tiny-helper".to_string()
            ),
            ("webpack", Some("5.90.0"), "my-app > webpack".to_string()),
        ]
    );
}
