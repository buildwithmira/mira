//! One structured shape for every error, shared by the terminal, the
//! `--json` output, and the dev overlay in the browser.

use std::path::Path;

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Diagnostic {
    pub message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub hint: Option<String>,
    pub causes: Vec<String>,
    /// Source lines around `line`.
    pub excerpt: Vec<ExcerptLine>,
}

#[derive(Debug, Serialize)]
pub struct ExcerptLine {
    pub number: usize,
    pub text: String,
    pub current: bool,
}

impl Diagnostic {
    /// Compiler errors use the shape `path[:line[:col]]: message`,
    /// optionally followed by a `hint: ...` line.
    pub fn from_error(err: &anyhow::Error, root: &Path) -> Diagnostic {
        let text = err.to_string();
        let (first, rest) = text.split_once('\n').unwrap_or((&text, ""));
        let (location, message) = split_location(first);
        let excerpt = match location {
            Some((path, Some(line))) => excerpt(&root.join(path), line),
            _ => Vec::new(),
        };
        Diagnostic {
            message: message.to_string(),
            file: location.map(|(path, _)| path.to_string()),
            line: location.and_then(|(_, line)| line),
            hint: rest.lines().find_map(|l| l.strip_prefix("hint: ")).map(str::to_string),
            causes: err.chain().skip(1).map(|c| c.to_string()).collect(),
            excerpt,
        }
    }

    pub fn location(&self) -> Option<String> {
        let file = self.file.as_ref()?;
        Some(match self.line {
            Some(n) => format!("{file}:{n}"),
            None => file.clone(),
        })
    }
}

pub fn split_location(line: &str) -> (Option<(&str, Option<usize>)>, &str) {
    let Some((loc, message)) = line.split_once(": ") else { return (None, line) };
    let looks_like_path = loc.contains(['/', '\\'])
        || [".md", ".mira", ".json"].iter().any(|ext| loc.trim_end_matches(|c: char| c.is_ascii_digit() || c == ':').ends_with(ext));
    if !looks_like_path {
        return (None, line);
    }
    // Peel `:line` and `:col` off the right so drive letters survive.
    let (mut path, mut numbers) = (loc, Vec::new());
    while let Some((p, n)) = path.rsplit_once(':') {
        let Ok(n) = n.parse::<usize>() else { break };
        numbers.push(n);
        path = p;
    }
    (Some((path, numbers.last().copied())), message)
}

fn excerpt(path: &Path, line: usize) -> Vec<ExcerptLine> {
    let Ok(src) = std::fs::read_to_string(path) else { return Vec::new() };
    let lines: Vec<&str> = src.lines().collect();
    let start = line.saturating_sub(2).max(1);
    let end = (line + 1).min(lines.len());
    (start..=end)
        .filter_map(|n| lines.get(n - 1).map(|text| ExcerptLine { number: n, text: text.to_string(), current: n == line }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_locations() {
        assert_eq!(split_location("routes/a.md:3: bad"), (Some(("routes/a.md", Some(3))), "bad"));
        assert_eq!(split_location("routes/a.md: bad"), (Some(("routes/a.md", None)), "bad"));
        assert_eq!(split_location("mira.config.json:4:9: bad"), (Some(("mira.config.json", Some(4))), "bad"));
        assert_eq!(split_location(r"C:\My Site\dist: bad"), (Some((r"C:\My Site\dist", None)), "bad"));
        assert_eq!(split_location("something went wrong: io"), (None, "something went wrong: io"));
    }
}
