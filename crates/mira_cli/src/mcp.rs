//! `mira mcp`: a Model Context Protocol server over stdio that gives agents
//! every part of a Mira site: its pages as Markdown, search, collection
//! entries with their fields, data files, and media.
//!
//! It reads a local project, rebuilt before each call so answers match the
//! source, or, with `--url`, any deployed Mira site through the files every
//! build publishes under `/_mira/`. Both modes answer the same way.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use mira_compiler::{BuildOptions, build};
use serde_json::{Value, json};

const PROTOCOL: &str = "2025-06-18";
/// The largest file the server reads, from disk or the network.
const MAX_BYTES: u64 = 16 * 1024 * 1024;
/// How long a deployed site's indexes are reused before fetching again.
const FRESH: Duration = Duration::from_secs(30);

/// Where a site's files come from.
pub enum Source {
    /// A project on disk, rebuilt into `.mira/mcp` before each call.
    Project { root: PathBuf, out: PathBuf },
    /// A deployed site, read over HTTPS.
    Site { base: String, agent: ureq::Agent, cache: RefCell<HashMap<String, (Instant, Option<String>)>> },
}

impl Source {
    pub fn project(root: &Path) -> Result<Source> {
        let root = std::path::absolute(root)?;
        let out = root.join(".mira").join("mcp");
        Ok(Source::Project { root, out })
    }

    /// A deployed Mira site at `url`, such as `https://example.com` or a
    /// site under a path like `https://example.github.io/docs`.
    pub fn site(url: &str) -> Result<Source> {
        let base = site_base(url)?;
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .max_redirects(0)
            .http_status_as_error(false)
            .user_agent(concat!("mira-mcp/", env!("CARGO_PKG_VERSION")))
            .tls_config(tls())
            .build();
        Ok(Source::Site { base, agent: config.into(), cache: RefCell::new(HashMap::new()) })
    }

    fn describe(&self) -> String {
        match self {
            Source::Project { root, .. } => root.display().to_string(),
            Source::Site { base, .. } => base.clone(),
        }
    }

    /// Brings a project's output up to date. A deployed site is already built.
    fn refresh(&self) -> Result<()> {
        if let Source::Project { root, out } = self {
            build(&BuildOptions { root: root.clone(), out: out.clone(), dev: false, host_config: false })?;
        }
        Ok(())
    }

    /// Reads a file the build publishes, such as `/_mira/search.json`.
    /// `None` means the site has no such file.
    fn get(&self, path: &str) -> Result<Option<String>> {
        debug_assert!(path.starts_with('/'));
        match self {
            Source::Project { out, .. } => {
                let file = path.trim_start_matches('/').split('/').fold(out.clone(), |p, part| p.join(part));
                match std::fs::metadata(&file) {
                    Ok(meta) if meta.is_file() => {
                        if meta.len() > MAX_BYTES {
                            bail!("{path} is larger than {} MB", MAX_BYTES / 1024 / 1024);
                        }
                        Ok(Some(std::fs::read_to_string(&file)?))
                    }
                    _ => Ok(None),
                }
            }
            Source::Site { base, agent, cache } => {
                let reuse = path.starts_with("/_mira/") || path == "/media.json";
                if reuse
                    && let Some((at, body)) = cache.borrow().get(path)
                    && at.elapsed() < FRESH
                {
                    return Ok(body.clone());
                }
                let url = format!("{base}{path}");
                let mut response = agent.get(&url).call().map_err(|e| anyhow!("could not reach {url}: {e}"))?;
                let status = response.status().as_u16();
                let body = match status {
                    200..=299 => {
                        let mut text = String::new();
                        response
                            .body_mut()
                            .as_reader()
                            .take(MAX_BYTES + 1)
                            .read_to_string(&mut text)
                            .map_err(|e| anyhow!("could not read {url}: {e}"))?;
                        if text.len() as u64 > MAX_BYTES {
                            bail!("{url} is larger than {} MB", MAX_BYTES / 1024 / 1024);
                        }
                        Some(text)
                    }
                    404 | 410 => None,
                    300..=399 => bail!("{url} redirected; pass the site's final address to --url"),
                    _ => bail!("{url} returned HTTP {status}"),
                };
                if reuse {
                    cache.borrow_mut().insert(path.to_string(), (Instant::now(), body.clone()));
                }
                Ok(body)
            }
        }
    }

    fn json(&self, path: &str) -> Result<Option<Value>> {
        match self.get(path)? {
            Some(text) => Ok(Some(serde_json::from_str(&text).map_err(|e| anyhow!("{path} is not valid JSON: {e}"))?)),
            None => Ok(None),
        }
    }

    /// Every page, from the search index every Mira build writes.
    fn pages(&self) -> Result<Vec<Value>> {
        match self.json("/_mira/search.json")? {
            Some(Value::Array(pages)) => Ok(pages),
            Some(_) => bail!("/_mira/search.json is not a list of pages"),
            None => bail!("{} has no /_mira/search.json, so it is not a Mira site or was built without its agent files", self.describe()),
        }
    }

    /// The site description and the content it publishes. Sites built before
    /// the content index existed still answer, from their pages alone.
    fn content(&self) -> Result<Value> {
        match self.json("/_mira/content.json")? {
            Some(index) => Ok(index),
            None => Ok(json!({ "site": {}, "pages": "/_mira/search.json", "collections": [], "data": [] })),
        }
    }
}

/// Windows and macOS use the system TLS library and trust store; elsewhere,
/// rustls with Mozilla's roots. Release builds then need no OpenSSL and no C
/// compiler for cryptography.
fn tls() -> ureq::tls::TlsConfig {
    let config = ureq::tls::TlsConfig::builder();
    #[cfg(any(windows, target_os = "macos"))]
    let config = config.provider(ureq::tls::TlsProvider::NativeTls).root_certs(ureq::tls::RootCerts::PlatformVerifier);
    config.build()
}

/// Accepts `https://` sites, and `http://` only on this machine, without
/// credentials, query strings, or fragments. Returns the URL without a
/// trailing slash.
fn site_base(url: &str) -> Result<String> {
    let url = url.trim();
    let local = ["http://localhost", "http://127.0.0.1", "http://[::1]"]
        .iter()
        .any(|p| url.strip_prefix(p).is_some_and(|rest| rest.is_empty() || rest.starts_with([':', '/'])));
    if !url.starts_with("https://") && !local {
        bail!("--url {url}: use an https:// address (http:// is allowed only for localhost)");
    }
    let authority = url.split("://").nth(1).unwrap_or("").split('/').next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') || url.contains(['?', '#', ' ', '\\']) {
        bail!("--url {url}: give the site's address only, such as https://example.com");
    }
    Ok(url.trim_end_matches('/').to_string())
}

pub fn run(source: Source) -> Result<()> {
    eprintln!("mira mcp: serving {} over stdio", source.describe());
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            write(&mut stdout, &json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "parse error"}}))?;
            continue;
        };
        // Notifications carry no id and get no reply.
        let Some(id) = message.get("id").cloned() else { continue };
        let method = message["method"].as_str().unwrap_or("");
        let params = &message["params"];
        let reply = match method {
            "initialize" => Ok(json!({
                "protocolVersion": params["protocolVersion"].as_str().unwrap_or(PROTOCOL),
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "mira", "version": env!("CARGO_PKG_VERSION") },
                "instructions": format!(
                    "Read the Mira site at {}. Start with site_info to see its collections and data, use list_pages or search to find pages, read_page for a page's Markdown, list_entries for a collection's entries and fields, read_data for a data file, and list_media for images and video.",
                    source.describe()
                )
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools() })),
            "tools/call" => Ok(call(&source, params)),
            _ => Err(json!({"code": -32601, "message": format!("method not found: {method}")})),
        };
        let response = match reply {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Err(error) => json!({"jsonrpc": "2.0", "id": id, "error": error}),
        };
        write(&mut stdout, &response)?;
    }
    Ok(())
}

fn write(out: &mut impl Write, value: &Value) -> Result<()> {
    writeln!(out, "{value}")?;
    out.flush()?;
    Ok(())
}

fn tools() -> Value {
    let none = json!({ "type": "object", "properties": {} });
    json!([
        {
            "name": "site_info",
            "description": "Describe the site: its title, description, and URL, and every content collection (with entry counts and field types) and data file it publishes. Call this first.",
            "inputSchema": none
        },
        {
            "name": "list_pages",
            "description": "List every page on the site with its URL, title, and description.",
            "inputSchema": none
        },
        {
            "name": "read_page",
            "description": "Read one page as Markdown, with frontmatter giving its title and canonical URL.",
            "inputSchema": {
                "type": "object",
                "properties": { "url": { "type": "string", "description": "A page path such as / or /docs/install/" } },
                "required": ["url"]
            }
        },
        {
            "name": "search",
            "description": "Search the site's pages. Returns the best matches with URLs and a snippet.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 25 }
                },
                "required": ["query"]
            }
        },
        {
            "name": "list_entries",
            "description": "List a content collection's entries with all their fields, such as date, tags, and author, plus each entry's URL. Use read_page on a URL for the entry's full text.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "collection": { "type": "string", "description": "A collection name from site_info" },
                    "offset": { "type": "integer", "minimum": 0 },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200 }
                },
                "required": ["collection"]
            }
        },
        {
            "name": "read_data",
            "description": "Read one of the site's data files, such as navigation or a team list, as JSON.",
            "inputSchema": {
                "type": "object",
                "properties": { "name": { "type": "string", "description": "A data file name from site_info" } },
                "required": ["name"]
            }
        },
        {
            "name": "list_media",
            "description": "List the site's images and videos with their alt text, captions, sizes, and file URLs.",
            "inputSchema": none
        }
    ])
}

fn call(source: &Source, params: &Value) -> Value {
    let result = (|| -> Result<String> {
        source.refresh()?;
        let args = &params["arguments"];
        match params["name"].as_str().unwrap_or("") {
            "site_info" => {
                let mut info = source.content()?;
                info["page_count"] = json!(source.pages()?.len());
                Ok(serde_json::to_string_pretty(&info)?)
            }
            "list_pages" => {
                let pages: Vec<Value> = source
                    .pages()?
                    .iter()
                    .map(|d| json!({ "url": d["url"], "title": d["title"], "description": d["description"] }))
                    .collect();
                Ok(serde_json::to_string_pretty(&pages)?)
            }
            "read_page" => {
                let url = args["url"].as_str().unwrap_or("/");
                let missing = || anyhow!("no page at {url}; call list_pages to see every URL");
                let path = twin_path(url).ok_or_else(missing)?;
                source.get(&path)?.ok_or_else(missing)
            }
            "search" => {
                let query = args["query"].as_str().unwrap_or("");
                let limit = args["limit"].as_u64().unwrap_or(8).clamp(1, 25) as usize;
                Ok(serde_json::to_string_pretty(&search(&source.pages()?, query, limit))?)
            }
            "list_entries" => {
                let name = args["collection"].as_str().unwrap_or("");
                let unknown = || anyhow!("no collection named \"{name}\"; call site_info to see the collections");
                if !is_name(name) {
                    return Err(unknown());
                }
                let Some(Value::Array(entries)) = source.json(&format!("/_mira/collections/{name}.json"))? else {
                    return Err(unknown());
                };
                let offset = args["offset"].as_u64().unwrap_or(0) as usize;
                let limit = args["limit"].as_u64().unwrap_or(50).clamp(1, 200) as usize;
                let page: Vec<&Value> = entries.iter().skip(offset).take(limit).collect();
                Ok(serde_json::to_string_pretty(&json!({ "collection": name, "total": entries.len(), "offset": offset, "entries": page }))?)
            }
            "read_data" => {
                let name = args["name"].as_str().unwrap_or("");
                let unknown = || anyhow!("no data file named \"{name}\"; call site_info to see the data files");
                if !is_name(name) {
                    return Err(unknown());
                }
                let data = source.json(&format!("/_mira/data/{name}.json"))?.ok_or_else(unknown)?;
                Ok(serde_json::to_string_pretty(&data)?)
            }
            "list_media" => Ok(serde_json::to_string_pretty(&source.json("/media.json")?.unwrap_or_else(|| json!([])))?),
            other => bail!("unknown tool {other}"),
        }
    })();
    match result {
        Ok(text) => json!({ "content": [{ "type": "text", "text": text }] }),
        Err(err) => json!({ "content": [{ "type": "text", "text": err.to_string() }], "isError": true }),
    }
}

/// Collection and data file names: letters, digits, `-`, and `_`.
fn is_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 100 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Maps `/docs/install/` to `/docs/install.md` and `/` to `/index.md`.
/// Refuses paths that try to leave the site, or that carry encoded or
/// unusual characters.
fn twin_path(url: &str) -> Option<String> {
    let url = url.split(['?', '#']).next().unwrap_or("");
    let parts: Vec<&str> = url.split('/').filter(|s| !s.is_empty()).collect();
    if parts.iter().any(|p| *p == "." || *p == ".." || p.contains(['%', '\\', ':'])) {
        return None;
    }
    match parts.split_last() {
        None => Some("/index.md".into()),
        Some((last, dirs)) => {
            let mut path = String::new();
            for dir in dirs {
                path.push('/');
                path.push_str(dir);
            }
            Some(format!("{path}/{}.md", last.trim_end_matches(".md")))
        }
    }
}

fn search(index: &[Value], query: &str, limit: usize) -> Vec<Value> {
    let terms: Vec<String> = query.to_lowercase().split_whitespace().map(str::to_string).collect();
    let field = |d: &Value, k: &str| d[k].as_str().unwrap_or("").to_lowercase();
    let mut hits: Vec<(u32, Value)> = index
        .iter()
        .filter_map(|d| {
            let (title, desc, text) = (field(d, "title"), field(d, "description"), field(d, "text"));
            let headings: Vec<String> = d["headings"]
                .as_array()
                .map_or(Vec::new(), |h| h.iter().map(|x| x["text"].as_str().unwrap_or("").to_lowercase()).collect());
            let mut total = 0;
            for term in &terms {
                let mut s = 0;
                if title.contains(term.as_str()) {
                    s += 10;
                }
                if headings.iter().any(|h| h.contains(term.as_str())) {
                    s += 6;
                }
                if desc.contains(term.as_str()) {
                    s += 3;
                }
                if text.contains(term.as_str()) {
                    s += 1;
                }
                if s == 0 {
                    return None;
                }
                total += s;
            }
            let at = terms.first().and_then(|t| text.find(t.as_str())).unwrap_or(0);
            let raw = d["text"].as_str().unwrap_or("");
            let start = (0..=at.saturating_sub(60)).rev().find(|&i| raw.is_char_boundary(i)).unwrap_or(0);
            let end = (start + 200).min(raw.len());
            let end = (end..=raw.len()).find(|&i| raw.is_char_boundary(i)).unwrap_or(raw.len());
            Some((total, json!({ "url": d["url"], "title": d["title"], "snippet": &raw[start..end] })))
        })
        .collect();
    hits.sort_by_key(|hit| std::cmp::Reverse(hit.0));
    hits.into_iter().take(limit).map(|(_, v)| v).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_urls_to_twins_safely() {
        assert_eq!(twin_path("/").as_deref(), Some("/index.md"));
        assert_eq!(twin_path("/docs/install/").as_deref(), Some("/docs/install.md"));
        assert_eq!(twin_path("/docs/install.md").as_deref(), Some("/docs/install.md"));
        assert_eq!(twin_path("/../../etc/passwd"), None);
        assert_eq!(twin_path("/%2e%2e/secret"), None);
        assert_eq!(twin_path("/docs/c:/x"), None);
    }

    #[test]
    fn accepts_only_safe_site_addresses() {
        assert_eq!(site_base("https://example.com/").unwrap(), "https://example.com");
        assert_eq!(site_base("https://example.github.io/docs").unwrap(), "https://example.github.io/docs");
        assert_eq!(site_base("http://localhost:4321").unwrap(), "http://localhost:4321");
        for bad in [
            "http://example.com",
            "https://user:pass@example.com",
            "https://example.com/?x=1",
            "ftp://example.com",
            "https://",
            "http://localhost.evil.com",
        ] {
            assert!(site_base(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn names_are_plain() {
        assert!(is_name("posts") && is_name("team-members") && is_name("nav_2"));
        assert!(!is_name("") && !is_name("../secret") && !is_name("a/b") && !is_name("a.json"));
    }

    #[test]
    fn ranks_title_matches_first() {
        let index = vec![
            json!({"url": "/a/", "title": "Other", "headings": [], "text": "mentions routing once"}),
            json!({"url": "/b/", "title": "Routing", "headings": [], "text": "all about routing"}),
        ];
        let hits = search(&index, "routing", 5);
        assert_eq!(hits[0]["url"], "/b/");
        assert_eq!(hits.len(), 2);
    }

    /// Builds the starter, then asks every tool the same questions of the
    /// project and of the built site served over HTTP.
    #[test]
    fn project_and_deployed_site_answer_alike() {
        let root = std::env::temp_dir().join(format!("mira-mcp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        mira_compiler::scaffold::scaffold(&root).unwrap();
        std::fs::create_dir_all(root.join("data")).unwrap();
        std::fs::write(root.join("data/team.json"), r#"[{"name": "Ada"}]"#).unwrap();
        let dist = root.join("dist");
        build(&BuildOptions { root: root.clone(), out: dist.clone(), dev: false, host_config: false }).unwrap();

        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        std::thread::spawn(move || {
            for request in server.incoming_requests() {
                let path = request.url().split('?').next().unwrap_or("/").trim_start_matches('/').to_string();
                let file = dist.join(&path);
                let response = match std::fs::read(&file) {
                    Ok(bytes) if file.is_file() => tiny_http::Response::from_data(bytes),
                    _ => tiny_http::Response::from_data(b"not found".to_vec()).with_status_code(404),
                };
                let _ = request.respond(response);
            }
        });

        let project = Source::project(&root).unwrap();
        let site = Source::site(&format!("http://127.0.0.1:{port}")).unwrap();
        let ask = |source: &Source, name: &str, arguments: Value| {
            let reply = call(source, &json!({ "name": name, "arguments": arguments }));
            let text = reply["content"][0]["text"].as_str().unwrap().to_string();
            (reply["isError"] == json!(true), text)
        };
        for source in [&project, &site] {
            let (err, info) = ask(source, "site_info", json!({}));
            assert!(!err && info.contains("\"posts\"") && info.contains("team"), "{info}");
            let (err, pages) = ask(source, "list_pages", json!({}));
            assert!(!err && pages.contains("/posts/"), "{pages}");
            let (err, home) = ask(source, "read_page", json!({ "url": "/" }));
            assert!(!err && home.starts_with("---"), "{home}");
            let (err, entries) = ask(source, "list_entries", json!({ "collection": "posts" }));
            let entries: Value = serde_json::from_str(&entries).unwrap();
            assert!(!err && entries["total"] == 2 && entries["entries"][0]["date"].is_string(), "{entries}");
            let (err, team) = ask(source, "read_data", json!({ "name": "team" }));
            assert!(!err && team.contains("Ada"), "{team}");
            let (err, hits) = ask(source, "search", json!({ "query": "motion" }));
            assert!(!err && hits.contains("/posts/"), "{hits}");
            assert!(ask(source, "read_page", json!({ "url": "/nope/" })).0);
            assert!(ask(source, "read_page", json!({ "url": "/../mira.config.json" })).0);
            assert!(ask(source, "list_entries", json!({ "collection": "../data/team" })).0);
            assert!(ask(source, "read_data", json!({ "name": "missing" })).0);
        }
    }
}
