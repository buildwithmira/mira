//! The build time search index, served at `/_mira/search.json`. The same
//! file backs `<mira-search>` in the browser and the JSON search API agents
//! call directly.

use serde::Serialize;

use crate::twin::decode_entities;

/// Text kept per page. Titles and headings carry most of the ranking, so a
/// capped body keeps the index small on large sites.
const TEXT_LIMIT: usize = 1600;

#[derive(Serialize)]
pub struct Doc {
    pub url: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub headings: Vec<Heading>,
    pub text: String,
}

#[derive(Serialize)]
pub struct Heading {
    pub id: String,
    pub text: String,
}

pub fn doc(url: &str, title: &str, description: Option<&str>, html: &str) -> Doc {
    let main = main_element(html).unwrap_or(html);
    let mut headings = Vec::new();
    for tag in ["<h2", "<h3"] {
        let mut rest = main;
        while let Some(i) = rest.find(tag) {
            rest = &rest[i..];
            let Some(open_end) = rest.find('>') else { break };
            let open = &rest[..open_end];
            let close = format!("</{}", &tag[1..]);
            let Some(close_at) = rest.find(&close) else { break };
            if let Some(id) = open.split("id=\"").nth(1).and_then(|s| s.split('"').next()) {
                headings.push(Heading { id: id.to_string(), text: plain_text(&rest[open_end + 1..close_at]) });
            }
            rest = &rest[close_at..];
        }
    }
    let mut text = plain_text(main);
    if text.len() > TEXT_LIMIT {
        let cut = (0..=TEXT_LIMIT).rev().find(|&i| text.is_char_boundary(i)).unwrap_or(0);
        text.truncate(cut);
    }
    Doc { url: url.to_string(), title: title.to_string(), description: description.map(str::to_string), headings, text }
}

fn main_element(html: &str) -> Option<&str> {
    let start = html.find("<main")?;
    let open_end = start + html[start..].find('>')? + 1;
    let end = html.rfind("</main>")?;
    (end >= open_end).then(|| &html[open_end..end])
}

/// Strips tags (dropping script, style, svg, and nav contents) and collapses
/// whitespace.
pub fn plain_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut rest = html;
    let mut skip: Option<&str> = None;
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            if skip.is_none() {
                out.push_str(&decode_entities(rest));
            }
            break;
        };
        if skip.is_none() {
            out.push_str(&decode_entities(&rest[..lt]));
        }
        rest = &rest[lt..];
        let end = rest.find('>').map_or(rest.len(), |e| e + 1);
        let tag = &rest[1..end.saturating_sub(1)];
        let name = tag.trim_start_matches('/').split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("");
        match skip {
            Some(s) if tag.starts_with('/') && name == s => skip = None,
            None if !tag.starts_with('/') && matches!(name, "script" | "style" | "svg" | "nav") => {
                skip = Some(match name {
                    "script" => "script",
                    "style" => "style",
                    "svg" => "svg",
                    _ => "nav",
                });
            }
            _ => {}
        }
        out.push(' ');
        rest = &rest[end..];
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_main_content() {
        let html = r#"<nav>Menu</nav><main><h1>Docs</h1><h2 id="install">Install &amp; run</h2><p>Run <code>mira new</code>.</p><svg><text>x</text></svg></main>"#;
        let d = doc("/docs/", "Docs", None, html);
        assert_eq!(d.headings[0].id, "install");
        assert_eq!(d.headings[0].text, "Install & run");
        assert_eq!(d.text, "Docs Install & run Run mira new .");
    }
}
