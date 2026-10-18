# Issue #25 — Normalize PyPI Names per PEP 503 Before Comparing

## Problem

PyPI treats `Foo_Bar`, `foo-bar`, and `foo.bar` as the same project per [PEP 503](https://peps.python.org/pep-0503/#normalized-names). The normalization rule is: lowercase the name, then collapse any run of `-`, `_`, or `.` into a single `-`.

Today the typosquat scanner compares candidate names against the dataset without applying this normalization. As a result:

- `Python_Dateutil` scores as **7 edits** from `python-dateutil` (the canonical form) when it should be **0** (same package, different spelling).
- `zope.interface` scores as **1 edit** from `zope-interface` when it should be **0**.
- A PyPI attacker publishing `python.dateutil` would slip past the distance check entirely (distance 3 from `python-dateutil`) when the normalized distance is 0.

This inflates distances for legitimate package aliases, causing both false positives (close-but-equivalent names flagged as typosquats) and false negatives (separator-swap attacks that exceed `DISTANCE_THRESHOLD`).

## Goal

Add `normalize_pypi_name(&str) -> String` in `src/typosquat/dataset.rs` and apply it to both the candidate name and every PyPI dataset entry at scoring time.

## Affected Files

| File | Change |
|---|---|
| `src/typosquat/dataset.rs` | Add `normalize_pypi_name`; apply it when loading PyPI entries |
| `src/typosquat/scanner.rs` | Pass normalized candidate name when scoring against the PyPI list |
| `PROGRESS.md` | Add entry |

## Design

### `normalize_pypi_name`

PEP 503's canonical normalization in two steps:

1. Lowercase the string.
2. Replace every run of one or more `-`, `_`, or `.` characters with a single `-`.

```rust
/// Normalises a PyPI package name following PEP 503.
///
/// PEP 503 says: lowercase the name, then replace any run of `-`, `_`, or
/// `.` with a single `-`. This means `Foo_Bar`, `foo-bar`, and `foo.bar` all
/// normalise to `foo-bar` and are treated as the same package by the registry.
///
/// Applying this before edit-distance comparison prevents separator variants
/// from inflating distances and causing false positives or missed detections.
pub fn normalize_pypi_name(name: &str) -> String {
    let lower = name.to_lowercase();
    // Collapse runs of -, _, . into a single -
    let mut result = String::with_capacity(lower.len());
    let mut in_sep = false;
    for c in lower.chars() {
        if matches!(c, '-' | '_' | '.') {
            if !in_sep {
                result.push('-');
                in_sep = true;
            }
        } else {
            result.push(c);
            in_sep = false;
        }
    }
    result
}
```

This runs in O(n) time with one allocation, no regex dependency required.

### Where to apply normalization

Normalization must be applied symmetrically — to both the candidate being scored and the dataset entries being compared against. Otherwise `python-dateutil` (already normalized, in the dataset) and `Python_Dateutil` (candidate, not normalized) would still differ.

**Option A — Normalize at load time (preferred)**

When `load_popular_packages_for(Ecosystem::PyPI)` (from issue #24) or the PyPI portion of `load_popular_packages()` is built, apply `normalize_pypi_name` to every entry. Then normalize the candidate at the call site in `score_package` / `score_package_in` before passing it to `nearest_match`.

```rust
// In dataset.rs — normalize PyPI entries at parse time:
Ecosystem::PyPI => {
    static PYPI: OnceLock<Vec<String>> = OnceLock::new();
    PYPI.get_or_init(|| {
        parse_string_array(PYPI_TOP_PACKAGES_JSON)
            .into_iter()
            .map(|name| normalize_pypi_name(&name))
            .collect()
    })
}
```

```rust
// In scanner.rs — normalize the candidate before scoring against PyPI list:
pub fn score_package_in(name: &str, eco: Ecosystem) -> ScanResult {
    let dataset = load_popular_packages_for(eco);

    // Apply PEP 503 normalization when comparing against the PyPI dataset.
    let effective_name = match eco {
        Ecosystem::PyPI => normalize_pypi_name(name),
        _               => name.to_string(),
    };

    let (nearest_name, distance) = nearest_match(&effective_name, dataset);
    let distance_match =
        (distance > 0 && distance <= DISTANCE_THRESHOLD).then_some((nearest_name, distance));

    let homoglyph_match = homoglyph_impersonation(&effective_name, dataset);

    ScanResult {
        candidate: name.to_string(), // always report the original name
        distance_match,
        homoglyph_match,
    }
}
```

Note that `ScanResult::candidate` always stores the original name given by the caller — not the normalized form — so output remains human-readable.

**Option B — Normalize in `nearest_match`**

Passing a normalization hook into `nearest_match` would generalize it but couples the distance module to ecosystem logic. Option A keeps normalization at the dataset/scanner boundary, which is the right layer for this concern.

### Combined dataset (`load_popular_packages`)

The combined list is used by `score_package(name)` (any-ecosystem mode). Applying PyPI normalization to the combined list would affect npm entries. Instead, `load_popular_packages()` should apply normalization only to the PyPI slice:

```rust
pub fn load_popular_packages() -> &'static [String] {
    static POPULAR_PACKAGES: OnceLock<Vec<String>> = OnceLock::new();
    POPULAR_PACKAGES.get_or_init(|| {
        let mut combined = parse_string_array(NPM_TOP_PACKAGES_JSON);
        combined.extend(
            parse_string_array(PYPI_TOP_PACKAGES_JSON)
                .into_iter()
                .map(|name| normalize_pypi_name(&name))
        );
        combined
    })
}
```

Npm names do not have a registry-level normalization rule equivalent to PEP 503, so they are left as-is.

## Acceptance Criteria Mapping

### Required test cases

| Input | Expected output |
|---|---|
| `"Python_Dateutil"` | `"python-dateutil"` |
| `"zope.interface"` | `"zope-interface"` |
| `"a__b..c"` | `"a-b-c"` |

```rust
#[test]
fn normalize_pypi_name_applies_pep503_rules() {
    assert_eq!(normalize_pypi_name("Python_Dateutil"), "python-dateutil");
    assert_eq!(normalize_pypi_name("zope.interface"),  "zope-interface");
    assert_eq!(normalize_pypi_name("a__b..c"),          "a-b-c");
}

#[test]
fn normalize_pypi_name_is_idempotent_on_canonical_form() {
    // Already-normalized names should be returned unchanged.
    assert_eq!(normalize_pypi_name("python-dateutil"), "python-dateutil");
    assert_eq!(normalize_pypi_name("requests"),         "requests");
}

#[test]
fn normalize_pypi_name_handles_empty_string() {
    assert_eq!(normalize_pypi_name(""), "");
}
```

### Scanner-level regression

A PyPI package variant using a separator should score as distance 0 (same package) and therefore **not** be flagged:

```rust
#[test]
fn pypi_separator_variant_is_not_flagged_as_typosquat() {
    // "python_dateutil" is the same package as "python-dateutil" under PEP 503.
    // After normalization both forms are "python-dateutil"; distance = 0 → not flagged.
    let result = score_package_in("python_dateutil", Ecosystem::PyPI);
    assert_eq!(result.distance_match, None);
    assert!(!result.is_likely_typosquat());
}
```

| Criterion | How it is met |
|---|---|
| `normalize_pypi_name` lowercases and collapses separators | `normalize_pypi_name_applies_pep503_rules` test |
| Applied to candidate and dataset entries | Option A — normalization in `load_popular_packages_for` + `score_package_in` |
| `Python_Dateutil → python-dateutil` | Covered by test |
| `zope.interface → zope-interface` | Covered by test |
| `a__b..c → a-b-c` | Covered by test |
| `cargo fmt`, `clippy -D warnings`, `cargo test --workspace` | Standard CI gates |
| `PROGRESS.md` entry + `Closes #25` in PR | Required before merge |

## Implementation Checklist

- [ ] Add `normalize_pypi_name(name: &str) -> String` to `src/typosquat/dataset.rs` (make it `pub`)
- [ ] Apply normalization to PyPI entries in `load_popular_packages()` and `load_popular_packages_for(Ecosystem::PyPI)`
- [ ] Apply normalization to the candidate in `score_package_in` for `Ecosystem::PyPI`
- [ ] Add `normalize_pypi_name_applies_pep503_rules` test
- [ ] Add `normalize_pypi_name_is_idempotent_on_canonical_form` test
- [ ] Add `normalize_pypi_name_handles_empty_string` test
- [ ] Add `pypi_separator_variant_is_not_flagged_as_typosquat` scanner test
- [ ] Add PROGRESS.md entry — closes #25

## Notes

- **Dependency order:** This issue depends on issue #24 (`score_package_in` / `load_popular_packages_for`) to apply normalization per-ecosystem cleanly. If #24 is not yet merged, normalization can be applied in `dataset.rs` when building the PyPI slice for the combined list, and a separate `score_package_pypi(name)` helper can stand in temporarily.
- **npm names:** npm names are case-sensitive and do not have an equivalent normalization rule. Do not apply PEP 503 normalization to npm entries.
- **No regex dependency:** The hand-rolled loop is simple, allocation-efficient, and avoids adding a crate dependency. The bundled JSON data is entirely under project control, so no edge-case Unicode separator handling is needed beyond `-`, `_`, `.`.
- **`normalize_pypi_name` should be `pub`** so it is accessible to integration tests and any future PyPI-specific scanner code without having to duplicate the logic.
- **Reporting:** `ScanResult::candidate` stores the original caller-supplied name, not the normalized form, so the output shown to users is the name they typed.
