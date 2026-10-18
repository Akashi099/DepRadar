//! Combined typosquat scoring: the single entry point that ties the
//! edit-distance signal ([`super::distance`]) and the homoglyph signals
//! ([`super::homoglyph`]) together into one answer.
//!
//! Each underlying signal stays independent and boolean/threshold-free on its
//! own — this module is where "is this name suspicious, and why" finally gets
//! decided. [`score_package`] runs a candidate against every signal and
//! [`ScanResult`] reports which fired and how confident the overall verdict
//! is.

use std::fmt;
use std::sync::OnceLock;

use super::bktree::BkTree;
use super::dataset::{popular_packages, popularity_rank};
use super::homoglyph::{digit_swap_impersonation, homoglyph_impersonation};

/// Maximum edit distance from a popular package name that counts as
/// "suspiciously close" rather than coincidental, scaled by the length (in
/// characters) of the candidate name.
///
/// A fixed threshold is far too loose for short names: `ms`, `qs` and `ws`
/// are all within two edits of each other, so a single cut-off would flag
/// most two- and three-letter packages. Short names get no tolerance, medium
/// names one edit, and longer names two:
///
/// | Candidate length | Threshold | Example                            |
/// |------------------|-----------|------------------------------------|
/// | 0-3              | 0         | `ms` vs `qs` is not flagged        |
/// | 4-6              | 1         | `reakt` -> `react` is flagged      |
/// | 7+               | 2         | `expresss` -> `express` is flagged |
///
/// A distance of 0 is an exact match — the real package, not a typosquat —
/// and is never flagged regardless of this threshold.
pub fn threshold_for(len: usize) -> usize {
    match len {
        0..=3 => 0,
        4..=6 => 1,
        _ => 2,
    }
}

/// Result of scoring a candidate package name against the typosquat
/// signals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanResult {
    /// The name that was scored, as given by the caller.
    pub candidate: String,
    /// Set when the nearest popular package is within
    /// [`DISTANCE_THRESHOLD`] edits (and not an exact match): the matched
    /// name and the edit distance to it.
    pub distance_match: Option<(String, usize)>,
    /// Set when [`homoglyph_impersonation`] found that the candidate, once
    /// lookalike characters are folded back to Latin, is a popular package:
    /// the matched name.
    pub homoglyph_match: Option<String>,
    /// Set when [`digit_swap_impersonation`] found that the candidate, once
    /// lookalike digits (`0`, `1`, `3`, `4`, `5`) are folded back to letters,
    /// is a popular package: the matched name.
    pub digit_swap_match: Option<String>,
    /// How likely the candidate is a typosquat, from 0 (no signal) to 100.
    /// See [`confidence`] for the formula.
    pub confidence: u8,
}

impl ScanResult {
    /// True if any signal fired.
    pub fn is_likely_typosquat(&self) -> bool {
        self.distance_match.is_some()
            || self.homoglyph_match.is_some()
            || self.digit_swap_match.is_some()
    }
}

impl fmt::Display for ScanResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.is_likely_typosquat() {
            return write!(f, "{}: no typosquat signal detected", self.candidate);
        }

        write!(
            f,
            "{}: likely typosquat (confidence {}/100)",
            self.candidate, self.confidence
        )?;
        if let Some((name, distance)) = &self.distance_match {
            write!(
                f,
                "\n  - edit distance {distance} from popular package \"{name}\""
            )?;
            if self.keyboard_typo {
                write!(
                    f,
                    "\n  - adjacent-key typo of \"{name}\" (likely accidental install)"
                )?;
            }
        }
        if let Some(name) = &self.homoglyph_match {
            write!(
                f,
                "\n  - homoglyph impersonation of popular package \"{name}\""
            )?;
        }
        if let Some(name) = &self.digit_swap_match {
            write!(
                f,
                "\n  - digit-for-letter swap impersonating popular package \"{name}\""
            )?;
        }
        Ok(())
    }
}

/// Computes the 0–100 confidence score for a set of signals.
///
/// The score is the sum of three parts, and is 0 when no signal fired:
///
/// | Part        | Points                                                        |
/// |-------------|---------------------------------------------------------------|
/// | lookalike   | 50 if the homoglyph or digit-swap signal fired, else 0        |
/// | distance    | `40 - 10 * distance` (30 at distance 1, 20 at 2) if the distance signal fired, else 0 |
/// | popularity  | by the best [`popularity_rank`] among the matched targets: 20 for rank < 10, 15 for < 100, 10 for < 500, 5 otherwise |
///
/// The maximum is 50 + 30 + 20 = 100. Because the lookalike part alone (50)
/// is worth more than the best distance part plus the best popularity part
/// (30 + 20), the ranking *both signals > lookalike only > distance only*
/// holds whatever the target's popularity:
///
/// - lookalike + distance: 75..=100
/// - lookalike only: 55..=70
/// - distance only: 25..=50
///
/// Worked examples against the bundled dataset:
///
/// - `react` → exact match, nothing fires: **0**.
/// - `expresss` → distance 1 from `express` (npm rank 111): 30 + 10 = **40**.
/// - full-width `ｒｅｑｕｅｓｔｓ` → homoglyph of `requests` (PyPI rank 44):
///   50 + 15 = **65**.
/// - `l0dash` → digit swap of `lodash` and distance 1 from it (npm rank
///   607): 50 + 30 + 5 = **85**.
/// - `reаct` with a Cyrillic `а` → homoglyph of `react` and distance 1 from
///   it (npm rank 0): 50 + 30 + 20 = **100**.
pub fn confidence(
    distance_match: Option<&(String, usize)>,
    homoglyph_match: Option<&str>,
    digit_swap_match: Option<&str>,
) -> u8 {
    let lookalike = homoglyph_match.or(digit_swap_match);
    if distance_match.is_none() && lookalike.is_none() {
        return 0;
    }

    let lookalike_points: usize = if lookalike.is_some() { 50 } else { 0 };
    let distance_points =
        distance_match.map_or(0, |(_, distance)| 40usize.saturating_sub(10 * distance));

    let best_rank = distance_match
        .map(|(name, _)| name.as_str())
        .into_iter()
        .chain(homoglyph_match)
        .chain(digit_swap_match)
        .filter_map(popularity_rank)
        .min();
    let popularity_points: usize = match best_rank {
        Some(rank) if rank < 10 => 20,
        Some(rank) if rank < 100 => 15,
        Some(rank) if rank < 500 => 10,
        _ => 5,
    };

    (lookalike_points + distance_points + popularity_points).min(100) as u8
}

/// BK-tree over the bundled dataset, built once on first use.
fn popular_index() -> &'static BkTree<'static> {
    static INDEX: OnceLock<BkTree<'static>> = OnceLock::new();
    INDEX.get_or_init(|| BkTree::new(popular_packages()))
}

/// Scores `name` against the bundled popular-package dataset using every
/// typosquat signal and returns why (if at all) it looks suspicious.
///
/// The dataset and its [`BkTree`] index are built on the first call and
/// reused afterwards, so callers only need a name.
pub fn score_package(name: &str) -> ScanResult {
    let dataset = popular_packages();

    // Results are sorted by distance, then dataset order, so the first hit is
    // what a linear `nearest_match` scan would return. An exact match
    // (distance 0) is the real package and suppresses the signal.
    let distance_match = popular_index()
        .within(name, DISTANCE_THRESHOLD)
        .first()
        .filter(|&&(_, distance)| distance > 0)
        .map(|&(nearest, distance)| (nearest.to_string(), distance));

    let homoglyph_match = homoglyph_impersonation(name, dataset);
    let digit_swap_match = digit_swap_impersonation(name, dataset);
    let confidence = confidence(
        distance_match.as_ref(),
        homoglyph_match.as_deref(),
        digit_swap_match.as_deref(),
    );

    ScanResult {
        candidate: name.to_string(),
        distance_match,
        homoglyph_match,
        digit_swap_match,
        confidence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_popular_package_is_not_flagged() {
        let result = score_package("react");
        assert_eq!(result.distance_match, None);
        assert_eq!(result.homoglyph_match, None);
        assert!(!result.is_likely_typosquat());
    }

    #[test]
    fn uppercase_popular_package_is_normalized_to_exact_match() {
        let result = score_package("React");
        assert_eq!(result.candidate, "React");
        assert!(!result.is_likely_typosquat());
    }

    #[test]
    fn unrelated_name_is_not_flagged() {
        // Long and distinctive enough that its nearest dataset neighbour is
        // far past the threshold (verified: distance 26 to the closest npm
        // entry).
        let result = score_package("definitely-not-a-real-package-zzz123");
        assert_eq!(result.distance_match, None);
        assert_eq!(result.homoglyph_match, None);
        assert!(!result.is_likely_typosquat());
    }

    #[test]
    fn distance_only_match_is_flagged_with_reason() {
        // One inserted 's' — no homoglyphs involved.
        let result = score_package("expresss");
        assert_eq!(result.distance_match, Some(("express".to_string(), 1)));
        assert_eq!(result.homoglyph_match, None);
        assert!(result.is_likely_typosquat());
    }

    #[test]
    fn homoglyph_only_match_is_flagged_with_reason() {
        // "requests" (PyPI) written in full-width Latin: identical edit
        // distance behaviour to any other unrelated string, but flagged by
        // the homoglyph signal.
        let fullwidth = "\u{FF52}\u{FF45}\u{FF51}\u{FF55}\u{FF45}\u{FF53}\u{FF54}\u{FF53}";
        let result = score_package(fullwidth);
        assert_eq!(result.distance_match, None);
        assert_eq!(result.homoglyph_match.as_deref(), Some("requests"));
        assert!(result.is_likely_typosquat());
    }

    #[test]
    fn both_signals_can_fire_on_the_same_candidate() {
        // "react" with a Cyrillic 'а' (U+0430): a single-character
        // substitution, so it is both one edit away from "react" *and* a
        // homoglyph impersonation of it.
        let result = score_package("re\u{0430}ct");
        assert_eq!(result.distance_match, Some(("react".to_string(), 1)));
        assert_eq!(result.homoglyph_match.as_deref(), Some("react"));
        assert!(result.is_likely_typosquat());
    }

    #[test]
    fn display_reports_both_reasons_when_both_fire() {
        let result = score_package("re\u{0430}ct");
        let rendered = result.to_string();
        assert!(rendered.contains("likely typosquat"));
        assert!(rendered.contains("edit distance 1 from popular package \"react\""));
        assert!(rendered.contains("homoglyph impersonation of popular package \"react\""));
    }

    #[test]
    fn adjacent_key_typo_is_reported_as_a_reason() {
        let result = score_package("reqiests");
        assert_eq!(result.distance_match, Some(("requests".to_string(), 1)));
        assert!(result.keyboard_typo);
        assert!(result.to_string().contains("adjacent-key typo of \"requests\""));
    }

    #[test]
    fn non_adjacent_substitution_is_not_a_keyboard_typo() {
        let result = score_package("reqzests");
        assert_eq!(result.distance_match, Some(("requests".to_string(), 1)));
        assert!(!result.keyboard_typo);
    }

    #[test]
    fn misspelled_scope_is_flagged_as_scope_confusion() {
        let result = score_package("@typess/node");
        assert_eq!(result.scope_confusion.as_deref(), Some("@types/node"));
        assert!(result.is_likely_typosquat());
        assert!(result
            .to_string()
            .contains("scope confusion with popular scoped package \"@types/node\""));
    }

    #[test]
    fn dropped_scope_is_flagged_as_scope_confusion() {
        let result = score_package("types-node");
        assert_eq!(result.scope_confusion.as_deref(), Some("@types/node"));
    }

    #[test]
    fn legitimate_scoped_package_is_not_flagged() {
        let result = score_package("@types/node");
        assert_eq!(result.scope_confusion, None);
        assert!(!result.is_likely_typosquat());
    }

    #[test]
    fn combosquat_is_flagged_with_reason() {
        let result = score_package("lodash-utils");
        assert_eq!(result.combosquat.as_deref(), Some("lodash"));
        assert!(result.is_likely_typosquat());
        assert!(result.to_string().contains("combosquat of popular package \"lodash\""));
    }

    #[test]
    fn real_multi_word_package_is_not_a_combosquat() {
        let result = score_package("react-dom");
        assert_eq!(result.combosquat, None);
    }

    #[test]
    fn display_reports_no_signal_for_clean_name() {
        let result = score_package("react");
        assert_eq!(result.to_string(), "react: no typosquat signal detected");
    }
}
