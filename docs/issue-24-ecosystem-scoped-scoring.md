# Issue #24 — Score Names Only Against Their Own Ecosystem's Dataset

## Problem

`load_popular_packages()` in `src/typosquat/dataset.rs` merges the npm and PyPI lists into one combined `Vec<String>`. `score_package()` in `src/typosquat/scanner.rs` scores a candidate against this combined list. This means a PyPI package can be flagged as a typosquat of an npm package (or vice versa) — a cross-ecosystem false positive. For example, a PyPI package whose name is one edit away from a popular npm package name would be flagged even though the two ecosystems are completely separate registries.

## Goal

Add ecosystem-scoped dataset access and scoring functions. Keep the existing `load_popular_packages()` and `score_package()` for the any-ecosystem / combined behaviour. Add `--ecosystem npm|pypi` to the `scan` binary so callers can opt into scoped scoring.

## Affected Files

| File | Change |
|---|---|
| `src/typosquat/dataset.rs` | Add `load_popular_packages_for(eco: Ecosystem)` |
| `src/typosquat/scanner.rs` | Add `score_package_in(name, eco)`; import `Ecosystem` |
| `src/bin/scan.rs` | Add `--ecosystem npm\|pypi` CLI flag |
| `PROGRESS.md` | Add entry |

## Design

### Reuse the existing `Ecosystem` enum

`Ecosystem` already exists in `src/cve/osv_client.rs`:

```rust
pub enum Ecosystem {
    Npm,
    PyPI,
    CratesIo,
}
```

Import it directly rather than introducing a duplicate. The typosquat module only needs `Npm` and `PyPI` variants for now; `CratesIo` can be added later when a crates.io list is shipped.

```rust
// In dataset.rs and scanner.rs:
use crate::cve::osv_client::Ecosystem;
```

### `load_popular_packages_for` in `dataset.rs`

Returns only the entries that belong to the requested registry. The two embedded JSON constants remain, so no new data files are needed:

```rust
/// Returns the popular-package list for a single ecosystem.
///
/// Use [`load_popular_packages`] when you want the combined npm + PyPI
/// dataset (the any-ecosystem default). This function is for callers that
/// already know which registry they are scanning and want to avoid
/// cross-ecosystem false positives.
///
/// `Ecosystem::CratesIo` is not yet supported (no bundled list); it returns
/// an empty slice and should be extended when a crates.io list is added.
pub fn load_popular_packages_for(eco: Ecosystem) -> &'static [String] {
    match eco {
        Ecosystem::Npm => {
            static NPM: OnceLock<Vec<String>> = OnceLock::new();
            NPM.get_or_init(|| parse_string_array(NPM_TOP_PACKAGES_JSON))
        }
        Ecosystem::PyPI => {
            static PYPI: OnceLock<Vec<String>> = OnceLock::new();
            PYPI.get_or_init(|| parse_string_array(PYPI_TOP_PACKAGES_JSON))
        }
        Ecosystem::CratesIo => &[], // no bundled list yet
    }
}
```

Each per-ecosystem list is cached in its own `OnceLock` (consistent with the approach from issue #22). The combined `POPULAR_PACKAGES` cache from issue #22 remains separate — `load_popular_packages()` delegates to it.

### `score_package_in` in `scanner.rs`

Mirrors `score_package` but scopes the dataset to a single ecosystem:

```rust
/// Scores `name` against the popular-package dataset for a specific ecosystem.
///
/// Use this instead of [`score_package`] when the registry of the package
/// being scanned is known. This prevents cross-ecosystem false positives
/// (e.g. a PyPI package being flagged as a typosquat of an npm package).
///
/// `score_package(name)` — which scores against the combined npm + PyPI
/// dataset — is preserved as the any-ecosystem fallback.
pub fn score_package_in(name: &str, eco: Ecosystem) -> ScanResult {
    let dataset = load_popular_packages_for(eco);

    let (nearest_name, distance) = nearest_match(name, dataset);
    let distance_match =
        (distance > 0 && distance <= DISTANCE_THRESHOLD).then_some((nearest_name, distance));

    let homoglyph_match = homoglyph_impersonation(name, dataset);

    ScanResult {
        candidate: name.to_string(),
        distance_match,
        homoglyph_match,
    }
}
```

`score_package` is unchanged:

```rust
pub fn score_package(name: &str) -> ScanResult {
    score_package_in(name, /* any-ecosystem — use combined list */)
    // ... or keep the existing implementation as-is
}
```

Both functions are `pub`; callers that do not know the ecosystem use `score_package`; callers that do use `score_package_in`.

### CLI flag in `src/bin/scan.rs`

Add `--ecosystem npm|pypi` as an optional flag. When omitted, the binary behaves as before (combined dataset):

```rust
use deprader::cve::osv_client::Ecosystem;
use deprader::typosquat::scanner::{score_package, score_package_in};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    let name = match args.iter().position(|a| !a.starts_with("--")).and_then(|_| args.get(1)) {
        Some(n) if !n.starts_with("--") => n.clone(),
        _ => {
            eprintln!("usage: scan <package-name> [--ecosystem npm|pypi]");
            return ExitCode::FAILURE;
        }
    };

    // Parse optional --ecosystem flag
    let eco: Option<Ecosystem> = args.windows(2).find_map(|w| {
        if w[0] == "--ecosystem" {
            match w[1].to_lowercase().as_str() {
                "npm"  => Some(Ecosystem::Npm),
                "pypi" => Some(Ecosystem::PyPI),
                other  => {
                    eprintln!("unknown ecosystem: {other}; valid values are npm, pypi");
                    None
                }
            }
        } else {
            None
        }
    });

    let result = match eco {
        Some(e) => score_package_in(&name, e),
        None    => score_package(&name),
    };

    println!("{result}");
    ExitCode::SUCCESS
}
```

Usage examples:
```
cargo run --bin scan -- expresss
cargo run --bin scan -- expresss --ecosystem npm
cargo run --bin scan -- requests --ecosystem pypi
```

## Acceptance Criteria Mapping

### Cross-ecosystem false-positive test

The issue requires a test showing a PyPI-only near-match is **not** flagged when scanning with `Ecosystem::Npm`:

```rust
#[test]
fn pypi_near_match_is_not_flagged_for_npm_ecosystem() {
    // Find a name that is within DISTANCE_THRESHOLD of a PyPI package
    // but not within threshold of any npm package.
    // "requestz" is 1 edit from "requests" (PyPI) but not close to any npm top package.
    let result = score_package_in("requestz", Ecosystem::Npm);
    assert_eq!(
        result.distance_match, None,
        "a PyPI-only near-match should not be flagged when scanning the npm ecosystem"
    );
    assert_eq!(result.homoglyph_match, None);
    assert!(!result.is_likely_typosquat());
}
```

And the complementary test showing it **is** flagged for PyPI:

```rust
#[test]
fn pypi_near_match_is_flagged_for_pypi_ecosystem() {
    let result = score_package_in("requestz", Ecosystem::PyPI);
    assert!(
        result.distance_match.is_some(),
        "a name 1 edit from 'requests' should be flagged in the PyPI ecosystem"
    );
}
```

| Criterion | How it is met |
|---|---|
| PyPI-only near-match not flagged for `Ecosystem::Npm` | `pypi_near_match_is_not_flagged_for_npm_ecosystem` test |
| `load_popular_packages_for(eco)` added | Implemented in `dataset.rs` |
| `score_package_in(name, eco)` added | Implemented in `scanner.rs` |
| `score_package(name)` preserved | Unchanged any-ecosystem behaviour |
| `--ecosystem npm\|pypi` added to scan binary | `src/bin/scan.rs` updated |
| `cargo fmt`, `clippy -D warnings`, `cargo test --workspace` | Standard CI gates |
| `PROGRESS.md` entry + `Closes #24` in PR | Required before merge |

## Implementation Checklist

- [ ] Import `Ecosystem` from `crate::cve::osv_client` in `dataset.rs` and `scanner.rs`
- [ ] Add per-ecosystem `OnceLock` statics in `dataset.rs`
- [ ] Implement `load_popular_packages_for(eco: Ecosystem) -> &'static [String]`
- [ ] Implement `score_package_in(name: &str, eco: Ecosystem) -> ScanResult` in `scanner.rs`
- [ ] Update `src/bin/scan.rs` with `--ecosystem` flag
- [ ] Add `pypi_near_match_is_not_flagged_for_npm_ecosystem` test
- [ ] Add complementary `pypi_near_match_is_flagged_for_pypi_ecosystem` test
- [ ] Add PROGRESS.md entry — closes #24

## Notes

- `Ecosystem::CratesIo` returns `&[]` for now. `nearest_match` on an empty dataset already returns `(String::new(), usize::MAX)`, so `score_package_in(name, Ecosystem::CratesIo)` will always return a `ScanResult` with both fields `None` — a safe no-op.
- The `homoglyph_impersonation` function in `homoglyph.rs` takes `&[String]`, so it transparently benefits from the ecosystem-scoped dataset without any changes.
- Do not re-export `Ecosystem` from the typosquat module — keep a single canonical definition in `cve::osv_client` and import from there.
- If issue #22 (OnceLock caching) lands first, the per-ecosystem statics here follow the same pattern and the combined `POPULAR_PACKAGES` cache remains independent.
