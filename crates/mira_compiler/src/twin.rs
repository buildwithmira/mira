//! Markdown twins: the agent readable version of every page.
//!
//! Pages authored in Markdown reuse their source. Component pages are
//! converted from the rendered `<main>` element, which covers the common
//! content tags and drops chrome such as navigation and scripts.

/// Converts the `<main>` element of `html` (or the whole input when there
/// is none) to Markdown.
pub fn html_to_markdown(html: &str) -> String {
    let html = main_element(html).unwrap_or(html);
    let mut c = Converter::default();
    let mut rest = html;
    while !rest.is_empty() {
        match rest.find('<') {
            Some(0) => {
                let Some(end) = rest.find('>') else { break };
                c.tag(&rest[1..end]);
                rest = &rest[end + 1..];
            }
            Some(i) => {
                c.text(&rest[..i]);
                rest = &rest[i..];
            }
            None => {
                c.text(rest);
                rest = "";
            }
        }
    }
    c.finish()
}

fn main_element(html: &str) -> Option<&str> {
    let start = html.find("<main")?;
    let open_end = start + html[start..].find('>')? + 1;
    let end = html.rfind("</main>")?;
    (end >= open_end).then(|| &html[open_end..end])
}

#[derive(Default)]
struct Converter {
    out: String,
    /// Inline text for the block being built.
    line: String,
    lists: Vec<(bool, usize)>,
    links: Vec<String>,
    skip: usize,
    pre: bool,
    pre_lang: String,
    link_has_blocks: bool,
    quote: usize,
}

impl Converter {
    fn tag(&mut self, raw: &str) {
        let closing = raw.starts_with('/');
        let raw = raw.trim_start_matches('/').trim_end_matches('/');
        let name_end = raw.find(|c: char| c.is_whitespace()).unwrap_or(raw.len());
        let name = raw[..name_end].to_ascii_lowercase();
        let attrs = &raw[name_end..];

        if matches!(name.as_str(), "script" | "style" | "nav" | "template" | "svg" | "button" | "form") {
            if closing {
                self.skip = self.skip.saturating_sub(1)
            } else {
                self.skip += 1
            }
            return;
        }
        if self.skip > 0 {
            return;
        }
        match (name.as_str(), closing) {
            ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", false) if !self.links.is_empty() => self.block(),
            ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", false) => {
                self.block();
                let level = name[1..].parse::<usize>().unwrap_or(1);
                self.line.push_str(&"#".repeat(level));
                self.line.push(' ');
            }
            (
                "p" | "div" | "section" | "article" | "header" | "footer" | "figure" | "table" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5"
                | "h6",
                _,
            ) => self.block(),
            ("figcaption", false) => self.block(),
            ("td" | "th", false) => self.line.push_str(" | "),
            ("br", _) => self.line.push_str("  \n"),
            ("hr", _) => {
                self.block();
                self.out.push_str("---\n\n");
            }
            ("strong" | "b", _) => self.line.push_str("**"),
            ("em" | "i", _) => self.line.push('*'),
            ("code", false) if self.pre => {
                if let Some(lang) = language(attrs) {
                    self.pre_lang = lang;
                }
            }
            ("code", _) if !self.pre => self.line.push('`'),
            ("pre", false) => {
                self.block();
                self.pre = true;
                self.pre_lang = language(attrs).unwrap_or_default();
            }
            ("pre", true) => {
                self.pre = false;
                let code = std::mem::take(&mut self.line);
                self.out.push_str(&format!("```{}\n", std::mem::take(&mut self.pre_lang)));
                self.out.push_str(code.trim_end_matches('\n'));
                self.out.push_str("\n```\n\n");
            }
            ("blockquote", false) => {
                self.block();
                self.quote += 1;
            }
            ("blockquote", true) => {
                self.block();
                self.quote = self.quote.saturating_sub(1);
            }
            ("ul" | "ol", false) => {
                self.block();
                self.lists.push((name == "ol", 0));
            }
            ("ul" | "ol", true) => {
                self.block();
                self.lists.pop();
                if self.lists.is_empty() {
                    self.out.push('\n');
                }
            }
            ("li", false) => {
                self.flush_line(false);
                let depth = self.lists.len().saturating_sub(1);
                let marker = match self.lists.last_mut() {
                    Some((true, n)) => {
                        *n += 1;
                        format!("{n}. ")
                    }
                    _ => "- ".into(),
                };
                self.line.push_str(&"  ".repeat(depth));
                self.line.push_str(&marker);
            }
            ("li", true) => self.flush_line(false),
            ("a", false) => {
                self.links.push(attr(attrs, "href").unwrap_or_default());
                self.line.push('[');
            }
            ("a", true) => {
                let href = self.links.pop().unwrap_or_default();
                self.line.truncate(self.line.trim_end().len());
                self.line.push_str(&format!("]({href})"));
                // A link wrapping blocks (a card) stands as its own paragraph.
                if self.links.is_empty() && std::mem::take(&mut self.link_has_blocks) {
                    self.block();
                }
            }
            ("img", _) => {
                let alt = attr(attrs, "alt").unwrap_or_default();
                let src = attr(attrs, "src").unwrap_or_default();
                self.line.push_str(&format!("![{alt}]({src})"));
            }
            _ => {}
        }
    }

    fn text(&mut self, raw: &str) {
        if self.skip > 0 {
            return;
        }
        let text = decode_entities(raw);
        if self.pre {
            self.line.push_str(&text);
            return;
        }
        let mut last_space = self.line.is_empty() || self.line.ends_with([' ', '\n', '[']);
        for c in text.chars() {
            if c.is_whitespace() {
                if !last_space {
                    self.line.push(' ');
                    last_space = true;
                }
            } else {
                self.line.push(c);
                last_space = false;
            }
        }
    }

    fn flush_line(&mut self, paragraph: bool) {
        let line = std::mem::take(&mut self.line);
        let line = line.trim_end();
        let content = line.trim_start_matches(['-', ' ', '#']).trim_start_matches(|c: char| c.is_ascii_digit() || c == '.');
        if content.trim().is_empty() {
            return;
        }
        let prefix = "> ".repeat(self.quote);
        for l in line.lines() {
            self.out.push_str(&prefix);
            self.out.push_str(l.trim_end());
            self.out.push('\n');
        }
        if paragraph {
            self.out.push('\n');
        }
    }

    fn block(&mut self) {
        // Blocks nested in a link stay on the link's line.
        if !self.links.is_empty() {
            self.link_has_blocks = true;
            if !self.line.ends_with([' ', '[']) {
                self.line.push(' ');
            }
            return;
        }
        let in_list = !self.lists.is_empty();
        self.flush_line(!in_list);
    }

    fn finish(mut self) -> String {
        self.block();
        let mut out = self.out.trim().to_string();
        while out.contains("\n\n\n") {
            out = out.replace("\n\n\n", "\n\n");
        }
        out.push('\n');
        out
    }
}

fn language(attrs: &str) -> Option<String> {
    if let Some(lang) = attr(attrs, "data-lang").filter(|l| !l.is_empty()) {
        return Some(lang);
    }
    attr(attrs, "class")?.split_whitespace().find_map(|w| w.strip_prefix("language-")).map(str::to_string)
}

fn attr(attrs: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let mut search = attrs;
    loop {
        let i = search.find(&needle)?;
        let preceded = i == 0 || search[..i].ends_with(char::is_whitespace);
        let rest = &search[i + needle.len()..];
        if preceded {
            return rest.find('"').map(|end| decode_entities(&rest[..end]));
        }
        search = rest;
    }
}

pub fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';').filter(|&e| e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            "middot" => Some('·'),
            "larr" => Some('←'),
            "rarr" => Some('→'),
            "hellip" => Some('…'),
            "mdash" => Some('—'),
            "ndash" => Some('–'),
            _ => entity
                .strip_prefix("#x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
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
    fn converts_main_content() {
        let html = r#"<header><nav><a href="/">Home</a></nav></header>
<main id="main"><h1>Writing</h1>
<ol class="posts"><li><a href="/posts/a/"><h2>First &amp; best</h2><p>Intro</p></a></li></ol>
<pre><code class="language-rs">let x = 1 &lt; 2;
</code></pre><p>Some <strong>bold</strong> and <code>code</code>.</p></main>"#;
        let md = html_to_markdown(html);
        assert!(md.starts_with("# Writing\n"), "{md}");
        assert!(md.contains("First & best"), "{md}");
        assert!(md.contains("1. [First & best Intro](/posts/a/)"), "{md}");
        assert!(md.contains("```rs\nlet x = 1 < 2;\n```"), "{md}");
        assert!(md.contains("Some **bold** and `code`."), "{md}");
        assert!(!md.contains("Home"), "{md}");
    }

    #[test]
    fn decodes_entities() {
        assert_eq!(decode_entities("a &amp; b &#8212; &#x2192; &bogus; &"), "a & b — → &bogus; &");
    }
}
