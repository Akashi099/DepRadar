//! Shared data models used across scanners.
//!
//! Lockfile parsers ([`crate::lockfile`]) all produce [`Dependency`] values so
//! the typosquat and CVE signals can run over any project the same way,
//! regardless of which registry or lockfile format it came from.

pub use crate::cve::osv_client::Ecosystem;

/// One package a project depends on, as read from a lockfile or manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    /// Package name as the registry knows it (e.g. `@angular/core`,
    /// `requests`, `serde`).
    pub name: String,
    /// Resolved version, when the source pins one. Manifests such as
    /// `package.json` only carry version ranges, so this is `None` there.
    pub version: Option<String>,
    /// Registry the package is published to.
    pub ecosystem: Ecosystem,
    /// `true` when the project declares the package itself, rather than it
    /// being pulled in by another dependency. Formats that don't record the
    /// distinction leave this `false`.
    pub direct: bool,
    /// `true` for development-only dependencies (`devDependencies`, Pipenv's
    /// `develop` section, ...).
    pub dev: bool,
}
