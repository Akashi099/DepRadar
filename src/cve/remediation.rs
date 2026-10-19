//! Remediation hints: which versions fix a vulnerability.

use super::osv_client::OsvVulnerability;
use std::cmp::Ordering;

/// Every `fixed` version OSV lists for `package` in `vuln`, ascending and
/// deduplicated. Git-commit ranges are skipped since their `fixed` values
/// are hashes, not versions.
pub fn fixed_versions(vuln: &OsvVulnerability, package: &str) -> Vec<String> {
    let mut fixed: Vec<String> = vuln
        .affected
        .iter()
        .filter(|a| {
            a.package
                .as_ref()
                .is_some_and(|p| p.name.eq_ignore_ascii_case(package))
        })
        .flat_map(|a| &a.ranges)
        .filter(|r| r.range_type != "GIT")
        .flat_map(|r| &r.events)
        .filter_map(|e| e.fixed.clone())
        .collect();
    fixed.sort_by(|a, b| compare_versions(a, b));
    fixed.dedup();
    fixed
}

/// The lowest fixed version strictly above `scanned`, i.e. what to
/// upgrade to.
pub fn fix_for(vuln: &OsvVulnerability, package: &str, scanned: &str) -> Option<String> {
    fixed_versions(vuln, package)
        .into_iter()
        .find(|v| compare_versions(v, scanned) == Ordering::Greater)
}

/// Compares dotted versions numerically; a pre-release suffix (`-rc.1`)
/// sorts before the same release, and non-numeric segments count as 0.
fn compare_versions(a: &str, b: &str) -> Ordering {
    fn split(v: &str) -> (Vec<u64>, bool) {
        let v = v.trim_start_matches('v');
        let (core, pre) = match v.split_once(['-', '+']) {
            Some((core, rest)) => (core, v.contains('-') && !rest.is_empty()),
            None => (v, false),
        };
        (
            core.split('.').map(|p| p.parse().unwrap_or(0)).collect(),
            pre,
        )
    }
    let ((na, pa), (nb, pb)) = (split(a), split(b));
    for i in 0..na.len().max(nb.len()) {
        let ord = na.get(i).unwrap_or(&0).cmp(nb.get(i).unwrap_or(&0));
        if ord != Ordering::Equal {
            return ord;
        }
    }
    pb.cmp(&pa)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lodash_advisory() -> OsvVulnerability {
        serde_json::from_str(
            r#"{"id":"GHSA-29mw-wpgm-hmr9","aliases":["CVE-2020-28500"],"affected":[
                {"package":{"name":"lodash","ecosystem":"npm"},
                 "ranges":[{"type":"SEMVER","events":[{"introduced":"0"},{"fixed":"4.17.21"}]}]},
                {"package":{"name":"other","ecosystem":"npm"},
                 "ranges":[{"type":"SEMVER","events":[{"introduced":"0"},{"fixed":"9.9.9"}]}]}]}"#,
        )
        .unwrap()
    }

    #[test]
    fn lodash_4_17_4_is_fixed_in_4_17_21() {
        let v = lodash_advisory();
        assert_eq!(fixed_versions(&v, "lodash"), ["4.17.21"]);
        assert_eq!(fix_for(&v, "lodash", "4.17.4").as_deref(), Some("4.17.21"));
        assert_eq!(fix_for(&v, "lodash", "4.17.21"), None);
    }

    #[test]
    fn picks_lowest_fix_above_scanned_version() {
        let v: OsvVulnerability = serde_json::from_str(
            r#"{"id":"X","affected":[{"package":{"name":"p","ecosystem":"npm"},"ranges":[
                {"type":"SEMVER","events":[{"introduced":"0"},{"fixed":"1.2.5"},
                                           {"introduced":"2.0.0"},{"fixed":"2.0.3"}]},
                {"type":"GIT","events":[{"fixed":"deadbeef"}]}]}]}"#,
        )
        .unwrap();
        assert_eq!(fixed_versions(&v, "p"), ["1.2.5", "2.0.3"]);
        assert_eq!(fix_for(&v, "p", "1.0.0").as_deref(), Some("1.2.5"));
        assert_eq!(fix_for(&v, "p", "1.9.0").as_deref(), Some("2.0.3"));
        assert!(compare_versions("1.10.0", "1.9.0").is_gt());
    }
}
