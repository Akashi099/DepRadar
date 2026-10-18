# Issue #23 — Count Adjacent Transpositions as One Edit (Damerau–Levenshtein)

## Problem

`levenshtein_distance` in `src/typosquat/distance.rs` implements standard Levenshtein — insert, delete, substitute only. It has no transposition operation, so swapping two adjacent characters (e.g. `exrpess` → `express`) costs **2** edits (one delete + one insert). The existing test in the module explicitly asserts this:

```rust
#[test]
fn distance_counts_transposition_as_two_edits() {
    assert_eq!(levenshtein_distance("recat", "react"), 2);
}
```

Adjacent-character transpositions are among the most common real-world typosquatting patterns (`exrpess`, `reqeusts`, `lodahs`). With `DISTANCE_THRESHOLD = 2` a single-swap typosquat can still be caught today, but only because it "uses up" both allowed edits. A two-swap typosquat like `rqeusets` (distance 4 by Levenshtein) slips through entirely.

## Goal

Add `damerau_levenshtein_distance(a, b)` using the **Optimal String Alignment (OSA)** variant of Damerau–Levenshtein. OSA counts an adjacent transposition as a single operation, reducing `exrpess` → `express` from 2 to 1. Wire it into `nearest_match` and `score_package`; keep the original `levenshtein_distance` function available for comparison and existing tests.

## Affected Files

| File | Change |
|---|---|
| `src/typosquat/distance.rs` | Add `damerau_levenshtein_distance`; update `nearest_match` to use it |
| `src/typosquat/scanner.rs` | `score_package` calls `nearest_match` — no direct change needed, but the existing transposition test must be updated |
| `PROGRESS.md` | Add entry |

## Algorithm — Optimal String Alignment

OSA extends the standard Levenshtein DP table with one extra case: when `a[i] == b[j-1]` and `a[i-1] == b[j]` (i.e. adjacent characters are swapped), the transposition cost is `prev_prev[j-1] + 1`, where `prev_prev` is the DP row two steps back.

OSA is simpler than the full Damerau–Levenshtein (which handles multi-character transpositions and is triangle-inequality compliant) but is sufficient for the single-swap typosquat detection use case and requires only one additional row of storage.

### Implementation

```rust
/// Optimal String Alignment (OSA) variant of Damerau–Levenshtein distance.
///
/// Counts insertions, deletions, substitutions, and **adjacent transpositions**
/// each as a single edit. This makes `exrpess` → `express` distance 1 rather
/// than the 2 that standard Levenshtein returns.
///
/// OSA is not a true metric (it violates the triangle inequality for some
/// inputs) but is correct for the single-swap typosquat detection use case.
/// Operates on Unicode scalar values; runs in O(a.len() × b.len()) time and
/// O(b.len()) extra space with an additional row for the transposition look-back.
pub fn damerau_levenshtein_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();

    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }

    // Three rows: two rows back (pp), one row back (prev), current (curr).
    let mut pp: Vec<usize> = vec![0; b.len() + 1]; // d[i-2][*], only needed for transposition
    let mut prev: Vec<usize> = (0..=b.len()).collect(); // d[i-1][*]
    let mut curr: Vec<usize> = vec![0; b.len() + 1];   // d[i][*]

    for (i, &ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let sub_cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1)          // delete a[i]
                .min(curr[j] + 1)                      // insert b[j]
                .min(prev[j] + sub_cost);              // substitute / match

            // Transposition: a[i-1] == b[j] && a[i] == b[j-1]
            if i > 0 && j > 0 && ca == b[j - 1] && a[i - 1] == cb {
                curr[j + 1] = curr[j + 1].min(pp[j - 1] + 1);
            }
        }
        std::mem::swap(&mut pp, &mut prev);
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[b.len()]
}
```

### Update `nearest_match`

Replace the `levenshtein_distance` call with `damerau_levenshtein_distance`:

```rust
pub fn nearest_match(candidate: &str, dataset: &[String]) -> (String, usize) {
    let mut best: Option<(&String, usize)> = None;

    for name in dataset {
        let distance = damerau_levenshtein_distance(candidate, name); // was levenshtein_distance
        if best.is_none_or(|(_, best_distance)| distance < best_distance) {
            best = Some((name, distance));
            if distance == 0 {
                break;
            }
        }
    }

    match best {
        Some((name, distance)) => (name.clone(), distance),
        None => (String::new(), usize::MAX),
    }
}
```

`levenshtein_distance` remains in the file, unchanged, for direct comparison and the existing tests that depend on its 2-edit transposition behaviour.

## Test Updates

### Existing test that must be updated

The current `distance_counts_transposition_as_two_edits` test asserts Levenshtein behaviour and stays as-is — it tests `levenshtein_distance` directly, which is unchanged. A new parallel test is added for the OSA function:

```rust
#[test]
fn damerau_counts_adjacent_transposition_as_one_edit() {
    // swap r and p: exrpess → express
    assert_eq!(damerau_levenshtein_distance("exrpess", "express"), 1);
    // swap e and u: reqeusts → requests
    assert_eq!(damerau_levenshtein_distance("reqeusts", "requests"), 1);
}

#[test]
fn damerau_identical_and_empty_cases_unchanged() {
    assert_eq!(damerau_levenshtein_distance("express", "express"), 0);
    assert_eq!(damerau_levenshtein_distance("", ""), 0);
    assert_eq!(damerau_levenshtein_distance("", "react"), 5);
    assert_eq!(damerau_levenshtein_distance("react", ""), 5);
}
```

The issue's acceptance criteria require exactly:
- `exrpess` → `express` = **1**
- `reqeusts` → `requests` = **1**
- Identical / empty cases unchanged (= 0, = 0, = n, = n as above)

### Scanner-level transposition test

Since `nearest_match` now uses the OSA distance, a transposition typosquat will be caught at distance 1 rather than 2. The existing scanner test `distance_only_match_is_flagged_with_reason` uses `"expresss"` (one extra character, distance 1) — it is unaffected. If any scanner test directly asserts a transposition result (none currently do), it should be updated to reflect the new distance.

## Acceptance Criteria Mapping

| Criterion | How it is met |
|---|---|
| `exrpess→express = 1` | `damerau_levenshtein_distance("exrpess", "express") == 1` |
| `reqeusts→requests = 1` | `damerau_levenshtein_distance("reqeusts", "requests") == 1` |
| Identical / empty unchanged | New `damerau_identical_and_empty_cases_unchanged` test |
| Old transposition test updated | Old test kept unchanged (tests `levenshtein_distance`, not OSA); OSA behaviour covered by new test |
| `cargo fmt`, `clippy -D warnings`, `cargo test --workspace` | Standard CI gates |
| `PROGRESS.md` entry + `Closes #23` in PR | Required before merge |

## Implementation Checklist

- [ ] Add `damerau_levenshtein_distance` to `src/typosquat/distance.rs`
- [ ] Update `nearest_match` to call `damerau_levenshtein_distance`
- [ ] Add `damerau_counts_adjacent_transposition_as_one_edit` test
- [ ] Add `damerau_identical_and_empty_cases_unchanged` test
- [ ] Add PROGRESS.md entry — closes #23

## Notes

- **Why OSA and not full Damerau–Levenshtein?** Full DL is a true metric and handles overlapping transpositions correctly, but it requires O(|Σ| × n) space for a last-occurrence table per character. OSA requires only one extra DP row and is correct for all realistic single-swap typosquat inputs.
- **Why keep `levenshtein_distance`?** The issue explicitly asks to keep it for comparison. Existing tests depend on its 2-edit transposition behaviour, and it serves as documentation of what changed.
- **`DISTANCE_THRESHOLD` stays at 2.** Transpositions now cost 1 instead of 2, giving the threshold more headroom to catch multi-error names while still excluding unrelated packages.
