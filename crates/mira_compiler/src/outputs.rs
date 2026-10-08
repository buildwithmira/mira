//! Site level files generated from every page: sitemap, feeds, llms.txt,
//! robots.txt, and host headers.

use crate::config::Config;
use crate::html::escape;

/// What site level outputs need to know about a rendered page.
pub struct PageMeta {
    pub url: String,
    pub title: String,
    pub description: Option<String>,
    pub date: Option<String>,
    pub collection: Option<String>,
    pub twin: Option<String>,
    pub not_found: bool,
    /// `robots: noindex` in frontmatter keeps a page out of the sitemap,
    /// llms.txt, and search.
    pub noindex: bool,
    /// `updated`, else `date`, for sitemap lastmod.
    pub lastmod: Option<String>,
    /// The page's own description, without the site fallback.
    pub own_description: Option<String>,
    /// The full `<title>`, including the site name.
    pub document_title: String,
}

impl PageMeta {
    pub fn listed(&self) -> bool {
        !self.not_found && !self.noindex
    }
}

/// Crawlers that fetch pages to answer a question and cite them.
pub const ANSWER_BOTS: [&str; 8] = [
    "OAI-SearchBot",
    "ChatGPT-User",
    "Claude-SearchBot",
    "Claude-User",
    "PerplexityBot",
    "Perplexity-User",
    "DuckAssistBot",
    "MistralAI-User",
];

/// Crawlers that collect pages to train AI models.
pub const TRAINING_BOTS: [&str; 8] = [
    "GPTBot",
    "ClaudeBot",
    "Google-Extended",
    "Applebot-Extended",
    "CCBot",
    "Bytespider",
    "meta-externalagent",
    "cohere-training-data-crawler",
];

/// `/` → `/index.md`, `/posts/a/` → `/posts/a.md`.
pub fn twin_url(url: &str) -> String {
    match url.trim_end_matches('/') {
        "" => "/index.md".into(),
        path => format!("{path}.md"),
    }
}

pub fn absolute(config: &Config, path: &str) -> Option<String> {
    config.site.url.as_ref().map(|origin| format!("{}{path}", origin.trim_end_matches('/')))
}

pub fn sitemap(config: &Config, pages: &[PageMeta]) -> Option<String> {
    config.site.url.as_ref()?;
    let mut xml =
        String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    for page in pages.iter().filter(|p| p.listed()) {
        xml.push_str("  <url><loc>");
        xml.push_str(&escape(&absolute(config, &page.url)?));
        xml.push_str("</loc>");
        if let Some(date) = page.lastmod.as_deref().filter(|d| is_iso_date(d)) {
            xml.push_str(&format!("<lastmod>{}</lastmod>", &date[..10]));
        }
        xml.push_str("</url>\n");
    }
    xml.push_str("</urlset>\n");
    Some(xml)
}

/// An RSS 2.0 feed of one collection, newest first.
pub fn rss(config: &Config, collection: &str, feed_path: &str, pages: &[PageMeta]) -> Option<String> {
    let site_url = absolute(config, "/")?;
    let mut items: Vec<&PageMeta> = pages.iter().filter(|p| p.listed() && p.collection.as_deref() == Some(collection)).collect();
    items.sort_by(|a, b| b.date.cmp(&a.date));
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rss version=\"2.0\" xmlns:atom=\"http://www.w3.org/2005/Atom\">\n<channel>\n",
    );
    xml.push_str(&format!("  <title>{} · {}</title>\n", escape(&config.site.title), escape(collection)));
    xml.push_str(&format!("  <link>{}</link>\n", escape(&site_url)));
    xml.push_str(&format!("  <description>{}</description>\n", escape(config.site.description.as_deref().unwrap_or(&config.site.title))));
    xml.push_str(&format!("  <atom:link href=\"{}\" rel=\"self\" type=\"application/rss+xml\"/>\n", escape(&absolute(config, feed_path)?)));
    for item in items {
        let link = escape(&absolute(config, &item.url)?);
        xml.push_str("  <item>\n");
        xml.push_str(&format!("    <title>{}</title>\n    <link>{link}</link>\n    <guid>{link}</guid>\n", escape(&item.title)));
        if let Some(d) = &item.description {
            xml.push_str(&format!("    <description>{}</description>\n", escape(d)));
        }
        if let Some(date) = item.date.as_deref().and_then(rfc822) {
            xml.push_str(&format!("    <pubDate>{date}</pubDate>\n"));
        }
        xml.push_str("  </item>\n");
    }
    xml.push_str("</channel>\n</rss>\n");
    Some(xml)
}

/// Approximate tokens in `text`, at four characters per token: close
/// enough to choose between fetching a page and fetching a section.
pub fn tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

/// The llms.txt index (https://llmstxt.org), written as a router: every
/// page with a one line description and its approximate size, so an agent
/// fetches only what it needs. `structured` lists the JSON files for
/// collections and data, as `(path, description)`.
pub fn llms_txt(config: &Config, pages: &[PageMeta], has_media: bool, full_tokens: usize, structured: &[(String, String)]) -> String {
    let mut out = format!("# {}\n\n", config.site.title);
    if let Some(d) = &config.site.description {
        out.push_str(&format!("> {d}\n\n"));
    }
    let at = |path: &str| absolute(config, path).unwrap_or_else(|| path.to_string());
    out.push_str(&format!(
        "Each link below is one page as Markdown, with its approximate size in tokens. Fetch the pages you need, or [llms-full.txt]({}) for every page in one file (~{full_tokens} tokens). Structured content is listed under Data.\n\n",
        at("/llms-full.txt")
    ));
    let link = |p: &PageMeta| {
        let target = at(&if p.twin.is_some() { twin_url(&p.url) } else { p.url.clone() });
        let size = p.twin.as_deref().map(|t| format!(" (~{} tokens)", tokens(t))).unwrap_or_default();
        match &p.description {
            Some(d) => format!("- [{}]({target}): {d}{size}\n", p.title),
            None => format!("- [{}]({target}){size}\n", p.title),
        }
    };
    out.push_str("## Pages\n\n");
    for page in pages.iter().filter(|p| p.collection.is_none() && p.listed()) {
        out.push_str(&link(page));
    }
    let mut collections: Vec<&str> = Vec::new();
    for name in pages.iter().filter(|p| p.listed()).filter_map(|p| p.collection.as_deref()) {
        if !collections.contains(&name) {
            collections.push(name);
        }
    }
    for name in collections {
        out.push_str(&format!("\n## {}\n\n", capitalize(name)));
        for page in pages.iter().filter(|p| p.listed() && p.collection.as_deref() == Some(name)) {
            out.push_str(&link(page));
        }
    }
    out.push_str("\n## Data\n\n");
    for (path, description) in structured {
        out.push_str(&format!("- [{path}]({}): {description}\n", at(path)));
    }
    out.push_str(&format!("- [Search index]({}): every page's title, headings, and opening text as JSON\n", at("/_mira/search.json")));
    if has_media {
        out.push_str(&format!("- [Media manifest]({}): every image and video with its size, alt text, and caption\n", at("/media.json")));
    }
    if config.agents.content {
        out.push_str(&format!(
            "- [Content index]({}): the collections and data files above, with their field types\n",
            at("/_mira/content.json")
        ));
    }
    if !config.actions.is_empty() {
        out.push_str(&format!(
            "\n## Actions\n\nEach action takes a JSON `POST` with the fields listed; a field marked ? is optional. Show the person you act for exactly what you will send, and send only once they agree. Schemas: [actions.json]({})\n\n",
            at("/_mira/actions.json")
        ));
        for (name, action) in &config.actions {
            let fields: Vec<String> = action.input.iter().map(|(f, t)| format!("{f} ({t})")).collect();
            out.push_str(&format!("- {name}: {} `POST {}`", action.description, action.endpoint));
            if !fields.is_empty() {
                out.push_str(&format!(" with {}", fields.join(", ")));
            }
            out.push('\n');
        }
    }
    let site = config.site.url.as_deref().map_or_else(|| "https://example.com".to_string(), |u| u.trim_end_matches('/').to_string());
    out.push_str(&format!(
        "\n## MCP\n\nAny MCP client can search this site, read pages or single sections, and query its collections:\n\n```\nnpx -y @miraframework/mira mcp --url {site}\n```\n"
    ));
    out
}

/// Every Markdown twin in one file, for agents that want the whole site.
pub fn llms_full_txt(config: &Config, pages: &[PageMeta]) -> String {
    let mut out = format!("# {}\n\n", config.site.title);
    for page in pages.iter().filter(|p| p.listed()) {
        if let Some(twin) = &page.twin {
            out.push_str("---\n\n");
            out.push_str(twin);
            out.push('\n');
        }
    }
    out
}

/// robots.txt from three switches (search, AI answers, AI training) plus
/// any per agent rules, which win. The catch all group carries a
/// Content-Signal line stating the same choices for crawlers that read it.
pub fn robots_txt(config: &Config, has_sitemap: bool) -> String {
    let agents = &config.agents;
    let mut rules: Vec<(String, bool)> = Vec::new();
    let mut add = |agent: &str, allow: bool| {
        if !rules.iter().any(|(a, _)| a.eq_ignore_ascii_case(agent)) {
            rules.push((agent.to_string(), allow));
        }
    };
    for (agent, rule) in &agents.robots {
        if agent != "*" {
            add(agent, rule == "allow");
        }
    }
    if agents.answers == "disallow" {
        for bot in ANSWER_BOTS {
            add(bot, false);
        }
    }
    if agents.training == "disallow" {
        for bot in TRAINING_BOTS {
            add(bot, false);
        }
    }
    let catch_all = match agents.robots.get("*") {
        Some(rule) => rule == "allow",
        None => agents.search == "allow",
    };

    let mut out = String::new();
    for (agent, allow) in &rules {
        out.push_str(&format!("User-agent: {agent}\n{}\n\n", if *allow { "Allow: /" } else { "Disallow: /" }));
    }
    let yes = |v: &str| if v == "allow" { "yes" } else { "no" };
    out.push_str(&format!(
        "User-agent: *\nContent-Signal: search={}, ai-input={}, ai-train={}\n{}\n\n",
        yes(&agents.search),
        yes(&agents.answers),
        yes(&agents.training),
        if catch_all { "Allow: /" } else { "Disallow: /" }
    ));
    if has_sitemap && let Some(url) = absolute(config, "/sitemap.xml") {
        out.push_str(&format!("Sitemap: {url}\n"));
    }
    out
}

/// A `_headers` file (Netlify and Cloudflare Pages syntax). Per page CSP
/// lives in each page's meta tag because it carries per page hashes; the
/// header adds the directives a meta tag cannot express.
pub fn headers_file(config: &Config) -> String {
    let mut out = String::from("/*\n");
    for (name, value) in security_headers(config) {
        out.push_str(&format!("  {name}: {value}\n"));
    }
    out.push_str(&format!("\n/*.md\n  Content-Type: {MARKDOWN}\n"));
    for dir in IMMUTABLE_DIRS {
        out.push_str(&format!("\n/{dir}/*\n  Cache-Control: {IMMUTABLE}\n"));
    }
    out
}

pub(crate) const MARKDOWN: &str = "text/markdown; charset=utf-8";
pub(crate) const IMMUTABLE: &str = "public, max-age=31536000, immutable";
/// Folders whose files carry a content hash or never change in place.
pub(crate) const IMMUTABLE_DIRS: [&str; 2] = ["fonts", "media"];

/// Headers every response gets. The per page CSP lives in each page's meta
/// tag; this adds the directives a meta tag cannot express.
pub(crate) fn security_headers(config: &Config) -> Vec<(&'static str, &'static str)> {
    let mut headers = vec![
        ("Content-Security-Policy", "frame-ancestors 'none'; object-src 'none'; base-uri 'self'"),
        ("X-Content-Type-Options", "nosniff"),
        ("X-Frame-Options", "DENY"),
        ("Referrer-Policy", "strict-origin-when-cross-origin"),
        ("Cross-Origin-Opener-Policy", "same-origin"),
        ("Cross-Origin-Resource-Policy", "same-origin"),
        ("Permissions-Policy", "camera=(), microphone=(), geolocation=(), payment=(), usb=(), interest-cohort=()"),
    ];
    if config.headers.hsts {
        headers.push(("Strict-Transport-Security", "max-age=63072000; includeSubDomains"));
    }
    headers
}

fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 10 && b[4] == b'-' && b[7] == b'-' && b[..4].iter().chain(&b[5..7]).chain(&b[8..10]).all(u8::is_ascii_digit)
}

/// `2026-10-06` → `Tue, 06 Oct 2026 00:00:00 GMT`.
fn rfc822(date: &str) -> Option<String> {
    if !is_iso_date(date) {
        return None;
    }
    let (y, m, d): (i64, usize, i64) = (date[..4].parse().ok()?, date[5..7].parse().ok()?, date[8..10].parse().ok()?);
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let month = MONTHS.get(m.checked_sub(1)?)?;
    // Sakamoto's day of week algorithm.
    let t = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let yy = if m < 3 { y - 1 } else { y };
    let dow = (yy + yy / 4 - yy / 100 + yy / 400 + t[m - 1] + d).rem_euclid(7) as usize;
    Some(format!("{}, {d:02} {month} {y} 00:00:00 GMT", DAYS[dow]))
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twin_urls() {
        assert_eq!(twin_url("/"), "/index.md");
        assert_eq!(twin_url("/posts/a/"), "/posts/a.md");
    }

    #[test]
    fn llms_txt_routes_with_sizes() {
        let page = |url: &str, collection: Option<&str>| PageMeta {
            url: url.into(),
            title: "Hours".into(),
            description: Some("When we are open.".into()),
            date: None,
            collection: collection.map(str::to_string),
            twin: Some("x".repeat(400)),
            not_found: false,
            noindex: false,
            lastmod: None,
            own_description: None,
            document_title: String::new(),
        };
        let structured = [("/_mira/collections/menu.json".to_string(), "menu, 12 entries with name, price".to_string())];
        let txt = llms_txt(&Config::default(), &[page("/hours/", None), page("/posts/a/", Some("posts"))], false, 900, &structured);
        assert!(txt.contains("- [Hours](/hours.md): When we are open. (~100 tokens)"), "{txt}");
        assert!(txt.contains("(~900 tokens)"), "{txt}");
        assert!(txt.contains("## Posts\n\n- [Hours](/posts/a.md)"), "{txt}");
        assert!(txt.contains("- [/_mira/collections/menu.json](/_mira/collections/menu.json): menu, 12 entries"), "{txt}");
    }

    #[test]
    fn formats_rfc822() {
        assert_eq!(rfc822("2026-10-06").as_deref(), Some("Tue, 06 Oct 2026 00:00:00 GMT"));
        assert_eq!(rfc822("2024-02-29").as_deref(), Some("Thu, 29 Feb 2024 00:00:00 GMT"));
        assert_eq!(rfc822("soon"), None);
    }

    #[test]
    fn robots_puts_catch_all_last() {
        let mut config = Config::default();
        config.agents.robots.insert("ExampleBot".into(), "disallow".into());
        let txt = robots_txt(&config, false);
        assert!(txt.starts_with("User-agent: ExampleBot\nDisallow: /"), "{txt}");
        assert!(txt.contains("User-agent: *\nContent-Signal: search=yes, ai-input=yes, ai-train=yes\nAllow: /"), "{txt}");
    }

    #[test]
    fn robots_blocks_training_but_not_answers() {
        let mut config = Config::default();
        config.agents.training = "disallow".into();
        config.agents.robots.insert("CCBot".into(), "allow".into());
        let txt = robots_txt(&config, false);
        assert!(txt.contains("User-agent: GPTBot\nDisallow: /"), "{txt}");
        assert!(txt.contains("User-agent: CCBot\nAllow: /"), "{txt}");
        assert!(!txt.contains("OAI-SearchBot"), "{txt}");
        assert!(txt.contains("ai-train=no"), "{txt}");
    }
}
