//! MDX: Markdown with components, rendered at build time.
//!
//! An `.mdx` file in `content/` or `routes/` is Markdown in which a
//! capitalized tag, such as `<Callout tone="warn">Text</Callout>`, renders
//! the Mira component `components/Callout.mira`. The component reads its
//! props as `{{ props.tone }}` and places the tag's children, rendered as
//! Markdown, at `<slot />`. No JavaScript runs, at build time or in the
//! browser: `import` lines are dropped, since components are found by
//! name, and anything that would need code to run is a build error with
//! the line to fix.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use serde_json::{Map, Value, json};

use crate::content::render_markdown;
use crate::template::{Component, parse_component};

/// Components by name, from `components/*.mira`.
pub type Components = HashMap<String, (PathBuf, Component)>;

pub fn load_components(root: &Path) -> Result<Components> {
    let dir = root.join("components");
    let mut components = HashMap::new();
    if !dir.is_dir() {
        return Ok(components);
    }
    for item in std::fs::read_dir(&dir)? {
        let path = item?.path();
        if path.extension().is_some_and(|e| e == "mira") {
            let name = path.file_stem().unwrap().to_string_lossy().to_string();
            let shown = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            let component = parse_component(&std::fs::read_to_string(&path)?, &shown)?;
            components.insert(name, (shown, component));
        }
    }
    Ok(components)
}

/// An MDX body rendered: its HTML and headings, the Markdown agents get (components
/// replaced by Markdown of what they render), the CSS of the components it
/// uses, and its word count and headings.
pub struct Rendered {
    pub twin: String,
    pub css: String,
    pub markdown: crate::content::Markdown,
}

pub fn render(body: &str, path: &Path, first_line: usize, components: &Components) -> Result<Rendered> {
    let mut state = State { path, components, parts: Vec::new(), css: Vec::new() };
    let (markdown, twin) = state.expand(body, first_line, 0)?;
    let mut md = render_markdown(&markdown);
    md.html = state.finish(md.html);
    let css = state.css.concat();
    Ok(Rendered { twin, css, markdown: md })
}

struct State<'a> {
    path: &'a Path,
    components: &'a Components,
    /// Rendered components, each standing in the Markdown as a token until
    /// the Markdown is rendered.
    parts: Vec<String>,
    /// CSS of each component used, once.
    css: Vec<String>,
}

fn token(n: usize) -> String {
    format!("MIRAMDX{n}X")
}

impl State<'_> {
    fn at(&self, line: usize) -> String {
        format!("{}:{line}", self.path.display())
    }

    /// Replaces components with tokens (for rendering) and with Markdown
    /// (for agents), drops imports and comments, and refuses JavaScript.
    fn expand(&mut self, src: &str, first_line: usize, depth: usize) -> Result<(String, String)> {
        if depth > 16 {
            bail!("{}: components are nested more than 16 deep", self.at(first_line));
        }
        let mut out = String::with_capacity(src.len());
        let mut twin = String::with_capacity(src.len());
        let mut fenced = false;
        let mut i = 0;
        let bytes = src.as_bytes();
        let line_at = |i: usize| first_line + src[..i].matches('\n').count();
        while i < src.len() {
            let line_start = i == 0 || bytes[i - 1] == b'\n';
            if line_start {
                let line_end = src[i..].find('\n').map_or(src.len(), |n| i + n + 1);
                let line = &src[i..line_end];
                let trimmed = line.trim_start();
                if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                    fenced = !fenced;
                }
                if fenced || trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                    out.push_str(line);
                    twin.push_str(line);
                    i = line_end;
                    continue;
                }
                if line.starts_with("import ") {
                    i = line_end;
                    continue;
                }
                if line.starts_with("export ") {
                    bail!(
                        "{}: MDX exports run JavaScript, which Mira does not\nhint: put values in the frontmatter and read them in components as props",
                        self.at(line_at(i))
                    );
                }
            }
            let rest = &src[i..];
            let c = rest.chars().next().unwrap();
            if c == '`' {
                // Inline code is copied as written.
                let run = rest.len() - rest.trim_start_matches('`').len();
                let close = rest[run..].find(&rest[..run]).map_or(rest.len(), |n| run + n + run);
                out.push_str(&rest[..close]);
                twin.push_str(&rest[..close]);
                i += close;
                continue;
            }
            if c == '\\' && rest[1..].starts_with(['{', '}', '<']) {
                out.push_str(&rest[..2]);
                twin.push_str(&rest[1..2]);
                i += 2;
                continue;
            }
            if c == '{' {
                if rest.starts_with("{/*")
                    && let Some(end) = rest.find("*/}")
                {
                    i += end + 3;
                    continue;
                }
                bail!(
                    "{}: {{ starts a JavaScript expression in MDX, which Mira does not run\nhint: write \\{{ for a literal brace, or pass values to components as props",
                    self.at(line_at(i))
                );
            }
            if c == '<' && rest[1..].starts_with(|c: char| c.is_ascii_uppercase()) {
                let block = line_start_before(src, i) && {
                    let used = self.tag_len(rest, line_at(i))?;
                    src[i + used..].split('\n').next().unwrap_or("").trim().is_empty()
                };
                let (used, html) = self.component(rest, line_at(i), depth, !block)?;
                let markdown = crate::twin::html_to_markdown(&html).trim().to_string();
                let n = self.parts.len();
                self.parts.push(html);
                if block {
                    out.push_str(&format!("\n\n{}\n\n", token(n)));
                    twin.push_str(&format!("\n\n{markdown}\n\n"));
                } else {
                    out.push_str(&token(n));
                    twin.push_str(markdown.trim_start_matches("# ").lines().next().unwrap_or(""));
                }
                i += used;
                continue;
            }
            out.push(c);
            twin.push(c);
            i += c.len_utf8();
        }
        let tidy = |s: String| {
            let mut s = s;
            while s.contains("\n\n\n") {
                s = s.replace("\n\n\n", "\n\n");
            }
            s.trim().to_string() + "\n"
        };
        Ok((tidy(out), tidy(twin)))
    }

    /// The length of a whole component, from `<Name` through its closing
    /// tag or `/>`.
    fn tag_len(&self, src: &str, line: usize) -> Result<usize> {
        let (name, _, open_len, closed) = self.open_tag(src, line)?;
        if closed {
            return Ok(open_len);
        }
        let (_, close_at) = self.matching_close(src, &name, open_len, line)?;
        Ok(close_at + name.len() + 3)
    }

    /// Parses `<Name a="x" b={2} c>`: the name, the props, the length of
    /// the tag, and whether it closes itself.
    fn open_tag(&self, src: &str, line: usize) -> Result<(String, Map<String, Value>, usize, bool)> {
        let name_len = src[1..].find(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '_')).unwrap_or(src.len() - 1);
        let name = src[1..1 + name_len].to_string();
        if name.contains('.') {
            bail!("{}: <{name}> is a member expression; name the component after its file in components/", self.at(line));
        }
        let mut props = Map::new();
        let mut i = 1 + name_len;
        loop {
            let rest = &src[i..];
            let skipped = rest.len() - rest.trim_start().len();
            i += skipped;
            let rest = &src[i..];
            if rest.starts_with("/>") {
                return Ok((name, props, i + 2, true));
            }
            if rest.starts_with('>') {
                return Ok((name, props, i + 1, false));
            }
            if rest.is_empty() {
                bail!("{}: <{name}> is never closed\nhint: end the tag with > or />", self.at(line));
            }
            let key_len = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ':')).unwrap_or(rest.len());
            if key_len == 0 {
                bail!("{}: <{name}> has something other than a prop at {:?}", self.at(line), rest.chars().take(12).collect::<String>());
            }
            let key = rest[..key_len].to_string();
            i += key_len;
            if !src[i..].starts_with('=') {
                props.insert(key, Value::Bool(true));
                continue;
            }
            i += 1;
            let rest = &src[i..];
            let (value, used) = match rest.chars().next() {
                Some(q @ ('"' | '\'')) => {
                    let end = rest[1..].find(q).ok_or_else(|| anyhow::anyhow!("{}: prop {key} has no closing quote", self.at(line)))?;
                    (Value::from(&rest[1..1 + end]), end + 2)
                }
                Some('{') => {
                    let end = braced(rest).ok_or_else(|| anyhow::anyhow!("{}: prop {key} has no closing }}", self.at(line)))?;
                    let inner = rest[1..end].trim();
                    let value = literal(inner).ok_or_else(|| {
                        anyhow::anyhow!(
                            "{}: prop {key}={{{inner}}} is a JavaScript expression, which Mira does not run\nhint: pass a string, number, true, false, or JSON",
                            self.at(line)
                        )
                    })?;
                    (value, end + 1)
                }
                _ => bail!("{}: prop {key} needs a value in quotes or braces", self.at(line)),
            };
            props.insert(key, value);
            i += used;
        }
    }

    /// Finds `</Name>` for a tag opened at the start of `src`, counting
    /// nested tags of the same name. Returns where the children start and
    /// where the closing tag starts.
    fn matching_close(&self, src: &str, name: &str, from: usize, line: usize) -> Result<(usize, usize)> {
        let open = format!("<{name}");
        let close = format!("</{name}>");
        let mut depth = 1;
        let mut i = from;
        loop {
            let next_open = src[i..].find(&open).map(|n| i + n);
            let next_close = src[i..].find(&close).map(|n| i + n);
            match (next_open, next_close) {
                (_, None) => {
                    bail!("{}: <{name}> is never closed\nhint: add </{name}> where it ends, or close the tag with />", self.at(line))
                }
                (Some(o), Some(c)) if o < c => {
                    let after = src[o + open.len()..].chars().next().unwrap_or(' ');
                    if after.is_whitespace() || after == '>' || after == '/' {
                        let tag_end = src[o..].find('>').map_or(src.len(), |n| o + n);
                        if !src[..tag_end].ends_with('/') {
                            depth += 1;
                        }
                    }
                    i = o + open.len();
                }
                (_, Some(c)) => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok((from, c));
                    }
                    i = c + close.len();
                }
            }
        }
    }

    /// Renders one component and returns how much of `src` it used.
    fn component(&mut self, src: &str, line: usize, depth: usize, inline: bool) -> Result<(usize, String)> {
        let (name, props, open_len, closed) = self.open_tag(src, line)?;
        let (used, children) = if closed {
            (open_len, String::new())
        } else {
            let (start, end) = self.matching_close(src, &name, open_len, line)?;
            (end + name.len() + 3, src[start..end].to_string())
        };
        let Some((file, component)) = self.components.get(&name) else {
            let known: Vec<&str> = self.components.keys().map(String::as_str).collect();
            bail!(
                "{}: no component named {name}\nhint: create components/{name}.mira{}",
                self.at(line),
                if known.is_empty() { String::new() } else { format!(", or use one of: {}", known.join(", ")) }
            );
        };
        let children_html = if children.trim().is_empty() {
            String::new()
        } else {
            let (markdown, _) = self.expand(&children, line, depth + 1)?;
            let html = self.finish(render_markdown(&markdown).html);
            match (inline, html.trim().strip_prefix("<p>").and_then(|h| h.strip_suffix("</p>"))) {
                (true, Some(inner)) if !inner.contains("<p>") => inner.to_string(),
                _ => html,
            }
        };
        if let Some(style) = &component.style
            && !self.css.iter().any(|c| c.contains(&format!("/*{}*/", file.display())))
        {
            self.css.push(format!("/*{}*/{}", file.display(), crate::assets::compact_css(style)));
        }
        let html = component.template.render(&json!({ "props": props }), &children_html);
        Ok((used, html.trim().to_string()))
    }

    /// Puts each rendered component where its token stands.
    fn finish(&self, mut html: String) -> String {
        for (n, part) in self.parts.iter().enumerate().rev() {
            let t = token(n);
            html = html.replace(&format!("<p>{t}</p>"), part).replace(&t, part);
        }
        html
    }
}

/// Whether only whitespace sits between the last line break and `i`.
fn line_start_before(src: &str, i: usize) -> bool {
    src[..i].rsplit('\n').next().unwrap_or("").trim().is_empty()
}

/// The index of the brace closing the one `src` starts with, skipping
/// braces inside strings.
fn braced(src: &str) -> Option<usize> {
    let mut depth = 0;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (i, c) in src.char_indices() {
        match quote {
            Some(q) => {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' | '`' => quote = Some(c),
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            },
        }
    }
    None
}

/// A prop value that needs no code to evaluate: JSON, or a string in single
/// quotes or backticks without interpolation.
fn literal(src: &str) -> Option<Value> {
    if let Ok(value) = serde_json::from_str::<Value>(src) {
        return Some(value);
    }
    for q in ['\'', '`'] {
        if let Some(inner) = src.strip_prefix(q).and_then(|s| s.strip_suffix(q))
            && !inner.contains(q)
            && !inner.contains("${")
        {
            return Some(Value::from(inner));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn components() -> Components {
        let mut c = HashMap::new();
        let callout = "<template><aside class=\"callout\" data-tone=\"{{ props.tone }}\"><strong>{{ props.title }}</strong><slot /></aside></template>\n<style>.callout { padding: 1rem }</style>";
        c.insert(
            "Callout".to_string(),
            (PathBuf::from("components/Callout.mira"), parse_component(callout, Path::new("Callout.mira")).unwrap()),
        );
        let badge = "<template><span class=\"badge\">{{ props.label }}</span></template>";
        c.insert("Badge".to_string(), (PathBuf::from("components/Badge.mira"), parse_component(badge, Path::new("Badge.mira")).unwrap()));
        c
    }

    #[test]
    fn renders_components_with_markdown_children() {
        let src = "import { Callout } from '../components'\n\n# Hours\n\n<Callout tone=\"warn\" title={\"Closed Monday\"}>\n  We are **closed** on Mondays.\n\n  <Badge label='New' />\n</Callout>\n\nOpen at 9 <Badge label=\"daily\" /> except holidays. {/* a note */}\n\n```text\n<Callout>{code}</Callout>\n```\n";
        let r = render(src, Path::new("content/a.mdx"), 1, &components()).unwrap();
        assert!(r.markdown.html.contains("<aside class=\"callout\" data-tone=\"warn\"><strong>Closed Monday</strong><p>We are <strong>closed</strong> on Mondays.</p>"), "{}", r.markdown.html);
        assert!(r.markdown.html.contains("<span class=\"badge\">New</span>"), "{}", r.markdown.html);
        assert!(r.markdown.html.contains("<p>Open at 9 <span class=\"badge\">daily</span> except holidays.</p>"), "{}", r.markdown.html);
        assert!(r.markdown.html.contains("&lt;Callout&gt;{code}&lt;/Callout&gt;"), "code stays code: {}", r.markdown.html);
        assert!(
            !r.markdown.html.contains("import") && !r.markdown.html.contains("a note") && !r.markdown.html.contains("MIRAMDX"),
            "{}",
            r.markdown.html
        );
        assert!(r.css.contains(".callout"), "{}", r.css);
        assert_eq!(r.markdown.toc.len(), 0);
        assert!(r.twin.contains("**Closed Monday**") && r.twin.contains("Open at 9 daily except"), "{}", r.twin);
        assert!(r.twin.contains("```text\n<Callout>{code}</Callout>\n```"), "{}", r.twin);
    }

    #[test]
    fn refuses_javascript_with_the_line() {
        let err = |src: &str| render(src, Path::new("a.mdx"), 3, &components()).err().unwrap().to_string();
        assert!(err("Hi\n\nexport const x = 1\n").starts_with("a.mdx:5: MDX exports run JavaScript"));
        assert!(err("Total: {price * 2}\n").starts_with("a.mdx:3: { starts a JavaScript expression"));
        assert!(err("<Badge label={user.name} />\n").contains("prop label={user.name} is a JavaScript expression"));
        assert!(err("<Chart />\n").contains("no component named Chart\nhint: create components/Chart.mira, or use one of"));
        assert!(err("<Callout>\nopen\n").contains("<Callout> is never closed"));
        assert_eq!(
            render("A \\{literal\\} brace\n", Path::new("a.mdx"), 1, &components()).unwrap().markdown.html,
            "<p>A {literal} brace</p>\n"
        );
    }
}
