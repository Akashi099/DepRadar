//! `depradar` — scan a project lockfile for typosquats and known
//! vulnerabilities.
//!
//! Run `depradar --help` for usage.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use deprader::ignore::{IgnoreFile, IGNORE_FILE_NAME};
use deprader::pipeline::{scan_lockfile, ScanOptions};
use deprader::report::sarif::to_sarif;
use deprader::report::AdvisorySeverities;

const HELP: &str = "\
depradar — scan a lockfile for typosquats and known vulnerabilities

Usage: depradar [OPTIONS] <LOCKFILE>

Arguments:
  <LOCKFILE>  package-lock.json, npm-shrinkwrap.json or requirements*.txt

Options:
  -f, --format <FORMAT>  Output format: text or sarif [default: text]
  -o, --output <FILE>    Write the report to FILE instead of stdout
      --offline          Skip the OSV vulnerability lookup (typosquat checks only)
  -h, --help             Print this help
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Text,
    Sarif,
}

#[derive(Debug)]
struct Args {
    lockfile: PathBuf,
    format: Format,
    output: Option<PathBuf>,
    offline: bool,
}

/// What the command line asked for.
enum Command {
    Help,
    Scan(Args),
}

fn parse_args(raw: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut lockfile = None;
    let mut format = Format::Text;
    let mut output = None;
    let mut offline = false;

    let mut raw = raw.into_iter();
    while let Some(arg) = raw.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => (flag.to_string(), Some(value)),
            _ => (arg.clone(), None),
        };
        let mut value = |name: &str| -> Result<String, String> {
            match inline {
                Some(value) => Ok(value.to_string()),
                None => raw.next().ok_or_else(|| format!("{name} needs a value")),
            }
        };
        match flag.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-f" | "--format" => {
                format = match value("--format")?.as_str() {
                    "text" => Format::Text,
                    "sarif" => Format::Sarif,
                    other => {
                        return Err(format!(
                            "unknown format `{other}` (expected text or sarif)"
                        ))
                    }
                }
            }
            "-o" | "--output" => output = Some(PathBuf::from(value("--output")?)),
            "--offline" => offline = true,
            _ if flag.starts_with('-') && flag != "-" => {
                return Err(format!("unknown option `{flag}`"))
            }
            _ if lockfile.is_some() => return Err(format!("unexpected argument `{arg}`")),
            _ => lockfile = Some(PathBuf::from(&arg)),
        }
    }

    let lockfile = lockfile.ok_or("missing <LOCKFILE>")?;
    Ok(Command::Scan(Args {
        lockfile,
        format,
        output,
        offline,
    }))
}

fn main() -> ExitCode {
    let args = match parse_args(env::args().skip(1)) {
        Ok(Command::Help) => {
            print!("{HELP}");
            return ExitCode::SUCCESS;
        }
        Ok(Command::Scan(args)) => args,
        Err(message) => {
            eprintln!("error: {message}\n\nRun `depradar --help` for usage.");
            return ExitCode::from(2);
        }
    };

    match run(&args) {
        Ok(true) => ExitCode::FAILURE,
        Ok(false) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
    }
}

/// Scans and writes the report. Returns whether there were findings.
fn run(args: &Args) -> Result<bool, String> {
    let ignore = load_ignore(&args.lockfile)?;
    let opts = ScanOptions {
        ignore: ignore.as_ref(),
        advisories: if args.offline {
            None
        } else {
            ScanOptions::default().advisories
        },
        ..ScanOptions::default()
    };
    let mut report = scan_lockfile(&args.lockfile, &opts).map_err(|err| err.to_string())?;

    let advisories = if args.offline {
        AdvisorySeverities::default()
    } else {
        let (advisories, warnings) = AdvisorySeverities::fetch(&report.findings);
        report.summary.warnings += warnings.len();
        report.warnings.extend(warnings);
        advisories
    };

    let rendered = match args.format {
        Format::Text => format!("{report}\n"),
        Format::Sarif => {
            let contents = fs::read_to_string(&args.lockfile).ok();
            let sarif = to_sarif(&report, &advisories, contents.as_deref());
            let mut text = serde_json::to_string_pretty(&sarif).map_err(|err| err.to_string())?;
            text.push('\n');
            text
        }
    };

    match &args.output {
        Some(path) => fs::write(path, rendered)
            .map_err(|err| format!("couldn't write {}: {err}", path.display()))?,
        None => print!("{rendered}"),
    }
    Ok(report.has_findings())
}

/// Loads the `.depradarignore` next to the lockfile, if there is one.
fn load_ignore(lockfile: &Path) -> Result<Option<IgnoreFile>, String> {
    let path = lockfile
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(IGNORE_FILE_NAME);
    if !path.is_file() {
        return Ok(None);
    }
    IgnoreFile::load(&path).map(Some).map_err(|err| err.to_string())
}
