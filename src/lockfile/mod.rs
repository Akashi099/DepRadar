//! Lockfile and manifest parsers.
//!
//! Each submodule turns one registry's file formats into a flat list of
//! [`Dependency`] values:
//!
//! - [`cargo`] — `Cargo.lock`
//! - [`npm`] — `package.json`
//! - [`pnpm`] — `pnpm-lock.yaml` (v6 and v9)
//! - [`pypi`] — `Pipfile.lock`
//! - [`yarn`] — `yarn.lock` (v1)
//!
//! [`detect`] works out which format a file is from its name, and
//! [`parse_file`] reads a file and dispatches it to the matching parser.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::model::Dependency;

pub mod cargo;
pub mod npm;
pub mod pnpm;
pub mod pypi;
pub mod yarn;

/// A lockfile or manifest format DepRadar recognises by file name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockfileKind {
    /// npm `package-lock.json`.
    PackageLock,
    /// Yarn `yarn.lock`.
    YarnLock,
    /// pnpm `pnpm-lock.yaml`.
    PnpmLock,
    /// pip `requirements.txt`, or any `requirements*.txt` variant.
    Requirements,
    /// Poetry `poetry.lock`.
    PoetryLock,
    /// Pipenv `Pipfile.lock`.
    PipfileLock,
    /// Cargo `Cargo.lock`.
    CargoLock,
    /// npm `package.json` manifest (direct dependencies only).
    PackageJson,
}

/// Works out a file's [`LockfileKind`] from its file name, ignoring the
/// directory it's in. Returns `None` for names DepRadar doesn't recognise.
pub fn detect(path: impl AsRef<Path>) -> Option<LockfileKind> {
    let file_name = path.as_ref().file_name()?.to_str()?;
    let kind = match file_name {
        "package-lock.json" => LockfileKind::PackageLock,
        "yarn.lock" => LockfileKind::YarnLock,
        "pnpm-lock.yaml" => LockfileKind::PnpmLock,
        "poetry.lock" => LockfileKind::PoetryLock,
        "Pipfile.lock" => LockfileKind::PipfileLock,
        "Cargo.lock" => LockfileKind::CargoLock,
        "package.json" => LockfileKind::PackageJson,
        name if name.starts_with("requirements") && name.ends_with(".txt") => {
            LockfileKind::Requirements
        }
        _ => return None,
    };
    Some(kind)
}

/// The result of parsing one lockfile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLockfile {
    pub ecosystem: crate::model::Ecosystem,
    pub dependencies: Vec<Dependency>,
    /// Entries that were skipped or only partly understood.
    pub warnings: Vec<String>,
}

/// Errors that stop a lockfile from being parsed at all.
#[derive(Debug)]
pub enum LockfileError {
    /// The file couldn't be read.
    Io(std::io::Error),
    /// The file name doesn't match any supported lockfile format.
    UnknownFormat(String),
    /// The file is a supported format but its contents are malformed.
    Invalid(String),
}

impl fmt::Display for LockfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LockfileError::Io(err) => write!(f, "failed to read lockfile: {err}"),
            LockfileError::UnknownFormat(name) => {
                write!(f, "unsupported lockfile `{name}`")
            }
            LockfileError::Invalid(reason) => write!(f, "malformed lockfile: {reason}"),
        }
    }
}

impl std::error::Error for LockfileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LockfileError::Io(err) => Some(err),
            LockfileError::UnknownFormat(_) | LockfileError::Invalid(_) => None,
        }
    }
}

/// Parses the lockfile at `path`, picking the parser from the file name.
pub fn parse_lockfile(path: &Path) -> Result<ParsedLockfile, LockfileError> {
    use crate::model::Ecosystem;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    if file_name == "package-lock.json" || file_name == "npm-shrinkwrap.json" {
        let contents = fs::read_to_string(path).map_err(LockfileError::Io)?;
        return npm::parse_lock(&contents);
    }

    let kind = detect(file_name).ok_or_else(|| LockfileError::UnknownFormat(file_name.into()))?;
    let contents = fs::read_to_string(path).map_err(LockfileError::Io)?;

    if kind == LockfileKind::Requirements {
        let (dependencies, warnings) = pypi::parse_requirements_with_warnings(&contents)
            .map_err(|err| LockfileError::Invalid(err.to_string()))?;
        return Ok(ParsedLockfile {
            ecosystem: Ecosystem::PyPI,
            dependencies,
            warnings,
        });
    }

    let parse: fn(&str) -> Result<Vec<Dependency>, ParseError> = match kind {
        LockfileKind::PipfileLock => pypi::parse_pipfile_lock,
        LockfileKind::CargoLock => cargo::parse_cargo_lock,
        LockfileKind::PackageJson => npm::parse_package_json,
        LockfileKind::YarnLock => yarn::parse_yarn_lock,
        LockfileKind::PnpmLock => pnpm::parse_pnpm_lock,
        LockfileKind::PoetryLock => pypi::parse_poetry_lock,
        LockfileKind::Requirements | LockfileKind::PackageLock => {
            unreachable!("handled above")
        }
    };
    let dependencies = parse(&contents).map_err(|err| LockfileError::Invalid(err.to_string()))?;
    let ecosystem = match kind {
        LockfileKind::PipfileLock | LockfileKind::PoetryLock => Ecosystem::PyPI,
        LockfileKind::CargoLock => Ecosystem::CratesIo,
        _ => Ecosystem::Npm,
    };
    Ok(ParsedLockfile {
        ecosystem,
        dependencies,
        warnings: Vec::new(),
    })
}

/// Reads the file at `path` and parses it with the parser matching its
/// [`detect`]ed kind.
///
/// Fails with [`ParseError::UnknownFile`] when the file name isn't
/// recognised, and with [`ParseError::Unsupported`] when it is but that
/// format's parser hasn't landed yet — in both cases without reading the
/// file.
pub fn parse_file(path: impl AsRef<Path>) -> Result<Vec<Dependency>, ParseError> {
    let path = path.as_ref();
    let kind = detect(path).ok_or_else(|| ParseError::UnknownFile(path.to_path_buf()))?;
    let parse: fn(&str) -> Result<Vec<Dependency>, ParseError> = match kind {
        LockfileKind::PipfileLock => pypi::parse_pipfile_lock,
        LockfileKind::CargoLock => cargo::parse_cargo_lock,
        LockfileKind::PackageJson => npm::parse_package_json,
        LockfileKind::YarnLock => yarn::parse_yarn_lock,
        LockfileKind::PnpmLock => pnpm::parse_pnpm_lock,
        LockfileKind::Requirements => pypi::parse_requirements,
        LockfileKind::PoetryLock => pypi::parse_poetry_lock,
        LockfileKind::PackageLock => return Err(ParseError::Unsupported(kind)),
    };
    parse(&fs::read_to_string(path)?)
}

/// Why a lockfile or manifest couldn't be parsed.
#[derive(Debug)]
pub enum ParseError {
    /// The file isn't valid JSON, or doesn't have the expected shape.
    Json(serde_json::Error),
    /// The file isn't valid TOML, or doesn't have the expected shape.
    Toml(toml::de::Error),
    /// The file isn't valid YAML, or doesn't have the expected shape.
    Yaml(serde_yaml::Error),
    /// The file couldn't be read.
    Io(io::Error),
    /// The file name isn't one [`detect`] recognises.
    UnknownFile(PathBuf),
    /// The format is recognised but has no parser yet.
    Unsupported(LockfileKind),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Json(err) => write!(f, "invalid JSON: {err}"),
            ParseError::Toml(err) => write!(f, "invalid TOML: {err}"),
            ParseError::Yaml(err) => write!(f, "invalid YAML: {err}"),
            ParseError::Io(err) => write!(f, "couldn't read file: {err}"),
            ParseError::UnknownFile(path) => {
                write!(f, "unrecognised lockfile name: {}", path.display())
            }
            ParseError::Unsupported(kind) => write!(f, "no parser for {kind:?} yet"),
        }
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ParseError::Json(err) => Some(err),
            ParseError::Toml(err) => Some(err),
            ParseError::Yaml(err) => Some(err),
            ParseError::Io(err) => Some(err),
            ParseError::UnknownFile(_) | ParseError::Unsupported(_) => None,
        }
    }
}

impl From<serde_json::Error> for ParseError {
    fn from(err: serde_json::Error) -> Self {
        ParseError::Json(err)
    }
}

impl From<toml::de::Error> for ParseError {
    fn from(err: toml::de::Error) -> Self {
        ParseError::Toml(err)
    }
}

impl From<serde_yaml::Error> for ParseError {
    fn from(err: serde_yaml::Error) -> Self {
        ParseError::Yaml(err)
    }
}

impl From<io::Error> for ParseError {
    fn from(err: io::Error) -> Self {
        ParseError::Io(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_path(relative: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
    }

    #[test]
    fn detects_every_supported_file_name() {
        let cases = [
            ("package-lock.json", Some(LockfileKind::PackageLock)),
            ("yarn.lock", Some(LockfileKind::YarnLock)),
            ("pnpm-lock.yaml", Some(LockfileKind::PnpmLock)),
            ("requirements.txt", Some(LockfileKind::Requirements)),
            ("requirements-dev.txt", Some(LockfileKind::Requirements)),
            ("requirements_test.txt", Some(LockfileKind::Requirements)),
            ("poetry.lock", Some(LockfileKind::PoetryLock)),
            ("Pipfile.lock", Some(LockfileKind::PipfileLock)),
            ("Cargo.lock", Some(LockfileKind::CargoLock)),
            ("package.json", Some(LockfileKind::PackageJson)),
            ("some/project/Cargo.lock", Some(LockfileKind::CargoLock)),
            ("Gemfile.lock", None),
            ("requirements.in", None),
            ("Pipfile", None),
            ("README.md", None),
            ("", None),
        ];
        for (path, expected) in cases {
            assert_eq!(detect(path), expected, "detect({path:?})");
        }
    }

    #[test]
    fn parse_file_dispatches_to_the_right_parser() {
        let pipfile = parse_file(repo_path("tests/fixtures/Pipfile.lock")).unwrap();
        assert!(pipfile.iter().any(|d| d.name == "requests"));

        let cargo = parse_file(repo_path("Cargo.lock")).unwrap();
        assert!(cargo.iter().any(|d| d.name == "soroban-sdk"));

        let package_json = parse_file(repo_path("tests/fixtures/package.json")).unwrap();
        assert!(package_json.iter().any(|d| d.name == "express"));
    }

    #[test]
    fn parse_file_rejects_unknown_and_unsupported_files() {
        assert!(matches!(
            parse_file("Gemfile.lock"),
            Err(ParseError::UnknownFile(_))
        ));
        assert!(matches!(
            parse_file("does/not/exist/package-lock.json"),
            Err(ParseError::Unsupported(LockfileKind::PackageLock))
        ));
        assert!(matches!(
            parse_file("does/not/exist/Cargo.lock"),
            Err(ParseError::Io(_))
        ));
    }
}
