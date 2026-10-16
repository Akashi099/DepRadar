# Popular-package datasets

Reference lists of widely-used package names, one JSON file per registry.
The typosquat scanner (`src/typosquat/`) compares a candidate name against
these to decide whether it is imitating something popular. Each file is a
flat JSON array of lowercase name strings, embedded into the binary at
compile time by `src/typosquat/dataset.rs`.

| File | Registry | Source | Snapshot date |
| --- | --- | --- | --- |
| `npm-top1000.json` | npm | Hand-curated representative snapshot | — |
| `pypi-top1000.json` | PyPI | Hand-curated representative snapshot | — |
| `crates-top1000.json` | crates.io | Top 1000 crates by recent (90-day) downloads from the [crates.io API](https://crates.io/api/v1/crates?sort=recent-downloads&per_page=100) (`sort=recent-downloads`, pages 1–10), lowercased and sorted | 2026-10-16 |
