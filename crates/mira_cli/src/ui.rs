//! Terminal output. Monochrome only: emphasis comes from bold and dim, and
//! graphics are drawn with braille stipples. Decoration is dropped when
//! stdout is not a terminal or `NO_COLOR` is set.

use std::io::IsTerminal;
use std::path::Path;
use std::sync::OnceLock;

use crate::diagnostic::Diagnostic;

/// The Mira mark (mira-mark.svg, a 5 by 5 grid spelling M) drawn as one
/// braille dot per square.
const MARK: [&str; 3] = ["⠅⠄⠀⠄⠅", "⠅⠀⠁⠀⠅", "⠁⠀⠀⠀⠁"];
/// Braille cells from empty to full, used to draw stippled bars.
const RAMP: [char; 9] = ['⠀', '⠁', '⠃', '⠇', '⡇', '⡏', '⡟', '⡿', '⣿'];
pub const DOT: &str = "⠿";

fn styled() -> bool {
    static STYLED: OnceLock<bool> = OnceLock::new();
    *STYLED.get_or_init(|| std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none())
}

fn wrap(code: &str, s: &str) -> String {
    if styled() { format!("\x1b[{code}m{s}\x1b[0m") } else { s.to_string() }
}

pub fn bold(s: &str) -> String {
    wrap("1", s)
}

pub fn dim(s: &str) -> String {
    wrap("2", s)
}

/// The stippled mark beside a command name and an optional detail.
pub fn header(command: &str, detail: &str) {
    let version = concat!("v", env!("CARGO_PKG_VERSION"));
    eprintln!();
    eprintln!("  {}   {} {}", MARK[0], bold(&format!("mira {command}")), dim(detail));
    eprintln!("  {}   {}", MARK[1], dim(version));
    eprintln!("  {}", MARK[2]);
    eprintln!();
}

pub fn section(title: &str) {
    eprintln!("  {}", bold(title));
}

/// A stippled bar `width` cells wide, filled to `fraction`.
pub fn bar(fraction: f64, width: usize) -> String {
    let steps = (fraction.clamp(0.0, 1.0) * (width * 8) as f64).round() as usize;
    (0..width).map(|i| RAMP[steps.saturating_sub(i * 8).min(8)]).collect()
}

/// `label ········ value`, with the leader filling to `width`.
pub fn leader(label: &str, width: usize) -> String {
    let len = label.chars().count();
    let dots = if width > len + 1 { width - len - 1 } else { 1 };
    format!("{label} {}", dim(&"·".repeat(dots)))
}

pub fn kb(bytes: usize) -> String {
    format!("{:.1} KB", bytes as f64 / 1024.0)
}

pub fn ms(ms: f64) -> String {
    if ms < 10.0 { format!("{ms:.1} ms") } else { format!("{ms:.0} ms") }
}

pub fn success(message: &str) {
    eprintln!("  {DOT} {}", bold(message));
}

/// A warning line: stippled marker, dim label, message.
pub fn warning(message: &str) {
    eprintln!("  {} {}  {}", bold("!"), dim("warn"), message);
}

/// Prints an error as a framed block: message, location with a source
/// excerpt when the line is known, then underlying causes and the hint.
pub fn error(err: &anyhow::Error, root: &Path) {
    let d = Diagnostic::from_error(err, root);
    eprintln!("  {} {}", bold("✕"), bold(&d.message));
    if let Some(at) = d.location() {
        eprintln!();
        eprintln!("    {}", dim(&at));
        let width = d.excerpt.last().map_or(1, |l| l.number.to_string().len());
        for line in &d.excerpt {
            let number = format!("{:>width$}", line.number);
            if line.current {
                eprintln!("    {} {} {}", bold("›"), bold(&number), line.text);
            } else {
                eprintln!("      {} {}", dim(&number), dim(&line.text));
            }
        }
    }
    if !d.causes.is_empty() {
        eprintln!();
        for cause in &d.causes {
            eprintln!("    {} {}", dim("cause"), cause);
        }
    }
    if let Some(hint) = &d.hint {
        eprintln!();
        eprintln!("    {}  {}", dim("hint"), hint);
    }
    eprintln!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_fills_by_eighths() {
        assert_eq!(bar(0.0, 3), "⠀⠀⠀");
        assert_eq!(bar(0.5, 2), "⣿⠀");
        assert_eq!(bar(1.0, 2), "⣿⣿");
        assert_eq!(bar(0.25, 2).chars().next(), Some('⡇'));
    }
}
