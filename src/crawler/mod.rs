//! Crawler infrastructure — registry crawlers and supporting utilities.
//!
//! The crawler side of DepRadar watches package registries for new packages
//! and new versions of existing packages, then hands them off to the scanner
//! pipeline.

pub mod checkpoint;
