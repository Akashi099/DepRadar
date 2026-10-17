//! Human-readable terminal output.
//!
//! Like [`Report`]'s `Display`, but every finding is prefixed with its
//! severity, and the label can be colored: critical and high in red,
//! medium in yellow, low dimmed. [`use_color`] decides whether to color,
//! honouring [`NO_COLOR`](https://no-color.org) and whether stdout is a
//! terminal.

use std::fmt::Write;

use owo_colors::OwoColorize;

use super::{label, severity_of, AdvisorySeverities, Severity};
use crate::pipeline::Report;

/// The `--color` setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorChoice {
    /// Color only when writing to a terminal.
    #[default]
    Auto,
    Always,
    Never,
}

/// Whether to emit ANSI colors. A non-empty `NO_COLOR` turns color off
/// whatever `choice` says; otherwise `Auto` colors only a terminal.
pub fn use_color(choice: ColorChoice, no_color: Option<&str>, is_terminal: bool) -> bool {
    if no_color.is_some_and(|value| !value.is_empty()) {
        return false;
    }
    match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => is_terminal,
    }
}

/// How to render a report as text.
#[derive(Debug, Clone, Copy, Default)]
pub struct TextOptions {
    pub color: bool,
}

/// `[HIGH]`-style severity tag, colored if asked.
fn tag(severity: Severity, color: bool) -> String {
    let text = format!("[{}]", label(severity).to_ascii_uppercase());
    if !color {
        return text;
    }
    match severity {
        Severity::Critical => text.red().bold().to_string(),
        Severity::High => text.red().to_string(),
        Severity::Medium => text.yellow().to_string(),
        Severity::Low => text.dimmed().to_string(),
    }
}

/// Renders `report` for the terminal.
pub fn render(report: &Report, advisories: &AdvisorySeverities, opts: &TextOptions) -> String {
    let mut out = String::new();
    // Writing to a String can't fail.
    let _ = write_report(&mut out, report, advisories, opts);
    out
}

fn write_report(
    out: &mut String,
    report: &Report,
    advisories: &AdvisorySeverities,
    opts: &TextOptions,
) -> std::fmt::Result {
    writeln!(
        out,
        "Scanned {} ({} packages, {} pinned versions checked for known vulnerabilities)",
        report.path.display(),
        report.summary.packages,
        report.summary.queried
    )?;

    if report.findings.is_empty() {
        writeln!(out, "\nNo findings.")?;
    } else {
        writeln!(out, "\nFindings ({}):", report.findings.len())?;
        for finding in &report.findings {
            let severity = severity_of(finding, advisories);
            writeln!(out, "  - {} {finding}", tag(severity, opts.color))?;
        }
    }

    if !report.suppressed.is_empty() {
        writeln!(out, "\nSuppressed ({}):", report.suppressed.len())?;
        for suppressed in &report.suppressed {
            write!(out, "  - {} — {}", suppressed.finding, suppressed.reason)?;
            match suppressed.expires {
                Some(expires) => writeln!(out, " (until {expires})")?,
                None => writeln!(out)?,
            }
        }
    }

    if !report.warnings.is_empty() {
        writeln!(out, "\nWarnings ({}):", report.warnings.len())?;
        for warning in &report.warnings {
            writeln!(out, "  - {warning}")?;
        }
    }

    writeln!(out, "\n{}", summary_line(report))
}

/// The closing `Summary: ...` line.
pub fn summary_line(report: &Report) -> String {
    format!(
        "Summary: {} typosquat(s), {} vulnerability finding(s), {} suppressed, {} warning(s)",
        report.summary.typosquats,
        report.summary.vulnerabilities,
        report.summary.suppressed,
        report.summary.warnings
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_color_wins_over_everything() {
        for choice in [ColorChoice::Auto, ColorChoice::Always, ColorChoice::Never] {
            assert!(!use_color(choice, Some("1"), true), "{choice:?}");
        }
    }

    #[test]
    fn empty_no_color_is_ignored() {
        assert!(use_color(ColorChoice::Auto, Some(""), true));
    }

    #[test]
    fn auto_colors_only_a_terminal() {
        assert!(use_color(ColorChoice::Auto, None, true));
        assert!(!use_color(ColorChoice::Auto, None, false));
        assert!(use_color(ColorChoice::Always, None, false));
        assert!(!use_color(ColorChoice::Never, None, true));
    }

    #[test]
    fn tags_are_plain_without_color() {
        assert_eq!(tag(Severity::Critical, false), "[CRITICAL]");
        assert_eq!(tag(Severity::Low, false), "[LOW]");
        assert!(tag(Severity::High, true).contains('\u{1b}'));
        assert!(tag(Severity::Medium, true).contains("[MEDIUM]"));
    }
}
