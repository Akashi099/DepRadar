//! `depradar` — scan a project lockfile for typosquats and known
//! vulnerabilities.
//!
//! Run `depradar --help` for usage.

use std::env;
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use deprader::ignore::{IgnoreFile, IGNORE_FILE_NAME};
use deprader::pipeline::{scan_lockfile, ScanOptions, StageTiming};
use deprader::report::sarif::to_sarif;
use deprader::report::text::{render, use_color, ColorChoice, TextOptions, Verbosity};
use deprader::report::{severity_of, AdvisorySeverities, Severity};

const HELP: &str = "\
depradar — scan a lockfile for typosquats and known vulnerabilities

Usage: depradar [OPTIONS] <LOCKFILE>

Arguments:
  <LOCKFILE>  package-lock.json, npm-shrinkwrap.json or requirements*.txt

Options:
  -f, --format <FORMAT>  Output format: text or sarif [default: text]
  -o, --output <FILE>    Write the report to FILE instead of stdout
      --color <WHEN>     Color severity labels: auto, always or never [default: auto]
  -q, --quiet            Print only the findings and the summary line
  -v, --verbose          Also list every dependency checked and the time per stage
      --fail-on <LEVEL>  Exit 1 if any finding is at or above LEVEL: low, medium,
                         high, critical or none (never fail) [default: high]
      --offline          Skip the OSV vulnerability lookup (typosquat checks only)
  -h, --help             Print this help

Color is off when stdout isn't a terminal (with --color auto) and whenever
the NO_COLOR environment variable is set to a non-empty value. --quiet and
--verbose only affect text output.

Exit codes:
  0  No findings at or above the --fail-on level (suppressed findings never count)
  1  At least one finding at or above the --fail-on level
  2  Usage error, or the scan couldn't complete (unreadable or malformed
     lockfile or .depradarignore, failed OSV lookup)
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Text,
    Sarif,
}

/// The `--fail-on` threshold: findings at or above it fail the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailOn {
    AtLeast(Severity),
    Never,
}

impl FailOn {
    fn parse(value: &str) -> Option<FailOn> {
        let level = match value.to_ascii_lowercase().as_str() {
            "low" => Severity::Low,
            "medium" => Severity::Medium,
            "high" => Severity::High,
            "critical" => Severity::Critical,
            "none" => return Some(FailOn::Never),
            _ => return None,
        };
        Some(FailOn::AtLeast(level))
    }

    fn fails(self, severity: Severity) -> bool {
        match self {
            FailOn::AtLeast(threshold) => severity >= threshold,
            FailOn::Never => false,
        }
    }
}

#[derive(Debug)]
struct Args {
    lockfile: PathBuf,
    format: Format,
    output: Option<PathBuf>,
    color: ColorChoice,
    verbosity: Verbosity,
    fail_on: FailOn,
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
    let mut color = ColorChoice::Auto;
    let mut quiet = false;
    let mut verbose = false;
    let mut fail_on = FailOn::AtLeast(Severity::High);
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
            "--color" => {
                color = match value("--color")?.as_str() {
                    "auto" => ColorChoice::Auto,
                    "always" => ColorChoice::Always,
                    "never" => ColorChoice::Never,
                    other => {
                        return Err(format!(
                            "unknown --color value `{other}` (expected auto, always or never)"
                        ))
                    }
                }
            }
            "-q" | "--quiet" => quiet = true,
            "-v" | "--verbose" => verbose = true,
            "--fail-on" => {
                let level = value("--fail-on")?;
                fail_on = FailOn::parse(&level).ok_or_else(|| {
                    format!(
                        "unknown --fail-on level `{level}` \
                         (expected low, medium, high, critical or none)"
                    )
                })?;
            }
            "--offline" => offline = true,
            _ if flag.starts_with('-') && flag != "-" => {
                return Err(format!("unknown option `{flag}`"))
            }
            _ if lockfile.is_some() => return Err(format!("unexpected argument `{arg}`")),
            _ => lockfile = Some(PathBuf::from(&arg)),
        }
    }

    let lockfile = lockfile.ok_or("missing <LOCKFILE>")?;
    let verbosity = match (quiet, verbose) {
        (true, true) => return Err("--quiet and --verbose can't be used together".to_string()),
        (true, false) => Verbosity::Quiet,
        (false, true) => Verbosity::Verbose,
        (false, false) => Verbosity::Normal,
    };
    Ok(Command::Scan(Args {
        lockfile,
        format,
        output,
        color,
        verbosity,
        fail_on,
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
        Ok(Outcome::Clean) => ExitCode::SUCCESS,
        Ok(Outcome::Failed) => ExitCode::FAILURE,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
    }
}

/// Whether the scan passed the `--fail-on` gate.
enum Outcome {
    /// No finding at or above the threshold (exit 0).
    Clean,
    /// At least one finding at or above the threshold (exit 1).
    Failed,
}

/// Scans, writes the report, and judges the findings against `--fail-on`.
fn run(args: &Args) -> Result<Outcome, String> {
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
        let started = Instant::now();
        let (advisories, warnings) = AdvisorySeverities::fetch(&report.findings);
        report.summary.warnings += warnings.len();
        report.warnings.extend(warnings);
        report.timings.push(StageTiming {
            stage: "severity",
            elapsed: started.elapsed(),
        });
        advisories
    };

    let rendered = match args.format {
        Format::Text => {
            // Files never get color unless asked for; `use_color` treats them
            // like a pipe.
            let is_terminal = args.output.is_none() && std::io::stdout().is_terminal();
            let no_color = env::var("NO_COLOR").ok();
            let opts = TextOptions {
                color: use_color(args.color, no_color.as_deref(), is_terminal),
                verbosity: args.verbosity,
            };
            render(&report, &advisories, &opts)
        }
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

    let failed = report
        .findings
        .iter()
        .any(|finding| args.fail_on.fails(severity_of(finding, &advisories)));
    Ok(if failed {
        Outcome::Failed
    } else {
        Outcome::Clean
    })
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
