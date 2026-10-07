use std::sync::LazyLock;

pub const BASE_CSS: &str = include_str!("../runtime/base.css");
pub const DEV_JS: &str = include_str!("../runtime/dev.js");
pub const OVERLAY_CSS: &str = include_str!("../runtime/overlay.css");
pub const SEARCH_JS: &str = include_str!("../runtime/search.js");
/// Video frame controls, loaded only on pages with a video frame.
pub const FRAME_JS: &str = include_str!("../runtime/frame.js");
/// The 404 page used when a project has no `routes/404`.
pub const NOT_FOUND_MIRA: &str = include_str!("../runtime/404.mira");

/// Component CSS split into `(class, css)` blocks at `@component` markers.
pub static COMPONENTS: LazyLock<Vec<(String, String)>> = LazyLock::new(|| {
    include_str!("../runtime/components.css")
        .split("/* @component ")
        .skip(1)
        .filter_map(|block| {
            let (class, css) = block.split_once("*/")?;
            Some((class.trim().to_string(), compact_css(css)))
        })
        .collect()
});

/// The component CSS a page needs: every block whose class appears in it.
pub fn components_for(html: &str) -> String {
    let used: String = COMPONENTS.iter().filter(|(class, _)| html.contains(class.as_str())).map(|(_, css)| css.as_str()).collect();
    if used.is_empty() { used } else { format!("@layer mira.components{{{used}}}") }
}

/// The client runtime with comment lines dropped and lines trimmed.
pub static RUNTIME_JS: LazyLock<String> = LazyLock::new(|| strip(include_str!("../runtime/mira.js")));

fn strip(src: &str) -> String {
    src.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("//")).collect::<Vec<_>>().join("\n")
}

/// Collapses CSS whitespace and drops comments. Not a full minifier, but
/// enough to keep inlined critical CSS lean.
pub fn compact_css(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    let mut pending_space = false;
    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            while let Some(c) = chars.next() {
                if c == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    break;
                }
            }
            continue;
        }
        if c.is_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space {
            let last = out.chars().last();
            let joins = |ch: char| matches!(ch, '{' | '}' | ';' | ',');
            if !out.is_empty() && !last.is_some_and(joins) && !joins(c) {
                out.push(' ');
            }
            pending_space = false;
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compacts_css() {
        let css = "a , b {\n  color: red; /* note */\n}\n.x :is(h2) { margin: 0 auto }";
        assert_eq!(compact_css(css), "a,b{color: red;}.x :is(h2){margin: 0 auto}");
    }

    #[test]
    fn splits_components() {
        assert!(COMPONENTS.iter().any(|(c, _)| c == "mira-btn"));
        let css = components_for(r#"<a class="mira-btn">Go</a>"#);
        assert!(css.contains(".mira-btn{") && !css.contains(".mira-glass"), "{css}");
        assert_eq!(components_for("<p>plain</p>"), "");
    }

    #[test]
    fn runtime_fits_budget() {
        use std::io::Write;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        gz.write_all(RUNTIME_JS.as_bytes()).unwrap();
        let size = gz.finish().unwrap().len();
        assert!(size < 2048, "runtime is {size} bytes gzipped, budget is 2048");
    }
}
