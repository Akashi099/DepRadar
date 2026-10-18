//! npm lockfile and manifest parsers.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use super::ParseError;
use crate::model::{Dependency, Ecosystem};

/// The dependency sections of a `package.json`. Only the package names are
/// used; the version ranges (or git/file/tag specs) they map to are ignored.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackageJson {
    #[serde(default)]
    dependencies: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    dev_dependencies: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    optional_dependencies: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    peer_dependencies: BTreeMap<String, serde_json::Value>,
}

/// Parses the direct dependencies declared in a `package.json`.
///
/// Without a lockfile there are no resolved versions, but the names a
/// project depends on can still be typosquat-checked. Reads
/// `dependencies`, `devDependencies`, `optionalDependencies` and
/// `peerDependencies`; every entry is `direct: true` with `version: None`.
///
/// A name listed in more than one section is returned once. It's only
/// marked `dev` when `devDependencies` is the only place it appears — a
/// library commonly lists a peer dependency under `devDependencies` too, so
/// it can run its own tests against it.
pub fn parse_package_json(contents: &str) -> Result<Vec<Dependency>, ParseError> {
    let manifest: PackageJson = serde_json::from_str(contents)?;

    let runtime = manifest
        .dependencies
        .into_keys()
        .chain(manifest.optional_dependencies.into_keys())
        .chain(manifest.peer_dependencies.into_keys())
        .map(|name| (name, false));
    let dev = manifest
        .dev_dependencies
        .into_keys()
        .map(|name| (name, true));

    let mut seen = BTreeSet::new();
    Ok(runtime
        .chain(dev)
        .filter(|(name, _)| seen.insert(name.clone()))
        .map(|(name, dev)| Dependency {
            name,
            version: None,
            ecosystem: Ecosystem::Npm,
            direct: true,
            dev,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACKAGE_JSON: &str = include_str!("../../tests/fixtures/package.json");

    fn find<'a>(deps: &'a [Dependency], name: &str) -> &'a Dependency {
        deps.iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| panic!("{name} not found in {deps:?}"))
    }

    #[test]
    fn parses_all_four_sections() {
        let deps = parse_package_json(PACKAGE_JSON).unwrap();
        assert!(deps.iter().all(|d| d.ecosystem == Ecosystem::Npm));
        assert!(deps.iter().all(|d| d.direct && d.version.is_none()));

        // dependencies
        assert!(!find(&deps, "express").dev);
        assert!(!find(&deps, "@angular/core").dev);
        // devDependencies
        assert!(find(&deps, "typescript").dev);
        assert!(find(&deps, "@types/node").dev);
        // optionalDependencies
        assert!(!find(&deps, "fsevents").dev);
        // peerDependencies
        assert!(!find(&deps, "rxjs").dev);
    }

    #[test]
    fn names_in_several_sections_are_listed_once() {
        let deps = parse_package_json(PACKAGE_JSON).unwrap();
        assert_eq!(deps.len(), 8);

        // Both a peer and a dev dependency: kept once, not dev-only.
        let react: Vec<_> = deps.iter().filter(|d| d.name == "react").collect();
        assert_eq!(react.len(), 1);
        assert!(!react[0].dev);
    }

    #[test]
    fn missing_sections_are_empty() {
        let deps = parse_package_json(r#"{"name": "empty", "version": "1.0.0"}"#).unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn invalid_json_is_an_error() {
        assert!(parse_package_json("{").is_err());
    }
}
