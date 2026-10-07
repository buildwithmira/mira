use std::path::Path;

use anyhow::{Result, anyhow, bail};
use std::sync::LazyLock;

use pulldown_cmark::{CodeBlockKind, CowStr, Event, Options, Parser, Tag, TagEnd};
use syntect::html::{ClassStyle, ClassedHTMLGenerator};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use crate::html::escape;
use serde_json::{Map, Value};

/// A source file split into frontmatter data and body.
pub struct Document {
    pub data: Map<String, Value>,
    pub body: String,
    /// 1-based line on which `body` starts in the source file.
    pub body_line: usize,
}

pub fn parse_document(src: &str, path: &Path) -> Result<Document> {
    let src = src.replace("\r\n", "\n");
    let Some(rest) = src.strip_prefix("---\n") else {
        return Ok(Document { data: Map::new(), body: src, body_line: 1 });
    };
    let (yaml, body) = match rest.find("\n---\n") {
        Some(i) => (&rest[..i], &rest[i + 5..]),
        None if rest.ends_with("\n---") => (&rest[..rest.len() - 4], ""),
        None if rest.starts_with("---\n") || rest == "---" => ("", rest.get(4..).unwrap_or("")),
        None => bail!("{}:1: frontmatter opened with --- but never closed", path.display()),
    };
    let data = if yaml.trim().is_empty() {
        Map::new()
    } else {
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).map_err(|e| {
            let line = e.location().map(|l| l.line() + 1).unwrap_or(1);
            anyhow!("{}:{}: invalid frontmatter: {}", path.display(), line, strip_yaml_positions(&e.to_string()))
        })?;
        match serde_json::to_value(value)? {
            Value::Object(map) => map,
            Value::Null => Map::new(),
            _ => bail!("{}:2: frontmatter must be a mapping of keys to values", path.display()),
        }
    };
    let body_line = yaml.lines().count() + 3;
    Ok(Document { data, body: body.to_string(), body_line })
}

/// Drops serde_yaml's " at line N column M" clauses, which count from the
/// start of the frontmatter rather than the file and would contradict the
/// file location reported alongside.
fn strip_yaml_positions(message: &str) -> String {
    let mut out = String::new();
    let mut rest = message;
    while let Some(i) = rest.find(" at line ") {
        out.push_str(&rest[..i]);
        let tail = &rest[i + " at line ".len()..];
        let skip =
            tail.find(|c: char| !(c.is_ascii_digit() || c == ' ' || c.is_ascii_alphabetic() && "column".contains(c))).unwrap_or(tail.len());
        rest = &tail[skip..];
    }
    out.push_str(rest);
    out
}

pub struct Markdown {
    pub html: String,
    pub words: usize,
    /// Second and third level headings, for tables of contents.
    pub toc: Vec<Heading>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Heading {
    pub level: u8,
    pub id: String,
    pub text: String,
}

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);

/// Renders Markdown at build time: slugged heading ids, smart punctuation,
/// GitHub style callouts, and code highlighted into `hl-` classes.
pub fn render_markdown(src: &str) -> Markdown {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_SMART_PUNCTUATION
        | Options::ENABLE_GFM;
    let mut events: Vec<Event> = Parser::new_ext(src, options).collect();

    let mut words = 0;
    let mut toc = Vec::new();
    let mut used = std::collections::HashMap::<String, usize>::new();
    for i in 0..events.len() {
        if let Event::Text(text) = &events[i] {
            words += text.split_whitespace().count();
        }
        let Event::Start(Tag::Heading { level, id, .. }) = &events[i] else { continue };
        let level = *level as u8;
        let mut text = String::new();
        for event in &events[i + 1..] {
            match event {
                Event::End(TagEnd::Heading(_)) => break,
                Event::Text(t) | Event::Code(t) => text.push_str(t),
                _ => {}
            }
        }
        let slug = match id {
            Some(id) => id.to_string(),
            None => {
                let mut slug = slugify(&text);
                let count = used.entry(slug.clone()).or_insert(0);
                *count += 1;
                if *count > 1 {
                    slug = format!("{slug}-{}", *count - 1);
                }
                slug
            }
        };
        if let Event::Start(Tag::Heading { id, .. }) = &mut events[i] {
            *id = Some(CowStr::from(slug.clone()));
        }
        if matches!(level, 2 | 3) {
            toc.push(Heading { level, id: slug, text });
        }
    }

    // Replace fenced and indented code blocks with highlighted HTML.
    let mut out: Vec<Event> = Vec::with_capacity(events.len());
    let mut code: Option<(String, String)> = None;
    for event in events {
        match (&mut code, event) {
            (None, Event::Start(Tag::CodeBlock(kind))) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => info.split_whitespace().next().unwrap_or("").to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                code = Some((lang, String::new()));
            }
            (Some((_, body)), Event::Text(t)) => body.push_str(&t),
            (Some(_), Event::End(TagEnd::CodeBlock)) => {
                let (lang, body) = code.take().unwrap();
                out.push(Event::Html(CowStr::from(highlight(&body, &lang))));
            }
            (Some(_), _) => {}
            (None, event) => out.push(event),
        }
    }

    let out = frame_images(out);
    let mut html = String::with_capacity(src.len() * 3 / 2);
    pulldown_cmark::html::push_html(&mut html, out.into_iter());
    Markdown { html, words, toc }
}

/// Turns `![alt](src "caption")` into `<mira-frame>`. An image alone in its
/// paragraph replaces the paragraph, since a figure cannot sit inside one.
fn frame_images(events: Vec<Event<'_>>) -> Vec<Event<'_>> {
    let mut out: Vec<Event> = Vec::with_capacity(events.len());
    let mut iter = events.into_iter().peekable();
    while let Some(event) = iter.next() {
        let Event::Start(Tag::Image { dest_url, title, .. }) = event else {
            out.push(event);
            continue;
        };
        let mut alt = String::new();
        for inner in iter.by_ref() {
            match inner {
                Event::End(TagEnd::Image) => break,
                Event::Text(t) | Event::Code(t) => alt.push_str(&t),
                _ => {}
            }
        }
        let caption = if title.is_empty() { String::new() } else { format!(" caption=\"{}\"", escape(&title)) };
        let frame = format!("<mira-frame src=\"{}\" alt=\"{}\"{caption}></mira-frame>", escape(&dest_url), escape(&alt));
        let alone = matches!(out.last(), Some(Event::Start(Tag::Paragraph))) && matches!(iter.peek(), Some(Event::End(TagEnd::Paragraph)));
        if alone {
            out.pop();
            iter.next();
            out.push(Event::Html(CowStr::from(frame + "\n")));
        } else {
            out.push(Event::InlineHtml(CowStr::from(frame)));
        }
    }
    out
}

/// Highlights `code` as `lang` into spans with `hl-` scope classes. Unknown
/// languages render as escaped plain text.
pub fn highlight(code: &str, lang: &str) -> String {
    let token = match lang {
        "mira" | "svelte" | "vue" => "html",
        "ts" | "tsx" | "jsx" | "typescript" | "mjs" => "js",
        "jsonc" | "json5" => "json",
        "sh" | "shell" | "zsh" | "console" => "bash",
        other => other,
    };
    let syntaxes = &*SYNTAXES;
    let body = match (!token.is_empty()).then(|| syntaxes.find_syntax_by_token(token)).flatten() {
        Some(syntax) => {
            let mut html = ClassedHTMLGenerator::new_with_class_style(syntax, syntaxes, ClassStyle::SpacedPrefixed { prefix: "hl-" });
            for line in LinesWithEndings::from(code) {
                if html.parse_html_for_line_which_includes_newline(line).is_err() {
                    return plain(code, lang);
                }
            }
            html.finalize()
        }
        None => return plain(code, lang),
    };
    format!("<pre class=\"mira-code\" data-lang=\"{}\"><code>{body}</code></pre>\n", escape(lang))
}

fn plain(code: &str, lang: &str) -> String {
    format!("<pre class=\"mira-code\" data-lang=\"{}\"><code>{}</code></pre>\n", escape(lang), escape(code))
}

/// Minutes to read at 230 words per minute, never less than one.
pub fn reading_time(words: usize) -> usize {
    words.div_ceil(230).max(1)
}

pub fn slugify(text: &str) -> String {
    let mut slug = String::new();
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-').to_string();
    if slug.is_empty() { "section".into() } else { slug }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_frontmatter() {
        let doc = parse_document("---\ntitle: Hi\n---\n# Body\n", Path::new("a.md")).unwrap();
        assert_eq!(doc.data["title"], "Hi");
        assert_eq!(doc.body, "# Body\n");
        assert_eq!(doc.body_line, 4);
    }

    #[test]
    fn reports_frontmatter_line() {
        let err = parse_document("---\ntitle: Hi\nbad: [\n---\n", Path::new("a.md")).err().unwrap().to_string();
        assert!(err.starts_with("a.md:4: invalid frontmatter"), "{err}");
        assert!(!err.contains("column"), "{err}");
    }

    #[test]
    fn highlights_code_and_collects_toc() {
        let md = render_markdown("## Setup\n\n```rust\nfn main() {}\n```\n\n> [!NOTE]\n> Hi\n");
        assert!(md.html.contains(r#"<pre class="mira-code" data-lang="rust">"#), "{}", md.html);
        assert!(md.html.contains("hl-storage"), "{}", md.html);
        assert!(md.html.contains("markdown-alert-note"), "{}", md.html);
        assert_eq!(md.toc[0].id, "setup");
        assert!(highlight("<b>", "nope").contains("&lt;b&gt;"));
    }

    #[test]
    fn frames_markdown_images() {
        let md = render_markdown("![A build](./a.png \"Cold build\")\n\nText with ![icon](/i.svg) inline.\n");
        assert!(md.html.starts_with(r#"<mira-frame src="./a.png" alt="A build" caption="Cold build"></mira-frame>"#), "{}", md.html);
        assert!(md.html.contains(r#"<p>Text with <mira-frame src="/i.svg" alt="icon"></mira-frame> inline.</p>"#), "{}", md.html);
    }

    #[test]
    fn headings_get_unique_ids() {
        let md = render_markdown("# Hello World\n\n## Hello World\n");
        assert!(md.html.contains(r#"<h1 id="hello-world">"#), "{}", md.html);
        assert!(md.html.contains(r#"<h2 id="hello-world-1">"#), "{}", md.html);
    }
}
