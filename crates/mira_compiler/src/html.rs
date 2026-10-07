use anyhow::{Result, bail};
use base64::Engine;
use sha2::{Digest, Sha256};

pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Rewrites `mira-morph="name"` attributes to `data-mira-morph` and returns
/// the CSS that gives each element its `view-transition-name`. Morph names
/// are compiled into the page stylesheet rather than inline styles so the
/// page keeps a strict CSP.
pub fn compile_morphs(html: &str) -> Result<(String, String)> {
    const ATTR: &str = "mira-morph=\"";
    let mut out = String::with_capacity(html.len());
    let mut names: Vec<String> = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find(ATTR) {
        let preceded_by_space = rest[..i].ends_with(|c: char| c.is_ascii_whitespace());
        if !preceded_by_space {
            out.push_str(&rest[..i + ATTR.len()]);
            rest = &rest[i + ATTR.len()..];
            continue;
        }
        let value_start = i + ATTR.len();
        let Some(len) = rest[value_start..].find('"') else {
            bail!("unterminated mira-morph attribute");
        };
        let name = morph_name(&rest[value_start..value_start + len])?;
        if names.contains(&name) {
            bail!(
                "mira-morph=\"{name}\" is used twice on one page; shared element names must be unique per page or the transition is skipped"
            );
        }
        out.push_str(&rest[..i]);
        out.push_str("data-mira-morph=\"");
        out.push_str(&name);
        out.push('"');
        names.push(name);
        rest = &rest[value_start + len + 1..];
    }
    out.push_str(rest);

    let css = names
        .iter()
        .map(|n| format!("[data-mira-morph=\"{n}\"]{{view-transition-name:{n};view-transition-class:mira-morph}}"))
        .collect::<String>();
    Ok((out, css))
}

/// Normalizes a morph name into a valid CSS custom identifier.
fn morph_name(raw: &str) -> Result<String> {
    let mut name: String = raw.trim().chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
    if name.is_empty() {
        bail!("mira-morph needs a name");
    }
    if name.starts_with(|c: char| c.is_ascii_digit()) || name.starts_with("--") {
        name.insert_str(0, "m-");
    }
    if matches!(name.as_str(), "none" | "auto" | "root" | "match-element" | "inherit" | "initial" | "unset") {
        bail!("mira-morph=\"{name}\" is a reserved CSS keyword, pick another name");
    }
    Ok(name)
}

/// Replaces `mira-nav` on links with `aria-current="page"` when the link
/// points at `url` (or a section containing it), and drops it otherwise.
pub fn mark_current_links(html: &str, url: &str) -> String {
    const ATTR: &str = " mira-nav";
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find(ATTR) {
        let after = &rest[i + ATTR.len()..];
        if !after.starts_with(|c: char| c.is_whitespace() || c == '>' || c == '/') {
            out.push_str(&rest[..i + ATTR.len()]);
            rest = after;
            continue;
        }
        let tag_start = rest[..i].rfind('<').unwrap_or(0);
        let tag = &rest[tag_start..i];
        let href = tag.split("href=\"").nth(1).and_then(|h| h.split('"').next()).unwrap_or("");
        let current = href == url || (href.len() > 1 && href.ends_with('/') && url.starts_with(href));
        out.push_str(&rest[..i]);
        if current {
            out.push_str(" aria-current=\"page\"");
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Gives every local `<img>` its intrinsic `width` and `height` (read from
/// the file under `public/`) so it reserves space before loading, and
/// defaults it to lazy loading with async decoding.
pub fn size_images(html: &str, public: &std::path::Path) -> String {
    if !html.contains("<img") {
        return html.to_string();
    }
    let mut out = String::with_capacity(html.len() + 64);
    let mut rest = html;
    while let Some(i) = rest.find("<img") {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let end = rest.find('>').map_or(rest.len(), |e| e + 1);
        let tag = &rest[..end];
        let body = tag.trim_end_matches('>').trim_end_matches('/').trim_end();
        let mut extra = String::new();
        let src = body.split(" src=\"").nth(1).and_then(|s| s.split('"').next()).unwrap_or("");
        if src.starts_with('/') && !src.starts_with("//") && !body.contains(" width=") && !body.contains(" height=") {
            let file = public.join(src.trim_start_matches('/').split(['?', '#']).next().unwrap_or(""));
            if let Ok(size) = imagesize::size(&file) {
                extra.push_str(&format!(" width=\"{}\" height=\"{}\"", size.width, size.height));
            }
        }
        if !body.contains(" loading=") {
            extra.push_str(" loading=\"lazy\"");
        }
        if !body.contains(" decoding=") {
            extra.push_str(" decoding=\"async\"");
        }
        out.push_str(body);
        out.push_str(&extra);
        out.push('>');
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// The CSP source expression for an inline script or style.
pub fn csp_hash(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    format!("'sha256-{}'", base64::engine::general_purpose::STANDARD.encode(digest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_morph_attributes() {
        let (html, css) = compile_morphs(r#"<h1 class="t" mira-morph="title 1">Hi</h1>"#).unwrap();
        assert_eq!(html, r#"<h1 class="t" data-mira-morph="title-1">Hi</h1>"#);
        assert!(css.contains("view-transition-name:title-1"));
    }

    #[test]
    fn sizes_local_images() {
        let dir = std::env::temp_dir().join("mira-size-images");
        std::fs::create_dir_all(&dir).unwrap();
        // A 3 by 2 GIF.
        std::fs::write(dir.join("a.gif"), b"GIF89a\x03\x00\x02\x00\x00\x00\x00;").unwrap();
        let html = size_images(r#"<img src="/a.gif" alt=""><img src="https://x/y.png" alt="" loading="eager">"#, &dir);
        assert_eq!(
            html,
            r#"<img src="/a.gif" alt="" width="3" height="2" loading="lazy" decoding="async"><img src="https://x/y.png" alt="" loading="eager" decoding="async">"#
        );
    }

    #[test]
    fn marks_current_links() {
        let html = r#"<a href="/" mira-nav>Home</a><a href="/posts/" mira-nav>Writing</a>"#;
        assert_eq!(mark_current_links(html, "/posts/a/"), r#"<a href="/">Home</a><a href="/posts/" aria-current="page">Writing</a>"#);
        assert_eq!(mark_current_links(html, "/"), r#"<a href="/" aria-current="page">Home</a><a href="/posts/">Writing</a>"#);
    }

    #[test]
    fn rejects_duplicate_morph_names() {
        assert!(compile_morphs(r#"<a mira-morph="x"></a><b mira-morph="x"></b>"#).is_err());
    }

    #[test]
    fn leaves_text_mentions_alone() {
        let src = "<code>data-mira-morph=\"x\"</code>";
        assert_eq!(compile_morphs(src).unwrap().0, src);
    }
}
