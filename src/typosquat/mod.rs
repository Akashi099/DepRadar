//! Typosquat detection module.
//!
//! Bundles the reference data a typosquat check needs — the lists of popular
//! package names an attacker is most likely to imitate ([`dataset`]) — two
//! independent signals for comparing a candidate name against them (the
//! edit-distance primitives in [`distance`] and the lookalike-character
//! checks in [`homoglyph`]) — a [`bktree`] index so distance lookups aren't a
//! linear scan, and [`scanner::score_package`], which combines the signals
//! into a single scored result.

pub mod bktree;
pub mod dataset;
pub mod distance;
pub mod homoglyph;
pub mod keyboard;
pub mod normalize;
pub mod scanner;
pub mod scope;
