//! Scope-confusion signal: imitating a trusted npm scope.
//!
//! Attackers borrow the trust of a well-known scope in two ways:
//!
//! - misspelling the scope while keeping the bare name (`@typess/node` for
//!   `@types/node`), and
//! - dropping the scope and gluing it onto the name (`types-node` or
//!   `typesnode` for `@types/node`).
//!
//! [`scope_confusion`] detects both against the scoped entries of the
//! popular-package dataset. Like the other signals it only reports a match;
//! the scanner decides what to do with it.

use super::distance::levenshtein_distance;
use super::normalize::split_npm_name;

/// Reports whether `candidate` imitates a popular scoped package.
///
/// Returns `Some(popular)` when either:
///
/// 1. `candidate` is scoped, its bare name equals the bare name of a popular
///    scoped package, and its scope is exactly one edit from that package's
///    scope (`@typess/node` → `@types/node`); or
/// 2. `candidate` is unscoped and equals `<scope>-<name>` or `<scope><name>`
///    for a popular scoped package (`types-node`, `typesnode` →
///    `@types/node`).
///
/// A candidate that is itself in `dataset` is never flagged, so the real
/// `@types/node` is clean. On a match the earliest `dataset` entry wins.
/// `candidate` is expected to be lowercased already.
pub fn scope_confusion(candidate: &str, dataset: &[String]) -> Option<String> {
    if dataset.iter().any(|name| name == candidate) {
        return None;
    }

    let (candidate_scope, candidate_bare) = split_npm_name(candidate);

    dataset
        .iter()
        .find(|popular| {
            let (Some(scope), bare) = split_npm_name(popular) else {
                return false;
            };
            match candidate_scope {
                Some(candidate_scope) => {
                    candidate_bare == bare && levenshtein_distance(candidate_scope, scope) == 1
                }
                None => {
                    candidate == format!("{scope}-{bare}") || candidate == format!("{scope}{bare}")
                }
            }
        })
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typosquat::dataset::load_popular_packages;

    #[test]
    fn misspelled_scope_is_flagged() {
        let dataset = load_popular_packages();
        assert_eq!(
            scope_confusion("@typess/node", &dataset).as_deref(),
            Some("@types/node")
        );
    }

    #[test]
    fn dropped_scope_is_flagged() {
        let dataset = load_popular_packages();
        assert_eq!(
            scope_confusion("types-node", &dataset).as_deref(),
            Some("@types/node")
        );
        assert_eq!(
            scope_confusion("typesnode", &dataset).as_deref(),
            Some("@types/node")
        );
    }

    #[test]
    fn legitimate_scoped_package_is_not_flagged() {
        let dataset = load_popular_packages();
        assert_eq!(scope_confusion("@types/node", &dataset), None);
    }

    #[test]
    fn different_bare_name_under_lookalike_scope_is_not_flagged() {
        let dataset = load_popular_packages();
        assert_eq!(scope_confusion("@typess/not-real", &dataset), None);
    }

    #[test]
    fn unrelated_unscoped_name_is_not_flagged() {
        let dataset = load_popular_packages();
        assert_eq!(scope_confusion("express", &dataset), None);
        assert_eq!(scope_confusion("nodejs-thing", &dataset), None);
    }
}
