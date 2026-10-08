//! `mira mcp`: a Model Context Protocol server over stdio that gives agents
//! every part of a Mira site: its pages as Markdown, search, collection
//! entries with typed fields, data files, and media.
//!
//! It reads a local project, rebuilt only when its files change, or, with
//! `--url`, any deployed Mira site through the files every build publishes
//! under `/_mira/`. Both modes answer the same way.
//!
//! Answers are kept small because every byte is an agent's token: search
//! returns a few short matches that point at sections, `read` can return a
//! single section, and `items` filters a collection by its fields before
//! anything is sent.
//!
//! Actions the site declares, such as booking a table, are offered as
//! tools too. Their input is checked against its types, and nothing is sent
//! until the person using the agent agrees: through the client's own prompt
//! when it supports MCP elicitation, or with a one-time confirmation the
//! agent must ask them for.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Result, anyhow, bail};
use mira_compiler::{BuildOptions, build};
use serde_json::{Map, Value, json};

const PROTOCOL: &str = "2025-06-18";
/// The largest file the server reads, from disk or the network.
const MAX_BYTES: u64 = 16 * 1024 * 1024;
/// How long a deployed site's files are reused before fetching again.
const FRESH: Duration = Duration::from_secs(30);
/// Characters `read` returns when the caller sets no `max_chars`.
const READ_CHARS: usize = 24_000;
/// Characters per search snippet.
const SNIPPET: usize = 160;
/// How long an action's confirmation stays valid.
const CONFIRM_FOR: Duration = Duration::from_secs(600);
/// The most of an endpoint's response passed back to the agent.
const REPLY_BYTES: u64 = 4096;

/// Where a site's files come from.
enum Origin {
    /// A project on disk, built into `.mira/mcp`.
    Project { root: PathBuf, out: PathBuf },
    /// A deployed site, read over HTTPS.
    Site { base: String },
}

/// A file's contents, or `None` for a 404, and when it was read.
type Fetched = (Instant, Option<Rc<str>>);

/// A site and what has been read from it. One server lives for the whole
/// MCP session, so the site is built or fetched once and reused.
pub struct Source {
    origin: Origin,
    /// HTTPS client for deployed sites and action endpoints.
    agent: ureq::Agent,
    /// Actions waiting for confirmation, by token: when each was offered,
    /// the action, and the exact JSON it will send.
    pending: RefCell<HashMap<String, (Instant, String, String)>>,
    /// Files by path, with when they were read. `None` records a 404.
    files: RefCell<HashMap<String, Fetched>>,
    /// Parsed JSON files by path, dropped with `files`.
    parsed: RefCell<HashMap<String, Rc<Value>>>,
    /// The project's file stamp at the last build.
    stamp: RefCell<Option<u64>>,
}

impl Source {
    fn new(origin: Origin) -> Source {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .max_redirects(0)
            .http_status_as_error(false)
            .user_agent(concat!("mira-mcp/", env!("CARGO_PKG_VERSION")))
            .tls_config(tls())
            .build();
        Source {
            origin,
            agent: config.into(),
            pending: RefCell::default(),
            files: RefCell::default(),
            parsed: RefCell::default(),
            stamp: RefCell::default(),
        }
    }

    pub fn project(root: &Path) -> Result<Source> {
        let root = std::path::absolute(root)?;
        let out = root.join(".mira").join("mcp");
        Ok(Source::new(Origin::Project { root, out }))
    }

    /// A deployed Mira site at `url`, such as `https://example.com` or a
    /// site under a path like `https://example.github.io/docs`.
    pub fn site(url: &str) -> Result<Source> {
        Ok(Source::new(Origin::Site { base: site_base(url)? }))
    }

    fn describe(&self) -> String {
        match &self.origin {
            Origin::Project { root, .. } => root.display().to_string(),
            Origin::Site { base, .. } => base.clone(),
        }
    }

    /// Rebuilds a project when any of its files changed since the last
    /// build, and drops what was read from the old output. A deployed site
    /// is already built; its files expire on their own.
    fn refresh(&self) -> Result<()> {
        let Origin::Project { root, out } = &self.origin else { return Ok(()) };
        let stamp = project_stamp(root);
        if *self.stamp.borrow() == Some(stamp) {
            return Ok(());
        }
        self.files.borrow_mut().clear();
        self.parsed.borrow_mut().clear();
        *self.stamp.borrow_mut() = None;
        build(&BuildOptions { root: root.clone(), out: out.clone(), dev: false, host_config: false })?;
        *self.stamp.borrow_mut() = Some(stamp);
        Ok(())
    }

    /// Reads a file the build publishes, such as `/_mira/search.json`.
    /// `None` means the site has no such file.
    fn get(&self, path: &str) -> Result<Option<Rc<str>>> {
        debug_assert!(path.starts_with('/'));
        if let Some((at, body)) = self.files.borrow().get(path) {
            let fresh = match self.origin {
                Origin::Project { .. } => true,
                Origin::Site { .. } => at.elapsed() < FRESH,
            };
            if fresh {
                return Ok(body.clone());
            }
        }
        self.parsed.borrow_mut().remove(path);
        let body = self.fetch(path)?.map(Rc::from);
        self.files.borrow_mut().insert(path.to_string(), (Instant::now(), body.clone()));
        Ok(body)
    }

    fn fetch(&self, path: &str) -> Result<Option<String>> {
        match &self.origin {
            Origin::Project { out, .. } => {
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
            Origin::Site { base } => {
                let url = format!("{base}{path}");
                let mut response = self.agent.get(&url).call().map_err(|e| anyhow!("could not reach {url}: {e}"))?;
                let status = response.status().as_u16();
                match status {
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
                        Ok(Some(text))
                    }
                    404 | 410 => Ok(None),
                    300..=399 => bail!("{url} redirected; pass the site's final address to --url"),
                    _ => bail!("{url} returned HTTP {status}"),
                }
            }
        }
    }

    /// A JSON file, parsed once and reused until the file is read again.
    fn json(&self, path: &str) -> Result<Option<Rc<Value>>> {
        let Some(text) = self.get(path)? else { return Ok(None) };
        if let Some(value) = self.parsed.borrow().get(path) {
            return Ok(Some(value.clone()));
        }
        let value: Rc<Value> = Rc::new(serde_json::from_str(&text).map_err(|e| anyhow!("{path} is not valid JSON: {e}"))?);
        self.parsed.borrow_mut().insert(path.to_string(), value.clone());
        Ok(Some(value))
    }

    /// Every page, from the search index every Mira build writes.
    fn pages(&self) -> Result<Rc<Value>> {
        match self.json("/_mira/search.json")? {
            Some(pages) if pages.is_array() => Ok(pages),
            Some(_) => bail!("/_mira/search.json is not a list of pages"),
            None => bail!("{} has no /_mira/search.json, so it is not a Mira site or was built without its agent files", self.describe()),
        }
    }

    /// The site's actions from `/_mira/actions.json`, if it has any.
    fn actions(&self) -> Result<Vec<Value>> {
        Ok(self.json("/_mira/actions.json")?.and_then(|a| a["actions"].as_array().cloned()).unwrap_or_default())
    }

    /// The site description and the content it publishes. Sites built before
    /// the content index existed still answer, from their pages alone.
    fn content(&self) -> Result<Value> {
        match self.json("/_mira/content.json")? {
            Some(index) => Ok((*index).clone()),
            None => Ok(json!({ "site": {}, "collections": [], "data": [] })),
        }
    }
}

/// A cheap fingerprint of a project's source files: their paths, sizes, and
/// modification times. Output, dependency, and hidden folders are skipped.
fn project_stamp(root: &Path) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let skip = |name: &str| name.starts_with('.') || matches!(name, "dist" | "node_modules" | "target");
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(items) = std::fs::read_dir(&dir) else { continue };
        let mut items: Vec<_> = items.filter_map(Result::ok).collect();
        items.sort_by_key(|i| i.file_name());
        for item in items {
            if skip(&item.file_name().to_string_lossy()) {
                continue;
            }
            let Ok(meta) = item.metadata() else { continue };
            if meta.is_dir() {
                stack.push(item.path());
                continue;
            }
            item.path().hash(&mut hasher);
            meta.len().hash(&mut hasher);
            meta.modified().ok().and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).hash(&mut hasher);
        }
    }
    hasher.finish()
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

/// The client connection: messages in, one per line, and messages out.
/// A message that arrives while the server waits for the client's answer
/// to an elicitation is queued and handled after.
struct Client<R, W> {
    lines: std::io::Lines<R>,
    out: W,
    queued: VecDeque<String>,
    /// The client can show the person a confirmation prompt.
    elicits: bool,
    asked: u64,
}

impl<R: BufRead, W: Write> Client<R, W> {
    fn next(&mut self) -> Option<std::io::Result<String>> {
        self.queued.pop_front().map(Ok).or_else(|| self.lines.next())
    }

    fn send(&mut self, value: &Value) -> Result<()> {
        writeln!(self.out, "{value}")?;
        self.out.flush()?;
        Ok(())
    }

    /// Asks the person, through the client, to agree to `message`. `None`
    /// when the client cannot ask, so the caller falls back to a token.
    fn confirm(&mut self, message: &str) -> Option<bool> {
        if !self.elicits {
            return None;
        }
        self.asked += 1;
        let id = format!("mira-confirm-{}", self.asked);
        let request = json!({
            "jsonrpc": "2.0", "id": id, "method": "elicitation/create",
            "params": { "message": message, "requestedSchema": { "type": "object", "properties": {} } }
        });
        self.send(&request).ok()?;
        while let Some(Ok(line)) = self.lines.next() {
            match serde_json::from_str::<Value>(&line) {
                Ok(reply) if reply["id"] == id && reply.get("method").is_none() => {
                    // An error means the client could not ask after all.
                    return reply["result"]["action"].as_str().map(|action| action == "accept");
                }
                _ => self.queued.push_back(line),
            }
        }
        Some(false)
    }
}

pub fn run(source: Source) -> Result<()> {
    eprintln!("mira mcp: serving {} over stdio", source.describe());
    let stdin = std::io::stdin();
    let mut client =
        Client { lines: stdin.lock().lines(), out: std::io::stdout().lock(), queued: VecDeque::new(), elicits: false, asked: 0 };
    while let Some(line) = client.next() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            client.send(&json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "parse error"}}))?;
            continue;
        };
        // Notifications carry no id and get no reply, and neither do stray
        // answers to requests the server is no longer waiting on.
        let Some(id) = message.get("id").cloned() else { continue };
        let Some(method) = message["method"].as_str() else { continue };
        let params = &message["params"];
        let reply = match method {
            "initialize" => {
                client.elicits = params["capabilities"].get("elicitation").is_some();
                Ok(json!({
                    "protocolVersion": params["protocolVersion"].as_str().unwrap_or(PROTOCOL),
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "mira", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": format!(
                        "The Mira site at {}. search returns paths with #sections; read one with read. items queries collections by field.",
                        source.describe()
                    )
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools_for(&source) })),
            "tools/call" => Ok(call(&source, params, &mut |m| client.confirm(m))),
            _ => Err(json!({"code": -32601, "message": format!("method not found: {method}")})),
        };
        let response = match reply {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Err(error) => json!({"jsonrpc": "2.0", "id": id, "error": error}),
        };
        client.send(&response)?;
    }
    Ok(())
}

/// The built-in tools, then one per action the site declares.
fn tools_for(source: &Source) -> Value {
    let mut list = tools();
    let actions = source.refresh().and_then(|()| source.actions()).unwrap_or_default();
    if let Value::Array(tools) = &mut list {
        for action in actions {
            let Some(name) = action["name"].as_str() else { continue };
            let mut schema = action["input_schema"].clone();
            schema["properties"]["confirm"] = json!({ "type": "string" });
            let host = action["endpoint"].as_str().unwrap_or("").split('/').nth(2).unwrap_or("");
            tools.push(json!({
                "name": name,
                "description": format!("{} Sends to {host} once the person you act for agrees.", action["description"].as_str().unwrap_or("")),
                "inputSchema": schema,
                "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": true }
            }));
        }
    }
    list
}

/// The tools, described in as few words as an agent needs to use them.
/// Parameters are `name:type`, with `*` marking required ones.
pub fn tools() -> Value {
    let tool = |name: &str, description: &str, params: &[&str]| {
        let mut properties = Map::new();
        let mut required = Vec::new();
        for param in params {
            let (key, kind) = param.split_once(':').unwrap_or((param, "string"));
            let key = match key.strip_suffix('*') {
                Some(key) => {
                    required.push(key);
                    key
                }
                None => key,
            };
            let schema = match kind {
                "string[]" => json!({ "type": "array", "items": { "type": "string" } }),
                kind => json!({ "type": kind }),
            };
            properties.insert(key.to_string(), schema);
        }
        let mut schema = json!({ "type": "object", "properties": properties });
        if !required.is_empty() {
            schema["required"] = json!(required);
        }
        json!({ "name": name, "description": description, "inputSchema": schema })
    };
    json!([
        tool("site", "Overview: collections with field types, data files. Call first.", &[]),
        tool("pages", "Pages as path | title | description. prefix filters, e.g. /docs/.", &["prefix"]),
        tool("search", "Best matches as score path#section title, then a snippet. limit defaults to 5.", &["query*", "limit:integer"]),
        tool("read", "A page as Markdown. Use path#section to read one section.", &["path*", "section", "max_chars:integer"]),
        tool(
            "items",
            "Query a collection. where {field: value} or {field: {gt|gte|lt|lte|ne|contains|in: value}}; sort field or -field; limit defaults to 20.",
            &["collection*", "where:object", "fields:string[]", "sort", "limit:integer", "offset:integer"]
        ),
        tool("data", "A data file as JSON. path picks one value, e.g. hours.monday.", &["name*", "path"]),
        tool("media", "Images and video with alt text, captions, and sizes. page filters by path.", &["page"]),
    ])
}

fn call(source: &Source, params: &Value, confirm: &mut dyn FnMut(&str) -> Option<bool>) -> Value {
    let result = (|| -> Result<String> {
        source.refresh()?;
        let args = &params["arguments"];
        let text = |key: &str| args[key].as_str().unwrap_or("");
        let number = |key: &str| args[key].as_u64().map(|n| n.min(1 << 24) as usize);
        match params["name"].as_str().unwrap_or("") {
            "site" => {
                let mut info = source.content()?;
                let count = source.pages()?.as_array().map_or(0, Vec::len);
                if let Some(map) = info.as_object_mut() {
                    for key in ["schema", "generator", "pages"] {
                        map.remove(key);
                    }
                    map.insert("page_count".into(), json!(count));
                }
                Ok(info.to_string())
            }
            "pages" => {
                let prefix = text("prefix");
                let mut out = String::new();
                for page in source.pages()?.as_array().into_iter().flatten() {
                    let url = page["url"].as_str().unwrap_or("");
                    if !url.starts_with(prefix) {
                        continue;
                    }
                    out.push_str(&format!("{url} | {}", page["title"].as_str().unwrap_or("")));
                    if let Some(d) = page["description"].as_str() {
                        out.push_str(&format!(" | {d}"));
                    }
                    out.push('\n');
                }
                if out.is_empty() {
                    bail!("no pages start with {prefix}");
                }
                Ok(out)
            }
            "read" => {
                let (path, anchor) = split_anchor(text("path"));
                let missing = || anyhow!("no page at {path}; call pages or search to find one");
                let file = twin_path(path).ok_or_else(missing)?;
                let page = source.get(&file)?.ok_or_else(missing)?;
                let section = Some(text("section")).filter(|s| !s.is_empty()).or(anchor);
                read(&page, path, section, number("max_chars").unwrap_or(READ_CHARS).max(200))
            }
            "search" => {
                let limit = number("limit").unwrap_or(5).clamp(1, 20);
                let hits = search(source.pages()?.as_array().map_or(&[][..], Vec::as_slice), text("query"), limit);
                if hits.is_empty() {
                    return Ok(format!("no pages match \"{}\"", text("query")));
                }
                Ok(hits.join("\n"))
            }
            "items" => {
                let name = text("collection");
                let unknown = || anyhow!("no collection named \"{name}\"; call site to see the collections");
                if !is_name(name) {
                    return Err(unknown());
                }
                let entries = source.json(&format!("/_mira/collections/{name}.json"))?.ok_or_else(unknown)?;
                let query = Query::parse(args)?;
                Ok(query.run(entries.as_array().map_or(&[][..], Vec::as_slice)).to_string())
            }
            "data" => {
                let name = text("name");
                let unknown = || anyhow!("no data file named \"{name}\"; call site to see the data files");
                if !is_name(name) {
                    return Err(unknown());
                }
                let data = source.json(&format!("/_mira/data/{name}.json"))?.ok_or_else(unknown)?;
                let path = text("path");
                if path.is_empty() {
                    return Ok(data.to_string());
                }
                lookup(&data, path).map(Value::to_string).ok_or_else(|| anyhow!("{name} has nothing at {path}"))
            }
            "media" => {
                let media = source.json("/media.json")?;
                let page = text("page");
                let items: Vec<&Value> = media
                    .as_deref()
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter(|m| page.is_empty() || m["pages"].as_array().is_some_and(|p| p.iter().any(|u| u == page)))
                    .collect();
                Ok(serde_json::to_string(&items)?)
            }
            other => match source.actions()?.into_iter().find(|a| a["name"] == other) {
                Some(action) => act(source, &action, args, confirm),
                None => bail!("unknown tool {other}"),
            },
        }
    })();
    match result {
        Ok(text) => json!({ "content": [{ "type": "text", "text": text }] }),
        Err(err) => json!({ "content": [{ "type": "text", "text": err.to_string() }], "isError": true }),
    }
}

/// Runs an action: checks the input, gets the person's agreement, then
/// sends the input as a JSON `POST` to the action's endpoint.
fn act(source: &Source, action: &Value, args: &Value, confirm: &mut dyn FnMut(&str) -> Option<bool>) -> Result<String> {
    let name = action["name"].as_str().unwrap_or("");
    let endpoint = action["endpoint"].as_str().unwrap_or("");
    // A deployed site's actions are data from the network; check again.
    if !mira_compiler::actions::endpoint_allowed(endpoint) {
        bail!("{name} has an endpoint Mira will not send to: {endpoint}");
    }
    let mut input = args.as_object().cloned().unwrap_or_default();
    let token = input.remove("confirm").and_then(|t| t.as_str().map(str::to_string));
    let fields = action["input"].as_object().cloned().unwrap_or_default();
    let body = Value::Object(mira_compiler::actions::validate(&fields, &Value::Object(input))?).to_string();

    if action["confirm"] != json!(false) {
        let summary = summarize(action, &body);
        match (token, confirm(&summary)) {
            (_, Some(true)) => {}
            (_, Some(false)) => return Ok(format!("Not sent: the person declined. Nothing went to {endpoint}.")),
            (Some(token), None) => {
                let offered = source.pending.borrow_mut().remove(&token);
                match offered {
                    Some((at, n, b)) if n == name && b == body && at.elapsed() < CONFIRM_FOR => {}
                    Some(_) => bail!("this confirmation was for different input or has expired; call {name} again without confirm"),
                    None => bail!("unknown confirmation; call {name} again without confirm to get one"),
                }
            }
            (None, None) => {
                let token = new_token();
                source.pending.borrow_mut().retain(|_, (at, _, _)| at.elapsed() < CONFIRM_FOR);
                source.pending.borrow_mut().insert(token.clone(), (Instant::now(), name.to_string(), body));
                return Ok(format!(
                    "Not sent yet. Show the person you act for exactly this and ask whether to send it:\n\n{summary}\n\nIf they agree, call {name} again with the same input and confirm: \"{token}\". The confirmation works once, for this input, for 10 minutes."
                ));
            }
        }
    }

    let mut response = source
        .agent
        .post(endpoint)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .send(body.as_str())
        .map_err(|e| anyhow!("could not reach {endpoint}: {e}"))?;
    let status = response.status().as_u16();
    let mut reply = String::new();
    let _ = response.body_mut().as_reader().take(REPLY_BYTES).read_to_string(&mut reply);
    match status {
        200..=299 => Ok(format!("Sent. {endpoint} answered HTTP {status}.\n{}", reply.trim())),
        _ => bail!("{endpoint} answered HTTP {status}; the action may not have happened.\n{}", reply.trim()),
    }
}

/// What an action will do, for the person to agree to.
fn summarize(action: &Value, body: &str) -> String {
    let mut out = format!("{}\nSend to: {}", action["description"].as_str().unwrap_or(""), action["endpoint"].as_str().unwrap_or(""));
    if let Ok(Value::Object(fields)) = serde_json::from_str::<Value>(body) {
        for (field, value) in fields {
            let shown = match value {
                Value::String(s) => s,
                other => other.to_string(),
            };
            out.push_str(&format!("\n{field}: {shown}"));
        }
    }
    out
}

/// A random, single use confirmation token.
fn new_token() -> String {
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    let first = hasher.finish();
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u64(first);
    format!("{first:016x}{:016x}", hasher.finish())
}

/// Collection and data file names: letters, digits, `-`, and `_`.
fn is_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 100 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// `/docs/a/#install` → (`/docs/a/`, `Some("install")`). Accepts a full URL
/// too, keeping only its path.
fn split_anchor(raw: &str) -> (&str, Option<&str>) {
    let raw = match raw.split_once("://") {
        Some((_, rest)) => rest.find('/').map_or("/", |i| &rest[i..]),
        None => raw,
    };
    let (path, anchor) = match raw.split_once('#') {
        Some((p, a)) => (p, Some(a).filter(|a| !a.is_empty())),
        None => (raw, None),
    };
    (path.split('?').next().unwrap_or(""), anchor)
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

/// A heading in a Markdown page: its level, text, id, and the byte range of
/// the section it opens, up to the next heading at the same or a higher level.
struct Section<'a> {
    level: usize,
    text: &'a str,
    id: String,
    start: usize,
    end: usize,
}

/// Headings outside fenced code, with ids made the way the compiler makes
/// them, so `#id` links from search and from the page itself resolve.
fn sections(body: &str) -> Vec<Section<'_>> {
    let mut found: Vec<Section> = Vec::new();
    let mut used = HashMap::<String, usize>::new();
    let mut fenced = false;
    let mut at = 0;
    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_end();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
        }
        let level = trimmed.bytes().take_while(|b| *b == b'#').count();
        if !fenced && (1..=6).contains(&level) && trimmed[level..].starts_with(' ') {
            let text = trimmed[level..].trim();
            let (text, id) = match text.rsplit_once(" {#") {
                Some((t, id)) if id.ends_with('}') => (t.trim(), id.trim_end_matches('}').to_string()),
                _ => {
                    let mut id = mira_compiler::content::slugify(text);
                    let n = used.entry(id.clone()).or_insert(0);
                    *n += 1;
                    if *n > 1 {
                        id = format!("{id}-{}", *n - 1);
                    }
                    (text, id)
                }
            };
            found.push(Section { level, text, id, start: at, end: body.len() });
        }
        at += line.len();
    }
    for i in 0..found.len() {
        let level = found[i].level;
        if let Some(next) = found[i + 1..].iter().find(|s| s.level <= level) {
            found[i].end = next.start;
        }
    }
    found
}

/// A page's Markdown without its frontmatter, or one section of it, cut to
/// `max` characters on a line break. A cut page lists its later sections,
/// so the agent can read the rest a part at a time.
fn read(page: &str, path: &str, section: Option<&str>, max: usize) -> Result<String> {
    let body = strip_frontmatter(page).trim();
    let all = sections(body);
    let text = match section {
        None => body,
        Some(wanted) => {
            let key = wanted.trim().trim_start_matches('#').to_lowercase();
            let slug = mira_compiler::content::slugify(&key);
            let found = all
                .iter()
                .find(|s| s.id == key || s.id == slug || s.text.to_lowercase() == key)
                .or_else(|| all.iter().find(|s| s.level > 1 && (s.id.contains(&slug) || s.text.to_lowercase().contains(&key))));
            match found {
                Some(s) => body[s.start..s.end].trim(),
                None => {
                    let ids: Vec<&str> = all.iter().filter(|s| s.level > 1).map(|s| s.id.as_str()).collect();
                    bail!("no section \"{wanted}\" on {path}; sections: {}", if ids.is_empty() { "none".into() } else { ids.join(", ") });
                }
            }
        }
    };
    if text.chars().count() <= max {
        return Ok(text.to_string());
    }
    let offset = text.as_ptr() as usize - body.as_ptr() as usize;
    let cut = text.char_indices().nth(max).map_or(text.len(), |(i, _)| i);
    let cut = text[..cut].rfind('\n').filter(|&i| i > cut / 2).unwrap_or(cut);
    let rest = text[cut..].chars().count();
    let later: Vec<&str> =
        all.iter().filter(|s| s.level > 1 && s.start >= offset + cut && s.start < offset + text.len()).map(|s| s.id.as_str()).collect();
    let mut out = format!("{}\n\n[{rest} more characters", text[..cut].trim_end());
    if !later.is_empty() {
        out.push_str(&format!(". Read a section with path#id: {}", later.join(", ")));
    }
    out.push(']');
    Ok(out)
}

fn strip_frontmatter(page: &str) -> &str {
    page.strip_prefix("---\n").and_then(|rest| rest.find("\n---\n").map(|i| &rest[i + 5..])).unwrap_or(page)
}

/// Lowercased query terms, with a plural `s` dropped so `hours` finds `hour`.
fn terms(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| if t.len() > 3 && t.ends_with('s') && !t.ends_with("ss") { t[..t.len() - 1].to_string() } else { t.to_string() })
        .collect()
}

/// Ranks pages by where the terms appear: title, then headings, then the
/// description, then the text. Pages matching the most terms are the only
/// ones returned. Each hit names the heading that matches best, so the
/// agent can read only that section.
fn search(index: &[Value], query: &str, limit: usize) -> Vec<String> {
    let terms = terms(query);
    if terms.is_empty() {
        return Vec::new();
    }
    let field = |d: &Value, k: &str| d[k].as_str().unwrap_or("").to_lowercase();
    let mut hits: Vec<(usize, u32, String)> = index
        .iter()
        .filter_map(|d| {
            let (title, desc, text) = (field(d, "title"), field(d, "description"), field(d, "text"));
            let headings: Vec<(String, &str, &str)> = d["headings"].as_array().map_or(Vec::new(), |h| {
                h.iter()
                    .map(|x| {
                        let shown = x["text"].as_str().unwrap_or("");
                        (shown.to_lowercase(), x["id"].as_str().unwrap_or(""), shown)
                    })
                    .collect()
            });
            let (mut score, mut matched) = (0, 0);
            for term in &terms {
                let t = term.as_str();
                let weights =
                    [(title.contains(t), 10), (headings.iter().any(|h| h.0.contains(t)), 6), (desc.contains(t), 3), (text.contains(t), 1)];
                let s: u32 = weights.iter().filter(|(hit, _)| *hit).map(|(_, w)| w).sum();
                if s > 0 {
                    matched += 1;
                    score += s;
                }
            }
            if matched == 0 {
                return None;
            }
            // A heading is worth pointing at only for terms the title lacks.
            let beyond_title: Vec<&String> = terms.iter().filter(|t| !title.contains(t.as_str())).collect();
            let best = headings
                .iter()
                .map(|h| (beyond_title.iter().filter(|t| h.0.contains(t.as_str())).count(), h))
                .filter(|(n, _)| *n > 0)
                .max_by_key(|(n, _)| *n)
                .map(|(_, h)| h);
            let url = d["url"].as_str().unwrap_or("");
            let title = d["title"].as_str().unwrap_or("");
            let (target, label) = match best {
                Some((_, id, heading)) => (format!("{url}#{id}"), format!("{title} › {heading}")),
                None => (url.to_string(), title.to_string()),
            };
            let raw = d["text"].as_str().unwrap_or("");
            // The text opens with the title; a match there says nothing the
            // title does not, so the description stands in for it.
            let lower_title = title.to_lowercase();
            let skip = text.find(lower_title.as_str()).map_or(0, |i| i + lower_title.len());
            let found = terms.iter().filter_map(|t| text[skip..].find(t.as_str()).map(|i| i + skip)).min();
            let snippet = match (found, d["description"].as_str()) {
                (None, Some(description)) => clip(description, 0),
                (found, _) => clip_around(raw, &text, found.unwrap_or(0)),
            };
            Some((matched, score, format!("{score} {target} {label}\n  {snippet}")))
        })
        .collect();
    let most = hits.iter().map(|h| h.0).max().unwrap_or(0);
    hits.retain(|h| h.0 == most);
    hits.sort_by_key(|h| std::cmp::Reverse(h.1));
    hits.into_iter().take(limit).map(|h| h.2).collect()
}

/// About `SNIPPET` characters of `raw` around byte `at` of its lowercased
/// form `lower`, starting a few words before it.
fn clip_around(raw: &str, lower: &str, at: usize) -> String {
    // Lowercasing can change byte lengths; start from the top when the
    // offset does not carry over to the original.
    let at = if raw.len() == lower.len() && raw.is_char_boundary(at) { at } else { 0 };
    let start = raw[..at].char_indices().rev().nth(50).map_or(0, |(i, _)| i);
    let start = if start > 0 { raw[start..].find(' ').map_or(start, |i| start + i + 1) } else { 0 };
    clip(raw, start)
}

/// About `SNIPPET` characters of `raw` from byte `start`, cut on a space.
fn clip(raw: &str, start: usize) -> String {
    let mut out: String = raw[start..].chars().take(SNIPPET).collect();
    if start + out.len() < raw.len() {
        if let Some(i) = out.rfind(' ') {
            out.truncate(i);
        }
        out.push('…');
    }
    if start > 0 {
        out.insert(0, '…');
    }
    out
}

/// `hours.monday` or `team.0.name` inside a JSON value.
fn lookup<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').filter(|p| !p.is_empty()).try_fold(value, |v, key| match v {
        Value::Array(items) => items.get(key.parse::<usize>().ok()?),
        _ => v.get(key),
    })
}

/// An `items` query: filters, sort, field selection, and a page window.
struct Query {
    filters: Vec<(String, String, Value)>,
    fields: Option<Vec<String>>,
    sort: Option<(String, bool)>,
    limit: usize,
    offset: usize,
}

const OPERATORS: [&str; 8] = ["eq", "ne", "gt", "gte", "lt", "lte", "contains", "in"];

impl Query {
    fn parse(args: &Value) -> Result<Query> {
        let mut filters = Vec::new();
        match &args["where"] {
            Value::Null => {}
            Value::Object(map) => {
                for (field, condition) in map {
                    match condition {
                        Value::Object(ops) => {
                            for (op, value) in ops {
                                if !OPERATORS.contains(&op.as_str()) {
                                    bail!("where.{field}: unknown operator {op}; use {}", OPERATORS.join(", "));
                                }
                                filters.push((field.clone(), op.clone(), value.clone()));
                            }
                        }
                        value => filters.push((field.clone(), "eq".into(), value.clone())),
                    }
                }
            }
            _ => bail!("where must be an object such as {{\"tags\": \"news\"}}"),
        }
        let fields = match &args["fields"] {
            Value::Array(list) => Some(list.iter().filter_map(Value::as_str).map(str::to_string).collect()),
            _ => None,
        };
        let sort = args["sort"].as_str().filter(|s| !s.is_empty()).map(|s| match s.strip_prefix('-') {
            Some(field) => (field.to_string(), true),
            None => (s.to_string(), false),
        });
        let limit = args["limit"].as_u64().unwrap_or(20).clamp(1, 200) as usize;
        let offset = args["offset"].as_u64().unwrap_or(0).min(1 << 24) as usize;
        Ok(Query { filters, fields, sort, limit, offset })
    }

    fn run(&self, entries: &[Value]) -> Value {
        let mut matched: Vec<&Value> = entries.iter().filter(|e| self.filters.iter().all(|(f, op, v)| test(lookup(e, f), op, v))).collect();
        if let Some((field, descending)) = &self.sort {
            matched.sort_by(|a, b| {
                let order = compare(lookup(a, field).unwrap_or(&Value::Null), lookup(b, field).unwrap_or(&Value::Null));
                if *descending { order.reverse() } else { order }
            });
        }
        let total = matched.len();
        let items: Vec<Value> = matched.into_iter().skip(self.offset).take(self.limit).map(|e| self.shape(e)).collect();
        let mut out = json!({ "total": total, "items": items });
        if self.offset + self.limit < total {
            out["next_offset"] = json!(self.offset + self.limit);
        }
        out
    }

    /// The requested fields, or every field but the body. An entry with a
    /// page keeps its URL and drops the slug, which the URL already holds.
    fn shape(&self, entry: &Value) -> Value {
        let Some(map) = entry.as_object() else { return entry.clone() };
        let mut out = Map::new();
        match &self.fields {
            Some(fields) => {
                for f in fields {
                    if let Some(v) = lookup(entry, f) {
                        out.insert(f.clone(), v.clone());
                    }
                }
            }
            None => {
                for (k, v) in map {
                    let redundant = k == "slug" && map.get("url").is_some_and(|u| !u.is_null());
                    if k != "markdown" && !redundant && !v.is_null() {
                        out.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        Value::Object(out)
    }
}

/// Orders numbers numerically and everything else as text, which keeps
/// ISO dates and times in time order. Missing values sort last.
fn compare(a: &Value, b: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Null, _) => Ordering::Greater,
        (_, Value::Null) => Ordering::Less,
        (Value::Number(x), Value::Number(y)) => x.as_f64().unwrap_or(0.0).total_cmp(&y.as_f64().unwrap_or(0.0)),
        _ => scalar(a).cmp(&scalar(b)),
    }
}

fn scalar(v: &Value) -> String {
    match v {
        Value::String(s) => s.to_lowercase(),
        other => other.to_string(),
    }
}

fn test(value: Option<&Value>, op: &str, wanted: &Value) -> bool {
    let value = value.unwrap_or(&Value::Null);
    let eq = |a: &Value| compare(a, wanted).is_eq();
    match op {
        // A list field matches when any of its items does.
        "eq" => match value {
            Value::Array(items) => items.iter().any(eq),
            v => eq(v),
        },
        "ne" => !test(Some(value), "eq", wanted),
        "contains" => match value {
            Value::Array(items) => items.iter().any(eq),
            Value::String(s) => s.to_lowercase().contains(&scalar(wanted)),
            _ => false,
        },
        "in" => wanted.as_array().is_some_and(|options| options.iter().any(|o| test(Some(value), "eq", o))),
        _ if value.is_null() => false,
        "gt" => compare(value, wanted).is_gt(),
        "gte" => compare(value, wanted).is_ge(),
        "lt" => compare(value, wanted).is_lt(),
        "lte" => compare(value, wanted).is_le(),
        _ => false,
    }
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
        assert_eq!(split_anchor("https://x.dev/docs/a/#setup"), ("/docs/a/", Some("setup")));
        assert_eq!(split_anchor("/docs/a/?q=1"), ("/docs/a/", None));
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
    fn ranks_title_matches_first_and_points_at_sections() {
        let index = vec![
            json!({"url": "/a/", "title": "Other", "headings": [], "text": "mentions routing once"}),
            json!({"url": "/b/", "title": "Routing", "description": "How files become URLs.", "headings": [{"id": "dynamic", "text": "Dynamic routes"}], "text": "Routing all about it"}),
        ];
        let hits = search(&index, "routing", 5);
        assert_eq!(hits.len(), 2);
        // A title match needs no section, and its snippet is the description.
        assert_eq!(hits[0], "11 /b/ Routing\n  How files become URLs.");
        // Pages matching every term outrank pages matching some, and a
        // heading matching a term the title lacks is pointed at.
        let hits = search(&index, "dynamic routing", 5);
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert!(hits[0].starts_with("17 /b/#dynamic Routing › Dynamic routes"), "{hits:?}");
        assert!(search(&index, "nothing here", 5).is_empty());
    }

    #[test]
    fn reads_sections_and_cuts_long_pages() {
        let page = "---\ntitle: Hours\n---\n\n# Hours\n\nIntro.\n\n## Weekdays\n\nNine to five.\n\n### Holidays\n\nClosed.\n\n## Weekends\n\n```\n## not a heading\n```\n";
        let all = read(page, "/h/", None, 10_000).unwrap();
        assert!(all.starts_with("# Hours") && !all.contains("title:"), "{all}");
        let weekdays = read(page, "/h/", Some("weekdays"), 10_000).unwrap();
        assert_eq!(weekdays, "## Weekdays\n\nNine to five.\n\n### Holidays\n\nClosed.");
        assert_eq!(read(page, "/h/", Some("#holidays"), 10_000).unwrap(), "### Holidays\n\nClosed.");
        let err = read(page, "/h/", Some("lunch"), 10_000).unwrap_err().to_string();
        assert!(err.contains("weekdays, holidays, weekends"), "{err}");
        let cut = read(page, "/h/", None, 30).unwrap();
        assert!(cut.starts_with("# Hours\n\nIntro.") && cut.contains("more characters") && cut.ends_with("holidays, weekends]"), "{cut}");
    }

    #[test]
    fn queries_collections_by_field() {
        let entries = vec![
            json!({"slug": "a", "url": "/e/a/", "title": "Keynote", "starts": "2026-10-07T09:00", "price": 0, "tags": ["main"], "markdown": "body"}),
            json!({"slug": "b", "url": "/e/b/", "title": "Rust", "starts": "2026-10-07T14:30", "price": 40, "tags": ["talk"]}),
            json!({"slug": "c", "url": null, "title": "Party", "starts": "2026-10-08T20:00", "price": 15, "tags": ["social", "talk"]}),
        ];
        let run = |args: Value| Query::parse(&args).unwrap().run(&entries);
        let r = run(json!({"where": {"price": {"gt": 0}}, "sort": "-price", "fields": ["title", "price"]}));
        assert_eq!(r, json!({"total": 2, "items": [{"title": "Rust", "price": 40}, {"title": "Party", "price": 15}]}));
        let r = run(json!({"where": {"tags": "talk", "starts": {"lt": "2026-10-08"}}}));
        assert_eq!(r["items"], json!([{"url": "/e/b/", "title": "Rust", "starts": "2026-10-07T14:30", "price": 40, "tags": ["talk"]}]));
        let r = run(json!({"where": {"title": {"contains": "key"}}}));
        assert!(r["items"][0].get("markdown").is_none() && r["items"][0].get("slug").is_none(), "{r}");
        assert_eq!(run(json!({"where": {"title": {"in": ["rust", "party"]}}}))["total"], 2);
        assert_eq!(run(json!({"limit": 1}))["next_offset"], 1);
        assert!(Query::parse(&json!({"where": {"price": {"like": 1}}})).is_err());
        assert_eq!(lookup(&json!({"hours": [{"day": "mon"}]}), "hours.0.day"), Some(&json!("mon")));
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
            let reply = call(source, &json!({ "name": name, "arguments": arguments }), &mut |_| None);
            let text = reply["content"][0]["text"].as_str().unwrap().to_string();
            (reply["isError"] == json!(true), text)
        };
        for source in [&project, &site] {
            let (err, info) = ask(source, "site", json!({}));
            assert!(!err && info.contains("\"posts\"") && info.contains("team"), "{info}");
            let (err, pages) = ask(source, "pages", json!({ "prefix": "/posts/" }));
            assert!(!err && pages.starts_with("/posts/") && !pages.contains("\n/ |"), "{pages}");
            let (err, home) = ask(source, "read", json!({ "path": "/" }));
            assert!(!err && home.contains("# ") && !home.starts_with("---") && !home.contains("url:"), "{home}");
            let (err, entries) = ask(source, "items", json!({ "collection": "posts", "fields": ["title", "date"] }));
            let entries: Value = serde_json::from_str(&entries).unwrap();
            assert!(!err && entries["total"] == 2 && entries["items"][0]["date"].is_string(), "{entries}");
            let (err, team) = ask(source, "data", json!({ "name": "team", "path": "0.name" }));
            assert!(!err && team == "\"Ada\"", "{team}");
            let (err, hits) = ask(source, "search", json!({ "query": "motion" }));
            assert!(!err && hits.contains("/posts/"), "{hits}");
            assert!(ask(source, "read", json!({ "path": "/nope/" })).0);
            assert!(ask(source, "read", json!({ "path": "/../mira.config.json" })).0);
            assert!(ask(source, "items", json!({ "collection": "../data/team" })).0);
            assert!(ask(source, "data", json!({ "name": "missing" })).0);
        }

        // A project rebuilds only when one of its files changes.
        let stamp = *project.stamp.borrow();
        ask(&project, "site", json!({}));
        assert_eq!(*project.stamp.borrow(), stamp);
        std::fs::write(root.join("data/team.json"), r#"[{"name": "Grace"}]"#).unwrap();
        let (_, team) = ask(&project, "data", json!({ "name": "team" }));
        assert!(team.contains("Grace"), "{team}");
    }

    /// Declares an action, then books through it: refused when the input is
    /// wrong, held until confirmed, sent once, and never resent with a
    /// spent confirmation.
    #[test]
    fn actions_send_only_after_confirmation() {
        let endpoint = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = endpoint.server_addr().to_ip().unwrap().port();
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        std::thread::spawn(move || {
            for mut request in endpoint.incoming_requests() {
                let mut body = String::new();
                request.as_reader().read_to_string(&mut body).unwrap();
                tx.send(format!("{} {}", request.method(), body)).unwrap();
                let _ = request.respond(tiny_http::Response::from_string(r#"{"booking":"B-17"}"#).with_status_code(201));
            }
        });

        let root = std::env::temp_dir().join(format!("mira-act-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        mira_compiler::scaffold::scaffold(&root).unwrap();
        let config = std::fs::read_to_string(root.join("mira.config.json")).unwrap();
        let mut config: Value = serde_json::from_str(&config).unwrap();
        config["actions"] = json!({ "book_table": {
            "description": "Book a table for dinner.",
            "input": { "name": "string", "party_size": "number", "time": "time", "notes": "string?" },
            "endpoint": format!("http://127.0.0.1:{port}/book")
        }});
        std::fs::write(root.join("mira.config.json"), config.to_string()).unwrap();

        let source = Source::project(&root).unwrap();
        let tools = tools_for(&source);
        let tool = tools.as_array().unwrap().iter().find(|t| t["name"] == "book_table").expect("action tool");
        assert_eq!(tool["inputSchema"]["required"], json!(["name", "party_size", "time"]));

        let ask = |arguments: Value, answer: Option<bool>| {
            let reply = call(&source, &json!({ "name": "book_table", "arguments": arguments }), &mut |_| answer);
            (reply["isError"] == json!(true), reply["content"][0]["text"].as_str().unwrap().to_string())
        };
        let input = json!({ "name": "Ada", "party_size": 2, "time": "19:30" });

        let (err, text) = ask(json!({ "name": "Ada", "party_size": "two", "time": "19:30" }), None);
        assert!(err && text.contains("party_size must be a number"), "{text}");

        let (err, text) = ask(input.clone(), None);
        assert!(!err && text.starts_with("Not sent yet") && text.contains("party_size: 2"), "{text}");
        let token = text.split("confirm: \"").nth(1).unwrap().split('"').next().unwrap().to_string();
        assert!(rx.try_recv().is_err(), "nothing is sent before confirmation");

        let mut changed = input.clone();
        changed["party_size"] = json!(8);
        changed["confirm"] = json!(token.clone());
        assert!(ask(changed, None).0, "a confirmation covers only the input it was given for");

        let (err, text) = ask(json!({ "name": "Ada", "party_size": 2, "time": "19:30" }), None);
        let token = text.split("confirm: \"").nth(1).unwrap().split('"').next().unwrap().to_string();
        assert!(!err);
        let mut confirmed = input.clone();
        confirmed["confirm"] = json!(token);
        let (err, text) = ask(confirmed.clone(), None);
        assert!(!err && text.contains("HTTP 201") && text.contains("B-17"), "{text}");
        let sent = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(sent, r#"POST {"name":"Ada","party_size":2,"time":"19:30"}"#);
        assert!(ask(confirmed, None).0, "a confirmation works once");

        // A client that asks the person itself: declined sends nothing.
        let (err, text) = ask(input.clone(), Some(false));
        assert!(!err && text.starts_with("Not sent: the person declined"), "{text}");
        assert!(rx.try_recv().is_err());
        let (err, text) = ask(input, Some(true));
        assert!(!err && text.starts_with("Sent."), "{text}");
        assert!(rx.recv_timeout(Duration::from_secs(5)).is_ok());
    }
}
