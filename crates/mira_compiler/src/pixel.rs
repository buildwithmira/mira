//! The Mira pixel module, expanded at compile time.
//!
//! `<mira-mark />` draws the 5 by 5 pixel M. `<mira-pixels text="404" />`
//! sets text in a 5 by 7 pixel face. Both render squares with `radius-px`
//! corners and a gap of one eighth of a pixel, filled with `currentColor`.
//! Add `dither` to also draw the unlit pixels, faintly.

use crate::html::escape;

const MARK: [&str; 5] = ["#...#", "##.##", "#.#.#", "#...#", "#...#"];

/// 5 by 7 glyphs. Unknown characters render as blank cells.
fn glyph(c: char) -> Option<[&'static str; 7]> {
    Some(match c.to_ascii_uppercase() {
        '0' => [".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###."],
        '1' => ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###."],
        '2' => [".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####"],
        '3' => ["####.", "....#", "....#", ".###.", "....#", "....#", "####."],
        '4' => ["...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#."],
        '5' => ["#####", "#....", "####.", "....#", "....#", "#...#", ".###."],
        '6' => [".###.", "#....", "#....", "####.", "#...#", "#...#", ".###."],
        '7' => ["#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#..."],
        '8' => [".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###."],
        '9' => [".###.", "#...#", "#...#", ".####", "....#", "....#", ".###."],
        'A' => [".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"],
        'B' => ["####.", "#...#", "#...#", "####.", "#...#", "#...#", "####."],
        'C' => [".###.", "#...#", "#....", "#....", "#....", "#...#", ".###."],
        'D' => ["####.", "#...#", "#...#", "#...#", "#...#", "#...#", "####."],
        'E' => ["#####", "#....", "#....", "####.", "#....", "#....", "#####"],
        'F' => ["#####", "#....", "#....", "####.", "#....", "#....", "#...."],
        'G' => [".###.", "#...#", "#....", "#.###", "#...#", "#...#", ".###."],
        'H' => ["#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"],
        'I' => [".###.", "..#..", "..#..", "..#..", "..#..", "..#..", ".###."],
        'J' => ["..###", "...#.", "...#.", "...#.", "...#.", "#..#.", ".##.."],
        'K' => ["#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#"],
        'L' => ["#....", "#....", "#....", "#....", "#....", "#....", "#####"],
        'M' => ["#...#", "##.##", "#.#.#", "#...#", "#...#", "#...#", "#...#"],
        'N' => ["#...#", "##..#", "#.#.#", "#..##", "#...#", "#...#", "#...#"],
        'O' => [".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."],
        'P' => ["####.", "#...#", "#...#", "####.", "#....", "#....", "#...."],
        'Q' => [".###.", "#...#", "#...#", "#...#", "#.#.#", "#..#.", ".##.#"],
        'R' => ["####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#"],
        'S' => [".####", "#....", "#....", ".###.", "....#", "....#", "####."],
        'T' => ["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#.."],
        'U' => ["#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."],
        'V' => ["#...#", "#...#", "#...#", "#...#", "#...#", ".#.#.", "..#.."],
        'W' => ["#...#", "#...#", "#...#", "#.#.#", "#.#.#", "##.##", "#...#"],
        'X' => ["#...#", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "#...#"],
        'Y' => ["#...#", "#...#", ".#.#.", "..#..", "..#..", "..#..", "..#.."],
        'Z' => ["#####", "....#", "...#.", "..#..", ".#...", "#....", "#####"],
        '-' => [".....", ".....", ".....", "#####", ".....", ".....", "....."],
        '.' => [".....", ".....", ".....", ".....", ".....", ".##..", ".##.."],
        '!' => ["..#..", "..#..", "..#..", "..#..", "..#..", ".....", "..#.."],
        '?' => [".###.", "#...#", "....#", "...#.", "..#..", ".....", "..#.."],
        ' ' => [".....", ".....", ".....", ".....", ".....", ".....", "....."],
        _ => return None,
    })
}

/// Renders a grid of `#` (lit) and `.` (unlit) rows as an SVG.
fn svg(rows: &[String], label: &str, class: &str, dither: bool) -> String {
    let height = rows.len();
    let width = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let (mut lit, mut off) = (String::new(), String::new());
    for (y, row) in rows.iter().enumerate() {
        for (x, c) in row.chars().enumerate() {
            let target = match c {
                '#' => &mut lit,
                '.' if dither => &mut off,
                _ => continue,
            };
            target.push_str(&format!("<rect x=\"{x}\" y=\"{y}\" width=\".875\" height=\".875\" rx=\".11\"/>"));
        }
    }
    let off = if off.is_empty() { off } else { format!("<g class=\"off\">{off}</g>") };
    format!(
        "<svg class=\"mira-pixels {class}\" viewBox=\"0 0 {:.3} {:.3}\" role=\"img\" aria-label=\"{}\">{off}{lit}</svg>",
        width as f32 - 0.125,
        height as f32 - 0.125,
        escape(label)
    )
}

pub fn mark(class: &str, dither: bool) -> String {
    let rows: Vec<String> = MARK.iter().map(|r| r.to_string()).collect();
    svg(&rows, "Mira", class, dither)
}

pub fn text(text: &str, class: &str, dither: bool) -> String {
    let glyphs: Vec<[&str; 7]> = text.chars().map(|c| glyph(c).unwrap_or(glyph(' ').unwrap())).collect();
    let rows: Vec<String> = (0..7).map(|y| glyphs.iter().map(|g| g[y]).collect::<Vec<_>>().join(".")).collect();
    svg(&rows, text, class, dither)
}

/// Expands `<mira-code lang="rust">...</mira-code>` into highlighted code.
/// The contents are taken literally (wrap template syntax in `{#raw}`),
/// with common indentation and surrounding blank lines removed.
pub fn expand_code(html: &str) -> String {
    if !html.contains("<mira-code") {
        return html.to_string();
    }
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find("<mira-code") {
        out.push_str(&rest[..i]);
        let tag_rest = &rest[i..];
        let (Some(open_end), Some(close)) = (tag_rest.find('>'), tag_rest.find("</mira-code>")) else {
            out.push_str(tag_rest);
            return out;
        };
        let tag = &tag_rest[..open_end];
        let lang = tag.split("lang=\"").nth(1).and_then(|s| s.split('"').next()).unwrap_or("");
        let code = dedent(&tag_rest[open_end + 1..close]);
        out.push_str(&crate::content::highlight(&code, lang));
        rest = &tag_rest[close + "</mira-code>".len()..];
    }
    out.push_str(rest);
    out
}

fn dedent(code: &str) -> String {
    let lines: Vec<&str> = code.lines().collect();
    let first = lines.iter().position(|l| !l.trim().is_empty()).unwrap_or(0);
    let last = lines.iter().rposition(|l| !l.trim().is_empty()).map_or(0, |i| i + 1);
    let lines = &lines[first..last.max(first)];
    let indent = lines.iter().filter(|l| !l.trim().is_empty()).map(|l| l.len() - l.trim_start().len()).min().unwrap_or(0);
    let mut out: String = lines.iter().map(|l| l.get(indent..).unwrap_or("").trim_end()).collect::<Vec<_>>().join("\n");
    out.push('\n');
    out
}

/// Expands `<mira-mark ... />` and `<mira-pixels text="..." ... />`.
pub fn expand(html: &str) -> String {
    if !html.contains("<mira-") {
        return html.to_string();
    }
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find("<mira-") {
        out.push_str(&rest[..i]);
        let tag_rest = &rest[i..];
        let Some(end) = tag_rest.find('>') else { break };
        let tag = &tag_rest[1..end].trim_end_matches('/');
        let name = tag.split_whitespace().next().unwrap_or("");
        let attr = |key: &str| {
            let needle = format!("{key}=\"");
            tag.find(&needle).and_then(|s| tag[s + needle.len()..].split('"').next()).map(crate::twin::decode_entities)
        };
        let dither = tag.split_whitespace().any(|w| w == "dither");
        let class = attr("class").unwrap_or_default();
        let rendered = match name {
            "mira-mark" => Some(mark(&escape(&class), dither)),
            "mira-pixels" => Some(text(&attr("text").unwrap_or_default(), &escape(&class), dither)),
            _ => None,
        };
        match rendered {
            Some(svg) => {
                out.push_str(&svg);
                rest = &tag_rest[end + 1..];
                // Tolerate an explicit closing tag.
                for close in ["</mira-mark>", "</mira-pixels>"] {
                    if let Some(after) = rest.strip_prefix(close) {
                        rest = after;
                    }
                }
            }
            None => {
                out.push_str("<mira-");
                rest = &tag_rest["<mira-".len()..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_mark_and_text() {
        let html = expand(r#"<p><mira-mark class="logo" /> <mira-pixels text="404" dither></mira-pixels></p>"#);
        assert!(html.starts_with(r#"<p><svg class="mira-pixels logo" viewBox="0 0 4.875 4.875" role="img" aria-label="Mira">"#), "{html}");
        assert!(html.contains(r#"aria-label="404""#), "{html}");
        assert!(html.contains(r#"<g class="off">"#), "{html}");
        assert!(html.ends_with("</svg></p>"), "{html}");
    }

    #[test]
    fn expands_code_blocks() {
        let html = expand_code("<div><mira-code lang=\"json\">\n    {\"a\": 1}\n      </mira-code></div>");
        assert!(html.starts_with(r#"<div><pre class="mira-code" data-lang="json"><code>"#), "{html}");
        assert!(html.ends_with("</code></pre>\n</div>"), "{html}");
        assert!(!html.contains("    {"), "{html}");
    }

    #[test]
    fn leaves_other_tags() {
        assert_eq!(expand("<mira-dev-overlay></mira-dev-overlay>"), "<mira-dev-overlay></mira-dev-overlay>");
    }
}
