//! Lookalike-character (homoglyph) checks for typosquat detection.
//!
//! Edit distance ([`super::distance`]) catches names that are a keystroke or
//! two away from a popular one. It does *not* catch a name that is visually
//! identical but built from different code points — `paypal` spelled with a
//! Cyrillic `а` (U+0430) instead of a Latin `a`, or `express` typed in
//! full-width Latin (`ｅｘｐｒｅｓｓ`). To a reader those look like the real
//! package; to a registry they are a brand-new name.
//!
//! This module is the second, independent signal: it maps known lookalike
//! ("homoglyph") characters back to their Latin equivalents with
//! [`normalize_homoglyphs`], then asks whether that normalised form is the
//! name of a popular package ([`homoglyph_impersonation`]). It shares nothing
//! with `distance.rs` — no edit-distance math, no thresholds — and fires only
//! when a substitution actually happened.
//!
//! The confusable table below is a hand-picked subset of the Unicode
//! consortium's `confusables.txt` (UTR #39), limited to single-character
//! lowercase Latin lookalikes that realistically show up in package names:
//! Cyrillic and Greek letters, Armenian letters, a few IPA/Latin-extended
//! forms, plus the full-width ASCII block handled as a contiguous range. It is
//! deliberately conservative — every entry is a character an attacker would
//! reach for, not an exhaustive Unicode confusable dump.
//!
//! Digit-for-letter swaps (`l0dash`, `requ3sts`) are handled by a separate,
//! opt-in step — [`normalize_digit_swaps`] / [`digit_swap_impersonation`] —
//! rather than the main table, because plenty of real packages legitimately
//! contain digits (`d3`, `i18n`, `boto3`) and folding them unconditionally
//! would change those names too.

/// Maps a single character to its Latin equivalent if it is a known homoglyph,
/// otherwise returns it unchanged.
fn fold_homoglyph(c: char) -> char {
    // Full-width ASCII form (U+FF01..=U+FF5E) is a 1:1 shift of printable
    // ASCII (U+0021..=U+007E), so `ｅｘｐｒｅｓｓ` -> `express`, `ｌｏｄａｓｈ` -> `lodash`.
    if ('\u{FF01}'..='\u{FF5E}').contains(&c) {
        return char::from_u32(c as u32 - 0xFF01 + 0x21).unwrap_or(c);
    }

    match c {
        // Cyrillic letters that share a glyph with lowercase Latin.
        '\u{0430}' => 'a', // а CYRILLIC SMALL LETTER A
        '\u{0435}' => 'e', // е CYRILLIC SMALL LETTER IE
        '\u{043E}' => 'o', // о CYRILLIC SMALL LETTER O
        '\u{0440}' => 'p', // р CYRILLIC SMALL LETTER ER
        '\u{0441}' => 'c', // с CYRILLIC SMALL LETTER ES
        '\u{0443}' => 'y', // у CYRILLIC SMALL LETTER U
        '\u{0445}' => 'x', // х CYRILLIC SMALL LETTER HA
        '\u{0455}' => 's', // ѕ CYRILLIC SMALL LETTER DZE
        '\u{0456}' => 'i', // і CYRILLIC SMALL LETTER BYELORUSSIAN-UKRAINIAN I
        '\u{0458}' => 'j', // ј CYRILLIC SMALL LETTER JE
        '\u{04BB}' => 'h', // һ CYRILLIC SMALL LETTER SHHA
        '\u{04CF}' => 'l', // ӏ CYRILLIC SMALL LETTER PALOCHKA
        '\u{0501}' => 'd', // ԁ CYRILLIC SMALL LETTER KOMI DE
        '\u{051B}' => 'q', // ԛ CYRILLIC SMALL LETTER QA
        '\u{051D}' => 'w', // ԝ CYRILLIC SMALL LETTER WE
        '\u{0475}' => 'v', // ѵ CYRILLIC SMALL LETTER IZHITSA
        '\u{04AF}' => 'y', // ү CYRILLIC SMALL LETTER STRAIGHT U
        '\u{04C0}' => 'l', // Ӏ CYRILLIC LETTER PALOCHKA
        // Greek letters with Latin lookalikes.
        '\u{03BF}' => 'o', // ο GREEK SMALL LETTER OMICRON
        '\u{03B1}' => 'a', // α GREEK SMALL LETTER ALPHA
        '\u{03B9}' => 'i', // ι GREEK SMALL LETTER IOTA
        '\u{03BD}' => 'v', // ν GREEK SMALL LETTER NU
        '\u{03C1}' => 'p', // ρ GREEK SMALL LETTER RHO
        '\u{03C5}' => 'u', // υ GREEK SMALL LETTER UPSILON
        '\u{03BA}' => 'k', // κ GREEK SMALL LETTER KAPPA
        '\u{03C7}' => 'x', // χ GREEK SMALL LETTER CHI
        '\u{03B3}' => 'y', // γ GREEK SMALL LETTER GAMMA
        '\u{03F2}' => 'c', // ϲ GREEK LUNATE SIGMA SYMBOL
        '\u{03F3}' => 'j', // ϳ GREEK LETTER YOT
        // Armenian letters with Latin lookalikes.
        '\u{0585}' => 'o', // օ ARMENIAN SMALL LETTER OH
        '\u{0570}' => 'h', // հ ARMENIAN SMALL LETTER HO
        '\u{057D}' => 's', // ս ARMENIAN SMALL LETTER SEH
        '\u{0578}' => 'n', // ո ARMENIAN SMALL LETTER VO
        '\u{0581}' => 'g', // ց ARMENIAN SMALL LETTER CO
        '\u{0566}' => 'q', // զ ARMENIAN SMALL LETTER ZA
        '\u{0584}' => 'f', // ք ARMENIAN SMALL LETTER KEH
        // Cherokee small letters that render as Latin small capitals.
        '\u{ABAF}' => 'c', // ꮯ CHEROKEE SMALL LETTER TLI
        '\u{ABA9}' => 'v', // ꮩ CHEROKEE SMALL LETTER DO
        '\u{AB83}' => 'w', // ꮃ CHEROKEE SMALL LETTER LA
        '\u{AB93}' => 'z', // ꮓ CHEROKEE SMALL LETTER NO
        // Latin-extended / IPA forms that render as a plain Latin letter.
        '\u{0261}' => 'g', // ɡ LATIN SMALL LETTER SCRIPT G
        '\u{0131}' => 'i', // ı LATIN SMALL LETTER DOTLESS I
        '\u{0251}' => 'a', // ɑ LATIN SMALL LETTER ALPHA
        '\u{0269}' => 'i', // ɩ LATIN SMALL LETTER IOTA
        '\u{0237}' => 'j', // ȷ LATIN SMALL LETTER DOTLESS J
        // Latin small capitals that read as lowercase letters.
        '\u{1D0F}' => 'o', // ᴏ LATIN LETTER SMALL CAPITAL O
        '\u{1D1C}' => 'u', // ᴜ LATIN LETTER SMALL CAPITAL U
        '\u{1D20}' => 'v', // ᴠ LATIN LETTER SMALL CAPITAL V
        '\u{1D21}' => 'w', // ᴡ LATIN LETTER SMALL CAPITAL W
        '\u{1D22}' => 'z', // ᴢ LATIN LETTER SMALL CAPITAL Z
        other => other,
    }
}

/// Rewrites `candidate` with every known homoglyph replaced by its Latin
/// equivalent.
///
/// Characters that are not in the confusable table (including ordinary ASCII)
/// pass through untouched, so a name that never used a lookalike comes back
/// byte-for-byte identical. This is a pure text transform — it does not lower-
/// case, strip, or otherwise normalise beyond the homoglyph mapping.
pub fn normalize_homoglyphs(candidate: &str) -> String {
    candidate.chars().map(fold_homoglyph).collect()
}

/// Multi-character ASCII sequences that read as a single Latin letter, paired
/// with the letter they imitate.
const ASCII_LOOKALIKES: &[(&str, &str)] = &[
    ("rn", "m"), // `rnoment` -> `moment`
    ("vv", "w"), // `vvebpack` -> `webpack`
    ("cl", "d"), // `clebug` -> `debug`
    ("nn", "m"), // `nnocha` -> `mocha`
];

/// Returns every variant of `candidate` produced by replacing one or more
/// multi-character ASCII lookalikes (`rn`→`m`, `vv`→`w`, `cl`→`d`, `nn`→`m`)
/// with the letter they imitate.
///
/// Each occurrence is replaced independently, so `rnodern` yields `modern`,
/// `rnodem` and `modem`. The unchanged `candidate` is not included, and the
/// result is sorted and de-duplicated; a name with no lookalike sequences
/// returns an empty list.
pub fn normalize_ascii_lookalikes(candidate: &str) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut pending = vec![candidate.to_string()];

    while let Some(current) = pending.pop() {
        for (from, to) in ASCII_LOOKALIKES {
            for (index, _) in current.match_indices(from) {
                let variant = format!(
                    "{}{}{}",
                    &current[..index],
                    to,
                    &current[index + from.len()..]
                );
                if seen.insert(variant.clone()) {
                    pending.push(variant);
                }
            }
        }
    }

    seen.into_iter().collect()
}

/// Reports whether `candidate` looks like a homoglyph impersonation of a
/// popular package.
///
/// Returns `Some(name)` when *both* hold:
///
/// 1. [`normalize_homoglyphs`] changed `candidate` — at least one lookalike
///    character was actually substituted, and
/// 2. the normalised form exactly matches an entry in `dataset` (in practice
///    the combined list from
///    [`load_popular_packages`](super::dataset::load_popular_packages)).
///
/// A pure-ASCII name is never flagged by this rule even if it is itself a
/// popular package, because condition 1 fails: nothing was disguised.
///
/// Failing that, the normalised form is also checked for multi-character
/// ASCII lookalikes ([`normalize_ascii_lookalikes`]): if one of its variants
/// is a popular package, that package is returned. This is skipped when the
/// normalised form is itself in `dataset`, so a real name that happens to
/// contain `rn` or `cl` is never flagged as an impersonation of another.
///
/// Returns `None` otherwise. On a match the earliest `dataset` entry wins,
/// mirroring [`nearest_match`](super::distance::nearest_match).
pub fn homoglyph_impersonation(candidate: &str, dataset: &[String]) -> Option<String> {
    let normalized = normalize_homoglyphs(candidate);
    if normalized != candidate {
        if let Some(name) = dataset.iter().find(|name| *name == &normalized) {
            return Some(name.clone());
        }
    }

    if dataset.iter().any(|name| name == &normalized) {
        return None;
    }
    let variants = normalize_ascii_lookalikes(&normalized);
    dataset
        .iter()
        .find(|name| variants.iter().any(|variant| variant == *name))
        .cloned()
}

/// Maps a digit commonly swapped for a lookalike letter back to that letter,
/// otherwise returns `c` unchanged.
fn fold_digit(c: char) -> char {
    match c {
        '0' => 'o',
        '1' => 'l',
        '3' => 'e',
        '4' => 'a',
        '5' => 's',
        other => other,
    }
}

/// Rewrites `candidate` with every swappable digit (`0`, `1`, `3`, `4`, `5`)
/// replaced by the letter it imitates (`o`, `l`, `e`, `a`, `s`).
///
/// Separate from, and not applied by, [`normalize_homoglyphs`]: callers opt
/// in by calling this (or [`digit_swap_impersonation`]) explicitly.
pub fn normalize_digit_swaps(candidate: &str) -> String {
    candidate.chars().map(fold_digit).collect()
}

/// Reports whether `candidate` impersonates a popular package by swapping
/// letters for lookalike digits (`l0dash` -> `lodash`, `requ3sts` ->
/// `requests`).
///
/// The candidate is first passed through [`normalize_homoglyphs`] (so a name
/// mixing both tricks is still caught) and then [`normalize_digit_swaps`].
/// Returns `Some(name)` when *all* hold:
///
/// 1. the homoglyph-normalised candidate is not itself in `dataset` — a real
///    package whose name contains digits (`boto3`, `h5py`) is not flagged,
/// 2. digit folding changed the name — at least one digit was swapped, and
/// 3. the fully normalised form exactly matches an entry in `dataset`.
///
/// On a match the earliest `dataset` entry wins.
pub fn digit_swap_impersonation(candidate: &str, dataset: &[String]) -> Option<String> {
    let letters = normalize_homoglyphs(candidate);
    if dataset.iter().any(|name| name == &letters) {
        return None;
    }
    let normalized = normalize_digit_swaps(&letters);
    if normalized == letters {
        return None;
    }
    dataset.iter().find(|name| *name == &normalized).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typosquat::dataset::load_popular_packages;

    #[test]
    fn normalize_is_identity_for_plain_ascii() {
        assert_eq!(normalize_homoglyphs("react"), "react");
        assert_eq!(normalize_homoglyphs("@angular/core"), "@angular/core");
        assert_eq!(normalize_homoglyphs(""), "");
    }

    #[test]
    fn normalize_maps_cyrillic_lookalikes_to_latin() {
        // "react" with a Cyrillic 'а' (U+0430) in place of the Latin 'a'.
        assert_eq!(normalize_homoglyphs("re\u{0430}ct"), "react");
        // "lodash" with Cyrillic 'о' (U+043E) and 'а' (U+0430).
        assert_eq!(normalize_homoglyphs("l\u{043E}d\u{0430}sh"), "lodash");
    }

    #[test]
    fn normalize_maps_fullwidth_latin_to_ascii() {
        // "express" typed entirely in the full-width Latin block (U+FF45 ...).
        assert_eq!(
            normalize_homoglyphs("\u{FF45}\u{FF58}\u{FF50}\u{FF52}\u{FF45}\u{FF53}\u{FF53}"),
            "express"
        );
    }

    #[test]
    fn normalize_maps_greek_lookalikes_to_latin() {
        // "commander" with its first 'o' swapped for a Greek omicron 'ο' (U+03BF).
        assert_eq!(normalize_homoglyphs("c\u{03BF}mmander"), "commander");
    }

    #[test]
    fn impersonation_flags_cyrillic_a_in_npm_package() {
        let dataset = load_popular_packages();
        // "react" (npm) with a Cyrillic 'а' (U+0430).
        assert_eq!(
            homoglyph_impersonation("re\u{0430}ct", &dataset).as_deref(),
            Some("react")
        );
    }

    #[test]
    fn impersonation_flags_fullwidth_pypi_package() {
        let dataset = load_popular_packages();
        // "requests" (PyPI) written in full-width Latin.
        let fullwidth = "\u{FF52}\u{FF45}\u{FF51}\u{FF55}\u{FF45}\u{FF53}\u{FF54}\u{FF53}";
        assert_eq!(
            homoglyph_impersonation(fullwidth, &dataset).as_deref(),
            Some("requests")
        );
    }

    #[test]
    fn impersonation_flags_greek_omicron_in_npm_package() {
        let dataset = load_popular_packages();
        // "lodash" (npm) with its 'o' swapped for a Greek omicron 'ο' (U+03BF).
        assert_eq!(
            homoglyph_impersonation("l\u{03BF}dash", &dataset).as_deref(),
            Some("lodash")
        );
    }

    #[test]
    fn impersonation_ignores_plain_ascii_popular_package() {
        let dataset = load_popular_packages();
        // "react" spelled correctly is the real package, not an impersonation.
        assert_eq!(homoglyph_impersonation("react", &dataset), None);
    }

    #[test]
    fn impersonation_ignores_homoglyph_name_that_is_not_popular() {
        let dataset = load_popular_packages();
        // Normalises to "reactxyzzy", which is not in the dataset.
        assert_eq!(homoglyph_impersonation("re\u{0430}ctxyzzy", &dataset), None);
    }
}
