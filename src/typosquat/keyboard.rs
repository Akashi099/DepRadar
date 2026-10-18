//! Keyboard-adjacency signal: was a substitution one slip of the finger?
//!
//! A typo that hits a neighbouring key (`reqiests`, where `i` sits next to
//! `u`) is a far likelier accidental install than a random substitution
//! (`reqzests`). [`keyboard_typo`] recognises that case using a QWERTY
//! adjacency map. Like the other signals it makes no scoring decision on its
//! own; the scanner reports it as an extra reason alongside a distance match.

/// QWERTY layout, one row per entry. Keys are adjacent when they are next to
/// each other in a row, or in a neighbouring row at the same or the next
/// position (rows are staggered by roughly half a key).
const QWERTY_ROWS: [&str; 4] = ["1234567890-", "qwertyuiop", "asdfghjkl", "zxcvbnm"];

/// Returns the (row, column) of `c` on the QWERTY layout, ignoring case.
fn key_position(c: char) -> Option<(usize, usize)> {
    let c = c.to_ascii_lowercase();
    QWERTY_ROWS
        .iter()
        .enumerate()
        .find_map(|(row, keys)| keys.find(c).map(|col| (row, col)))
}

/// True when `a` and `b` are different keys that touch on a QWERTY keyboard.
pub fn keys_adjacent(a: char, b: char) -> bool {
    let (Some((row_a, col_a)), Some((row_b, col_b))) = (key_position(a), key_position(b)) else {
        return false;
    };
    if (row_a, col_a) == (row_b, col_b) {
        return false;
    }
    if row_a == row_b {
        return col_a.abs_diff(col_b) == 1;
    }
    // Each row is shifted right by about half a key relative to the one
    // above, so a key touches the upper row's keys at the same column and
    // one to the right.
    let (upper_col, lower_col) = match row_b.checked_sub(row_a) {
        Some(1) => (col_a, col_b),
        _ if row_a.checked_sub(row_b) == Some(1) => (col_b, col_a),
        _ => return false,
    };
    upper_col == lower_col || upper_col == lower_col + 1
}

/// True when the only difference between `candidate` and `target` is a
/// single substitution of a key adjacent on a QWERTY keyboard.
///
/// Both names must have the same length and differ in exactly one position.
/// `keyboard_typo("reqiests", "requests")` is true (`i` is next to `u`);
/// `keyboard_typo("reqzests", "requests")` is false.
pub fn keyboard_typo(candidate: &str, target: &str) -> bool {
    let candidate: Vec<char> = candidate.chars().collect();
    let target: Vec<char> = target.chars().collect();
    if candidate.len() != target.len() {
        return false;
    }

    let mut differences = candidate.iter().zip(&target).filter(|(c, t)| c != t);
    match (differences.next(), differences.next()) {
        (Some((&c, &t)), None) => keys_adjacent(c, t),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjacent_key_substitution_is_a_keyboard_typo() {
        assert!(keyboard_typo("reqiests", "requests"));
    }

    #[test]
    fn distant_key_substitution_is_not_a_keyboard_typo() {
        assert!(!keyboard_typo("reqzests", "requests"));
    }

    #[test]
    fn identical_names_are_not_a_keyboard_typo() {
        assert!(!keyboard_typo("requests", "requests"));
    }

    #[test]
    fn insertions_and_multiple_edits_are_not_keyboard_typos() {
        assert!(!keyboard_typo("expresss", "express"));
        assert!(!keyboard_typo("rewuests", "rwquests"));
    }

    #[test]
    fn adjacency_covers_rows_and_neighbouring_rows() {
        assert!(keys_adjacent('u', 'i'));
        assert!(keys_adjacent('u', 'j'));
        assert!(keys_adjacent('u', 'h'));
        assert!(keys_adjacent('a', 'z'));
        assert!(!keys_adjacent('u', 'z'));
        assert!(!keys_adjacent('q', 'p'));
    }
}
