//! Crawlers that watch package registries for newly published packages.
//!
//! Everything network-facing goes through [`crate::net::PoliteClient`], so
//! the crawlers themselves only deal with parsing and checkpointing.
//!
//! * [`pypi`] — PyPI's RSS feeds of new projects and updates (#91).
//! * [`crates`] — crates.io's "newest crates" API (#92).

pub mod crates;
pub mod pypi;
