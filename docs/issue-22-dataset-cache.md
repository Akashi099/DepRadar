# Issue #22 — Cache the Popular-Package Dataset Instead of Reloading on Every Scan

## Problem

`score_package()` in `src/typosquat/scanner.rs` calls `load_popular_packages()` on every invocation. Each call re-parses ~2,000 names from the two embedded JSON arrays via `parse_string_array` in `src/typosquat/dataset.rs`. When scanning a lockfile with hundreds of dependencies this work is repeated hundreds of times, even though the dataset is compile-time-embedded and never changes at runtime.

## Goal

Parse the combined npm + PyPI dataset exactly once per process lifetime and return a shared, static reference on every subsequent call. The public API of `load_popular_packages()` must be preserved so callers (scanner, homoglyph module, tests) need no changes.

## Affected Files

| File | Change |
|---|---|
| `src/typosquat/dataset.rs` | Add `OnceLock` cache; update `load_popular_packages` to use it |
| `src/typosquat/scanner.rs` | Update `score_package` to accept `&[String]` from the cached slice |
| `PROGRESS.md` | Add entry |

## Design

### Cache with `std::sync::OnceLock`

`OnceLock<T>` (stabilised in Rust 1.70, no external dependency) initialises its value exactly once across threads and then returns a shared reference for the lifetime of the process. Because the JSON is embedded at compile time, parsing is pure CPU work with no I/O — it is safe to do inside `OnceLock::get_or_init`.

```rust
use std::sync::OnceLock;

/// Process-wide cache of the parsed popular-package list.
/// Initialised once on first call; all subsequent calls return the same slice.
static POPULAR_PACKAGES: OnceLock<Vec<String>> = OnceLock::new();

/// Returns the combined npm + PyPI popular-package list.
///
/// The list is parsed from the embedded JSON files the first time this
/// function is called; every subsequent call returns a reference to the
/// same allocation. Callers may hold the returned slice for any duration —
/// the backing `Vec` lives for the lifetime of the process.
pub fn load_popular_packages() -> &'static [String] {
    POPULAR_PACKAGES.get_or_init(|| {
        let mut combined = parse_string_array(NPM_TOP_PACKAGES_JSON);
        combined.extend(parse_string_array(PYPI_TOP_PACKAGES_JSON));
        combined
    })
}
```

The return type changes from `Vec<String>` to `&'static [String]`. This is a source-compatible improvement for all existing callers that iterate or index the slice; the only adjustment needed is at call sites that previously called `.len()`, `.iter()`, or used the value in a `for` loop — all of which work identically on `&[String]`.

### Update `score_package`

`score_package` currently owns a `Vec<String>` from `load_popular_packages()`. After the change it holds a `&'static [String]`:

```rust
pub fn score_package(name: &str) -> ScanResult {
    let dataset = load_popular_packages(); // now &'static [String]

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

`nearest_match` already takes `&[String]`, and `homoglyph_impersonation` takes `&[String]`, so no further changes are needed in `distance.rs` or `homoglyph.rs`.

## Acceptance Criteria Mapping

### Pointer-equality test

The issue requires a test that calls the loader twice and asserts both calls return the same pointer (`std::ptr::eq`):

```rust
#[test]
fn loader_returns_same_allocation_on_repeated_calls() {
    let first = load_popular_packages();
    let second = load_popular_packages();
    // Both slices must be backed by the same allocation — OnceLock
    // guarantees the Vec is initialised only once.
    assert!(
        std::ptr::eq(first.as_ptr(), second.as_ptr()),
        "expected both calls to return the same pointer (cache miss on second call)"
    );
}
```

This test passes if and only if `OnceLock` is correctly initialised — a newly allocated `Vec` on each call would produce different pointers and fail.

### Existing loader test

The existing `loader_returns_non_empty_list_including_known_packages` test checks `react` and `requests` are present. The only update needed is changing `packages.iter()` to work on `&[String]` (it already does — no change required).

### CI

```
cargo fmt
cargo clippy -- -D warnings
cargo test --workspace
```

## Implementation Checklist

- [ ] Add `static POPULAR_PACKAGES: OnceLock<Vec<String>>` to `dataset.rs`
- [ ] Change `load_popular_packages` return type to `&'static [String]`
- [ ] Update `score_package` in `scanner.rs` to use `&'static [String]`
- [ ] Add `loader_returns_same_allocation_on_repeated_calls` test in `dataset.rs`
- [ ] Add PROGRESS.md entry — closes #22

## Notes

- `OnceLock` is in `std::sync` since Rust 1.70. No new dependency is required.
- The pointer-equality test is only meaningful when run single-threaded (the default for `cargo test`). `OnceLock` is thread-safe by design, but `ptr::eq` on two concurrent first callers would be a data race test, not a correctness test; the single-threaded sequential check is sufficient.
- Do not replace `OnceLock` with `lazy_static` or `once_cell` — those are third-party crates and the stdlib primitive does the same job.
- `load_popular_packages()` should remain `pub` so existing call sites in tests and the homoglyph module compile without change.
