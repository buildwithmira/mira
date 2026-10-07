//! Build time checks over rendered pages: internal links and assets must
//! resolve, anchors should exist, and images should carry alt text.

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Result, bail};

pub struct Page<'a> {
    pub url: &'a str,
    pub source: &'a str,
    pub html: &'a str,
}

/// Every URL path the build serves, beyond rendered pages.
pub fn known_paths(pages: &[Page], extra: impl IntoIterator<Item = String>, public: &Path) -> HashSet<String> {
    let mut known: HashSet<String> = HashSet::new();
    for page in pages {
        known.insert(page.url.to_string());
        if let Some(stripped) = page.url.strip_suffix('/').filter(|s| !s.is_empty()) {
            known.insert(stripped.to_string());
            known.insert(format!("{}index.html", page.url));
        }
    }
    known.extend(extra);
    if public.is_dir() {
        for entry in walkdir::WalkDir::new(public).into_iter().filter_map(Result::ok) {
            if entry.file_type().is_file()
                && let Ok(rel) = entry.path().strip_prefix(public)
            {
                known.insert(format!("/{}", rel.to_string_lossy().replace('\\', "/")));
            }
        }
    }
    known
}

/// Fails on internal links or assets that do not resolve, and returns
/// warnings for missing anchors, images without alt text, and repeated h1s.
pub fn check(pages: &[Page], known: &HashSet<String>, root: &Path) -> Result<Vec<String>> {
    let mut warnings = Vec::new();
    for page in pages {
        for (attr, value) in references(page.html) {
            if value.starts_with("//") || value.starts_with("/_mira/") {
                continue;
            }
            let (path, fragment) = match value.split_once('#') {
                Some((p, f)) => (p, Some(f)),
                None => (value, None),
            };
            let path = path.split('?').next().unwrap_or("");
            if path.is_empty() {
                // A same page anchor.
                if let Some(f) = fragment.filter(|f| !f.is_empty())
                    && !has_id(page.html, f)
                {
                    warnings.push(format!("{}: link to #{f} on {}, but no element has that id", page.source, page.url));
                }
                continue;
            }
            if !path.starts_with('/') {
                continue;
            }
            let decoded = percent_decode(path);
            if !known.contains(path) && !known.contains(&decoded) {
                let line = line_of(root, page.source, value);
                let loc = match line {
                    Some(n) => format!("{}:{n}", page.source),
                    None => page.source.to_string(),
                };
                let what = if attr == "href" { "links to" } else { "loads" };
                bail!(
                    "{loc}: {} {what} {value}, which does not exist\nhint: fix the path, add the page under routes/, or add the file under public/",
                    page.url
                );
            }
            if let Some(f) = fragment.filter(|f| !f.is_empty())
                && let Some(target) = pages.iter().find(|p| p.url == path || p.url.trim_end_matches('/') == path)
                && !has_id(target.html, f)
            {
                warnings.push(format!("{}: link to {value}, but {} has no element with id \"{f}\"", page.source, target.url));
            }
        }
        for img in tags(page.html, "<img") {
            if !img.contains(" alt=") {
                warnings.push(format!("{}: an <img> on {} has no alt text; use alt=\"\" for decorative images", page.source, page.url));
            }
        }
        let h1s = page.html.matches("<h1").count();
        if h1s > 1 {
            warnings.push(format!("{}: {} has {h1s} <h1> elements; keep one per page", page.source, page.url));
        }
    }
    Ok(warnings)
}

/// SEO checks across rendered pages. Pages marked noindex and the 404 page
/// are skipped.
pub fn seo(pages: &[crate::outputs::PageMeta]) -> Vec<String> {
    let mut warnings = Vec::new();
    let listed: Vec<&crate::outputs::PageMeta> = pages.iter().filter(|p| p.listed()).collect();
    for page in &listed {
        let title_len = page.document_title.chars().count();
        if title_len > 60 {
            warnings.push(format!("{}: title is {title_len} characters; search results show about 60", page.url));
        }
        match &page.own_description {
            None => warnings
                .push(format!("{}: no description, so it shares the site description; add `description:` to its frontmatter", page.url)),
            Some(d) => {
                let len = d.chars().count();
                if len < 50 {
                    warnings.push(format!("{}: description is {len} characters; aim for 50 to 160", page.url));
                } else if len > 160 {
                    warnings.push(format!("{}: description is {len} characters; search results cut it near 160", page.url));
                }
            }
        }
    }
    let mut by_title: std::collections::BTreeMap<&str, Vec<&str>> = Default::default();
    let mut by_description: std::collections::BTreeMap<&str, Vec<&str>> = Default::default();
    for page in &listed {
        by_title.entry(page.document_title.as_str()).or_default().push(&page.url);
        if let Some(d) = &page.own_description {
            by_description.entry(d.as_str()).or_default().push(&page.url);
        }
    }
    for (title, urls) in by_title.iter().filter(|(_, u)| u.len() > 1) {
        warnings.push(format!("{} share the title \"{title}\"; give each page its own", urls.join(", ")));
    }
    for (_, urls) in by_description.iter().filter(|(_, u)| u.len() > 1) {
        warnings.push(format!("{} share the same description; give each page its own", urls.join(", ")));
    }
    warnings
}

fn references(html: &str) -> Vec<(&'static str, &str)> {
    let mut out = Vec::new();
    for attr in ["href", "src"] {
        let needle = format!(" {attr}=\"");
        let mut rest = html;
        while let Some(i) = rest.find(&needle) {
            rest = &rest[i + needle.len()..];
            if let Some(end) = rest.find('"') {
                out.push((attr, &rest[..end]));
                rest = &rest[end..];
            }
        }
    }
    out
}

fn tags<'a>(html: &'a str, open: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find(open) {
        rest = &rest[i..];
        let end = rest.find('>').unwrap_or(rest.len());
        out.push(&rest[..end]);
        rest = &rest[end..];
    }
    out
}

fn has_id(html: &str, id: &str) -> bool {
    html.contains(&format!(" id=\"{id}\""))
}

fn line_of(root: &Path, source: &str, needle: &str) -> Option<usize> {
    let src = std::fs::read_to_string(root.join(source)).ok()?;
    src.lines().position(|l| l.contains(needle)).map(|i| i + 1)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(b) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pages() -> Vec<Page<'static>> {
        vec![
            Page {
                url: "/",
                source: "routes/index.md",
                html: r##"<h1 id="top">Hi</h1><a href="/docs/#install">Docs</a><a href="#top">Top</a><img src="/logo.svg" alt="">"##,
            },
            Page { url: "/docs/", source: "routes/docs.md", html: r#"<h2 id="install">Install</h2>"# },
        ]
    }

    #[test]
    fn passes_when_links_resolve() {
        let pages = pages();
        let known = known_paths(&pages, ["/logo.svg".to_string()], Path::new("missing"));
        assert!(check(&pages, &known, Path::new(".")).unwrap().is_empty());
    }

    #[test]
    fn fails_on_broken_links() {
        let mut pages = pages();
        pages[1].html = r#"<a href="/nope/">x</a>"#;
        let known = known_paths(&pages, ["/logo.svg".to_string()], Path::new("missing"));
        let err = check(&pages, &known, Path::new(".")).unwrap_err().to_string();
        assert!(err.starts_with("routes/docs.md: /docs/ links to /nope/"), "{err}");
    }

    #[test]
    fn warns_on_missing_anchor_and_alt() {
        let pages = vec![Page { url: "/", source: "a.md", html: r##"<a href="#gone">x</a><img src="/a.png">"## }];
        let known = known_paths(&pages, ["/a.png".to_string()], Path::new("missing"));
        let warnings = check(&pages, &known, Path::new(".")).unwrap();
        assert_eq!(warnings.len(), 2, "{warnings:?}");
    }
}
