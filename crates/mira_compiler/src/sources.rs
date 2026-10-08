//! Collections from a headless CMS, a database API, or any JSON or GraphQL
//! endpoint, fetched at build time: Sanity, Contentful, and Supabase by
//! name; Strapi, Directus, WordPress, Ghost, Hygraph, Shopify, Payload, and
//! functions on AWS, Google Cloud, or Azure through `json` or `graphql`.
//!
//! ```json
//! "collections": {
//!   "posts": {
//!     "fields": { "title": "string", "date": "date" },
//!     "source": {
//!       "sanity": { "project": "abc123", "dataset": "production", "query": "*[_type == \"post\"]" },
//!       "map": { "date": "publishedAt" }
//!     }
//!   }
//! }
//! ```
//!
//! Each item becomes an entry like a Markdown file in `content/<name>/`:
//! the schema checks it, routes give it a URL, and agents get it in the
//! content index. Rich text (Sanity Portable Text, Contentful Rich Text) is
//! converted to Markdown, and images in it are downloaded into the build
//! cache so they are sized and encoded like local ones.
//!
//! Tokens are read from environment variables named in the config, never
//! from the config itself. Responses are cached in `.mira/cache/`: `mira
//! dev` reuses them for ten minutes, and any build falls back to them, with
//! a warning, when the network fails.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use serde_json::{Map, Value, json};

/// The most a source may return, per request.
const MAX_BYTES: u64 = 32 * 1024 * 1024;
/// How long `mira dev` reuses a cached response.
const DEV_FRESH: Duration = Duration::from_secs(600);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub sanity: Option<Sanity>,
    pub contentful: Option<Contentful>,
    pub json: Option<JsonApi>,
    pub supabase: Option<Supabase>,
    pub graphql: Option<GraphQl>,
    /// Entry fields taken from other paths in each item, such as
    /// `"date": "publishedAt"` or `"slug": "slug.current"`.
    #[serde(default)]
    pub map: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sanity {
    pub project: String,
    pub dataset: String,
    /// A GROQ query returning a list of documents.
    pub query: String,
    /// An environment variable holding a read token, for private datasets.
    pub token_env: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contentful {
    pub space: String,
    #[serde(default = "master")]
    pub environment: String,
    pub content_type: String,
    /// The environment variable holding a Content Delivery API token.
    pub token_env: String,
    pub locale: Option<String>,
}

fn master() -> String {
    "master".into()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JsonApi {
    pub url: String,
    /// Where the list sits in the response, such as `data.posts`. The
    /// response itself when left out.
    pub items: Option<String>,
    /// An environment variable holding a bearer token.
    pub token_env: Option<String>,
    /// Request headers, each naming the environment variable that holds
    /// its value, such as `{ "x-api-key": "API_KEY" }`.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

/// A GraphQL query, sent as a `POST`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphQl {
    pub url: String,
    pub query: String,
    /// Where the list sits in the response, such as `data.posts.nodes`.
    pub items: String,
    pub token_env: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

/// A Supabase table, read through its REST API.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Supabase {
    /// The project URL, such as `https://abcd.supabase.co`.
    pub url: String,
    pub table: String,
    /// Columns, as in PostgREST: `*`, or `title,slug,author(name)`.
    #[serde(default = "star")]
    pub select: String,
    /// Column filters, as in PostgREST: `{ "published": "eq.true" }`.
    #[serde(default)]
    pub filter: BTreeMap<String, String>,
    /// The environment variable holding the anon or service key.
    pub key_env: String,
}

fn star() -> String {
    "*".into()
}

/// An item turned into an entry: its fields, its Markdown for rendering
/// (images pointing into the cache), and its Markdown for agents (images
/// pointing at their original URLs).
pub struct Item {
    pub data: Map<String, Value>,
    pub markdown: String,
    pub twin: String,
}

impl Source {
    /// Checks the source when the config loads.
    pub fn check(&self, collection: &str) -> Result<()> {
        let at = format!("mira.config.json: collections.{collection}.source");
        let kinds =
            [self.sanity.is_some(), self.contentful.is_some(), self.json.is_some(), self.supabase.is_some(), self.graphql.is_some()]
                .iter()
                .filter(|k| **k)
                .count();
        if kinds != 1 {
            bail!("{at}: name exactly one of sanity, contentful, supabase, graphql, or json");
        }
        let plain = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        let env_ok = |name: &Option<String>| name.as_deref().is_none_or(plain);
        if let Some(s) = &self.sanity
            && (!plain(&s.project) || !plain(&s.dataset) || !env_ok(&s.token_env))
        {
            bail!("{at}.sanity: project, dataset, and token_env take letters, digits, - and _ only");
        }
        if let Some(c) = &self.contentful
            && (!plain(&c.space) || !plain(&c.environment) || !plain(&c.content_type) || !plain(&c.token_env))
        {
            bail!("{at}.contentful: space, environment, content_type, and token_env take letters, digits, - and _ only");
        }
        let https = |url: &str| url.starts_with("https://") && !url.contains(['@', ' ', '?', '#']);
        let headers_ok = |headers: &BTreeMap<String, String>| {
            headers.iter().all(|(k, v)| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') && plain(v))
        };
        if let Some(g) = &self.graphql {
            if !https(&g.url) {
                bail!("{at}.graphql.url must be an https:// URL without credentials or a query string");
            }
            if !env_ok(&g.token_env) || !headers_ok(&g.headers) {
                bail!("{at}.graphql: token_env and header values name environment variables, with letters, digits, - and _ only");
            }
        }
        if let Some(s) = &self.supabase {
            let column = |c: &str| !c.is_empty() && c.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            if !https(&s.url) || !plain(&s.table) || !plain(&s.key_env) || !s.filter.keys().all(|k| column(k)) {
                bail!(
                    "{at}.supabase: url is the https:// project URL, and table, key_env, and filter columns take letters, digits, - and _ only"
                );
            }
        }
        if let Some(j) = &self.json {
            if !j.url.starts_with("https://") || j.url.contains(['@', ' ']) {
                bail!(
                    "{at}.json.url must be an https:// URL without credentials\nhint: put tokens in an environment variable and name it in token_env"
                );
            }
            if !env_ok(&j.token_env) || !headers_ok(&j.headers) {
                bail!("{at}.json: token_env and header values name environment variables, with letters, digits, - and _ only");
            }
        }
        Ok(())
    }

    /// Fetches every item. `fields` are the schema's fields, if it has
    /// any: only those are taken from each item, plus `slug` and `body`.
    pub fn load(
        &self,
        root: &Path,
        collection: &str,
        fields: Option<&BTreeMap<String, String>>,
        dev: bool,
    ) -> Result<(Vec<Item>, Vec<String>)> {
        let mut fetch = Fetcher { root, dev, warnings: Vec::new() };
        let (raw, kind): (Vec<Value>, &str) = if let Some(s) = &self.sanity {
            (fetch.sanity(s)?, "sanity")
        } else if let Some(c) = &self.contentful {
            (fetch.contentful(c)?, "contentful")
        } else if let Some(j) = &self.json {
            (fetch.json(j)?, "json")
        } else if let Some(s) = &self.supabase {
            (fetch.supabase(s)?, "supabase")
        } else if let Some(g) = &self.graphql {
            (fetch.graphql(g)?, "graphql")
        } else {
            bail!("collections.{collection}.source names no source");
        };
        let mut items = Vec::with_capacity(raw.len());
        for (i, item) in raw.iter().enumerate() {
            let at = || format!("{kind} item {} of collections.{collection}", i + 1);
            let mut data = Map::new();
            let wanted: Vec<String> = match fields {
                Some(f) => f.keys().cloned().chain(self.map.keys().cloned()).collect(),
                None => item.as_object().map(|o| o.keys().filter(|k| !k.starts_with('_')).cloned().collect()).unwrap_or_default(),
            };
            for field in wanted {
                if field == "body" {
                    continue;
                }
                let path = self.map.get(&field).map_or(field.as_str(), String::as_str);
                if let Some(value) = lookup(item, path).filter(|v| !v.is_null()) {
                    data.insert(field.clone(), plain_value(value));
                }
            }
            let slug_path = self.map.get("slug").map_or("slug", String::as_str);
            let slug = match lookup(item, slug_path) {
                Some(Value::String(s)) => s.clone(),
                Some(Value::Number(n)) => n.to_string(),
                Some(Value::Object(o)) if o.get("current").is_some_and(Value::is_string) => o["current"].as_str().unwrap().to_string(),
                _ => bail!("{}: has no slug at {slug_path}\nhint: map one with \"map\": {{ \"slug\": \"path.to.slug\" }}", at()),
            };
            data.insert("slug".into(), Value::from(slug));
            let body_path = self.map.get("body").map_or("body", String::as_str);
            let mut images = Vec::new();
            let markdown = match lookup(item, body_path) {
                None | Some(Value::Null) => String::new(),
                Some(Value::String(s)) => s.clone(),
                Some(Value::Array(blocks)) => portable_text(blocks, self.sanity.as_ref(), &mut images),
                Some(Value::Object(doc)) if doc.get("nodeType").is_some() => rich_text(&Value::Object(doc.clone()), &mut images),
                Some(other) => bail!("{}: the body at {body_path} is {}, not text or rich text", at(), kind_of(other)),
            };
            let twin = markdown.clone();
            let markdown = fetch.localize_images(markdown, &images)?;
            items.push(Item { data, markdown, twin });
        }
        Ok((items, fetch.warnings))
    }
}

/// Strips CMS wrappers from a field value: a Sanity slug becomes its
/// string.
fn plain_value(value: &Value) -> Value {
    match value {
        Value::Object(o) if o.get("_type").and_then(Value::as_str) == Some("slug") => o.get("current").cloned().unwrap_or(Value::Null),
        other => other.clone(),
    }
}

fn kind_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "empty",
        Value::Bool(_) => "true or false",
        Value::Number(_) => "a number",
        Value::String(_) => "text",
        Value::Array(_) => "a list",
        Value::Object(_) => "an object",
    }
}

/// `slug.current` or `authors.0.name` inside a JSON value.
pub fn lookup<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').filter(|p| !p.is_empty()).try_fold(value, |v, key| match v {
        Value::Array(items) => items.get(key.parse::<usize>().ok()?),
        _ => v.get(key),
    })
}

struct Fetcher<'a> {
    root: &'a Path,
    dev: bool,
    warnings: Vec<String>,
}

impl Fetcher<'_> {
    fn cache_dir(&self, kind: &str) -> PathBuf {
        self.root.join(".mira").join("cache").join(kind)
    }

    /// Requests `url` (a `POST` when there is a body), reusing or falling
    /// back to the cached copy. `label` is how errors name the request,
    /// without any token.
    fn get(&mut self, url: &str, headers: &[(String, String)], body: Option<&str>, label: &str) -> Result<Vec<u8>> {
        let file = cache_file(self.root, &cache_key(url, body));
        let cached = std::fs::metadata(&file).ok().and_then(|m| m.modified().ok()).and_then(|t| t.elapsed().ok());
        if self.dev && cached.is_some_and(|age| age < DEV_FRESH) {
            return Ok(std::fs::read(&file)?);
        }
        match http(url, headers, body) {
            Ok(bytes) => {
                std::fs::create_dir_all(file.parent().unwrap())?;
                std::fs::write(&file, &bytes)?;
                Ok(bytes)
            }
            Err(err) if cached.is_some() => {
                self.warnings.push(format!("{label}: {err}; built from the copy cached {} minutes ago", cached.unwrap().as_secs() / 60));
                Ok(std::fs::read(&file)?)
            }
            Err(err) => Err(err.context(label.to_string())),
        }
    }

    fn get_json(&mut self, url: &str, headers: &[(String, String)], label: &str) -> Result<Value> {
        let bytes = self.get(url, headers, None, label)?;
        serde_json::from_slice(&bytes).map_err(|e| anyhow!("{label}: the response is not JSON: {e}"))
    }

    fn sanity(&mut self, s: &Sanity) -> Result<Vec<Value>> {
        let token = env_token(s.token_env.as_deref())?;
        let host = if token.is_some() { "api" } else { "apicdn" };
        let url = format!(
            "https://{}.{host}.sanity.io/v2025-02-19/data/query/{}?query={}&perspective=published",
            s.project,
            s.dataset,
            encode(&s.query)
        );
        let response = self.get_json(&url, &bearer(token), &format!("Sanity project {}", s.project))?;
        match response.get("result") {
            Some(Value::Array(items)) => Ok(items.clone()),
            _ => bail!("Sanity project {}: the query must return a list of documents, such as *[_type == \"post\"]", s.project),
        }
    }

    fn contentful(&mut self, c: &Contentful) -> Result<Vec<Value>> {
        let token = env_token(Some(&c.token_env))?;
        let label = format!("Contentful space {}", c.space);
        let mut items = Vec::new();
        let mut assets = Map::new();
        let mut entries = Map::new();
        let mut skip = 0;
        loop {
            let mut url = format!(
                "https://cdn.contentful.com/spaces/{}/environments/{}/entries?content_type={}&include=2&limit=1000&skip={skip}",
                c.space, c.environment, c.content_type
            );
            if let Some(locale) = &c.locale {
                url.push_str(&format!("&locale={}", encode(locale)));
            }
            let page = self.get_json(&url, &bearer(token.clone()), &label)?;
            if let Some(message) = page.get("message").and_then(Value::as_str).filter(|_| page.get("items").is_none()) {
                bail!("{label}: {message}");
            }
            for asset in page["includes"]["Asset"].as_array().into_iter().flatten() {
                if let Some(id) = asset["sys"]["id"].as_str() {
                    assets.insert(id.to_string(), asset["fields"].clone());
                }
            }
            for entry in page["includes"]["Entry"].as_array().into_iter().flatten() {
                if let Some(id) = entry["sys"]["id"].as_str() {
                    entries.insert(id.to_string(), entry["fields"].clone());
                }
            }
            let batch = page["items"].as_array().cloned().unwrap_or_default();
            let total = page["total"].as_u64().unwrap_or(0) as usize;
            skip += batch.len();
            for item in &batch {
                // Entries in the results can link to each other, too.
                if let Some(id) = item["sys"]["id"].as_str() {
                    entries.insert(id.to_string(), item["fields"].clone());
                }
                let mut fields = item["fields"].clone();
                if let Value::Object(map) = &mut fields {
                    map.entry("id").or_insert_with(|| item["sys"]["id"].clone());
                    map.entry("created").or_insert_with(|| item["sys"]["createdAt"].clone());
                    map.entry("updated").or_insert_with(|| item["sys"]["updatedAt"].clone());
                }
                items.push(fields);
            }
            if batch.is_empty() || skip >= total {
                break;
            }
        }
        // Linked assets become their URLs, and linked entries their fields.
        let includes = json!({ "Asset": assets, "Entry": entries });
        Ok(items.into_iter().map(|item| resolve_links(item, &includes, 0)).collect())
    }

    fn json(&mut self, j: &JsonApi) -> Result<Vec<Value>> {
        let headers = headers(j.token_env.as_deref(), &j.headers)?;
        let host = j.url.split('/').nth(2).unwrap_or("");
        let label = format!("JSON source {host}");
        let response = self.get_json(&j.url, &headers, &label)?;
        let list = match &j.items {
            Some(path) => lookup(&response, path).ok_or_else(|| anyhow!("{label}: the response has nothing at {path}"))?,
            None => &response,
        };
        match list {
            Value::Array(items) => Ok(items.clone()),
            other => bail!(
                "{label}: expected a list of items, got {}\nhint: set \"items\" to the path of the list, such as data.posts",
                kind_of(other)
            ),
        }
    }

    fn graphql(&mut self, g: &GraphQl) -> Result<Vec<Value>> {
        let mut headers = headers(g.token_env.as_deref(), &g.headers)?;
        headers.push(("Content-Type".into(), "application/json".into()));
        let host = g.url.split('/').nth(2).unwrap_or("");
        let label = format!("GraphQL source {host}");
        let body = json!({ "query": g.query }).to_string();
        let bytes = self.get(&g.url, &headers, Some(&body), &label)?;
        let response: Value = serde_json::from_slice(&bytes).map_err(|e| anyhow!("{label}: the response is not JSON: {e}"))?;
        if let Some(message) = response["errors"][0]["message"].as_str() {
            bail!("{label}: {message}");
        }
        match lookup(&response, &g.items) {
            Some(Value::Array(items)) => Ok(items.clone()),
            _ => bail!("{label}: the response has no list at {}\nhint: set \"items\" to its path, such as data.posts.nodes", g.items),
        }
    }

    /// Reads every row a thousand at a time, so tables of any size load.
    fn supabase(&mut self, s: &Supabase) -> Result<Vec<Value>> {
        let key = env_token(Some(&s.key_env))?.unwrap_or_default();
        let headers = vec![("apikey".to_string(), key.clone()), ("Authorization".to_string(), format!("Bearer {key}"))];
        let label = format!("Supabase table {}", s.table);
        let mut rows = Vec::new();
        loop {
            let mut url = format!("{}/rest/v1/{}?select={}", s.url.trim_end_matches('/'), s.table, encode(&s.select));
            for (column, condition) in &s.filter {
                url.push_str(&format!("&{column}={}", encode(condition)));
            }
            url.push_str(&format!("&limit=1000&offset={}", rows.len()));
            let page = self.get_json(&url, &headers, &label)?;
            let Value::Array(batch) = page else {
                bail!("{label}: {}", page["message"].as_str().unwrap_or("the response is not a list of rows"));
            };
            let done = batch.len() < 1000;
            rows.extend(batch);
            if done {
                break;
            }
        }
        Ok(rows)
    }

    /// Downloads each image into the cache and points the Markdown at the
    /// copy, which the media pipeline then sizes and encodes. Formats it
    /// cannot encode stay as links to the original.
    fn localize_images(&mut self, mut markdown: String, images: &[String]) -> Result<String> {
        for url in images {
            let ext = url.split(['?', '#']).next().unwrap_or("").rsplit('.').next().unwrap_or("").to_ascii_lowercase();
            if !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp") {
                continue;
            }
            let dir = self.cache_dir("media");
            let name = format!("{}.{ext}", hash(url));
            let file = dir.join(&name);
            if !file.is_file() {
                let bytes = http(url, &[], None).with_context(|| format!("downloading image {url}"))?;
                std::fs::create_dir_all(&dir)?;
                std::fs::write(&file, bytes)?;
            }
            markdown = markdown.replace(&format!("]({url})"), &format!("](@/.mira/cache/media/{name})"));
        }
        Ok(markdown)
    }
}

/// Where the response to a request is cached, keyed by `cache_key`.
pub(crate) fn cache_file(root: &Path, key: &str) -> PathBuf {
    root.join(".mira").join("cache").join("sources").join(format!("{}.json", hash(key)))
}

/// A request's URL and body, which together decide its response. Tokens
/// are left out, so they never touch the disk.
pub(crate) fn cache_key(url: &str, body: Option<&str>) -> String {
    match body {
        Some(body) => format!("{url}\n{body}"),
        None => url.to_string(),
    }
}

fn bearer(token: Option<String>) -> Vec<(String, String)> {
    token.map(|t| vec![("Authorization".to_string(), format!("Bearer {t}"))]).unwrap_or_default()
}

/// A bearer token and named headers, read from their environment variables.
fn headers(token_env: Option<&str>, named: &BTreeMap<String, String>) -> Result<Vec<(String, String)>> {
    let mut out = bearer(env_token(token_env)?);
    for (header, env) in named {
        out.push((header.clone(), env_token(Some(env))?.unwrap_or_default()));
    }
    Ok(out)
}

fn env_token(name: Option<&str>) -> Result<Option<String>> {
    let Some(name) = name else { return Ok(None) };
    match std::env::var(name) {
        Ok(token) if !token.trim().is_empty() => Ok(Some(token.trim().to_string())),
        _ => bail!("the environment variable {name} is not set\nhint: set it to the token, or add it to your host's environment variables"),
    }
}

fn http(url: &str, headers: &[(String, String)], body: Option<&str>) -> Result<Vec<u8>> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .max_redirects(3)
        .https_only(true)
        .http_status_as_error(false)
        .user_agent(concat!("mira/", env!("CARGO_PKG_VERSION")))
        .tls_config(tls())
        .build();
    let agent: ureq::Agent = config.into();
    // The URL can carry a query but never a token; errors name the host.
    let host = url.split('/').nth(2).unwrap_or(url);
    let sent = match body {
        Some(body) => {
            let mut request = agent.post(url).header("Accept", "application/json");
            for (name, value) in headers {
                request = request.header(name, value);
            }
            request.send(body)
        }
        None => {
            let mut request = agent.get(url).header("Accept", "application/json");
            for (name, value) in headers {
                request = request.header(name, value);
            }
            request.call()
        }
    };
    let mut response = sent.map_err(|e| anyhow!("could not reach {host}: {e}"))?;
    let status = response.status().as_u16();
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(response.body_mut().as_reader(), MAX_BYTES + 1), &mut bytes)
        .map_err(|e| anyhow!("could not read from {host}: {e}"))?;
    if bytes.len() as u64 > MAX_BYTES {
        bail!("{host} returned more than {} MB", MAX_BYTES / 1024 / 1024);
    }
    match status {
        200..=299 => Ok(bytes),
        401 | 403 => bail!("{host} refused the request (HTTP {status})\nhint: check the token and its permissions"),
        _ => {
            let text = String::from_utf8_lossy(&bytes);
            let detail: String = text.chars().take(200).collect();
            bail!("{host} returned HTTP {status}: {detail}")
        }
    }
}

/// Windows and macOS use the system TLS library; elsewhere, rustls.
fn tls() -> ureq::tls::TlsConfig {
    let config = ureq::tls::TlsConfig::builder();
    #[cfg(any(windows, target_os = "macos"))]
    let config = config.provider(ureq::tls::TlsProvider::NativeTls).root_certs(ureq::tls::RootCerts::PlatformVerifier);
    config.build()
}

fn hash(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(text.as_bytes());
    digest.iter().take(10).map(|b| format!("{b:02x}")).collect()
}

/// Percent-encodes a query string value.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Replaces Contentful links with what they point at: an asset with its
/// URL, title, and description, an entry with its fields. Two levels deep.
fn resolve_links(value: Value, includes: &Value, depth: usize) -> Value {
    match value {
        Value::Object(map) if map.get("sys").and_then(|s| s.get("type")).and_then(Value::as_str) == Some("Link") => {
            let sys = &map["sys"];
            let id = sys["id"].as_str().unwrap_or("");
            match sys["linkType"].as_str() {
                Some("Asset") => match includes["Asset"].get(id) {
                    Some(asset) => json!({
                        "url": asset["file"]["url"].as_str().map(|u| if u.starts_with("//") { format!("https:{u}") } else { u.to_string() }),
                        "title": asset["title"],
                        "description": asset["description"],
                    }),
                    None => Value::Null,
                },
                Some("Entry") if depth < 2 => {
                    includes["Entry"].get(id).cloned().map_or(Value::Null, |e| resolve_links(e, includes, depth + 1))
                }
                _ => Value::Null,
            }
        }
        // Rich text stays a document; its embedded assets resolve when it
        // is converted.
        Value::Object(map) if map.get("nodeType").is_some() => embed_assets(Value::Object(map), includes),
        Value::Object(map) => Value::Object(map.into_iter().map(|(k, v)| (k, resolve_links(v, includes, depth))).collect()),
        Value::Array(items) => Value::Array(items.into_iter().map(|v| resolve_links(v, includes, depth)).collect()),
        other => other,
    }
}

/// Fills `data.target` of embedded asset nodes with the asset itself.
fn embed_assets(mut node: Value, includes: &Value) -> Value {
    if node["nodeType"] == "embedded-asset-block" {
        let id = node["data"]["target"]["sys"]["id"].as_str().unwrap_or("").to_string();
        if let Some(asset) = includes["Asset"].get(&id) {
            node["data"]["asset"] = asset.clone();
        }
    }
    if let Some(content) = node.get_mut("content").and_then(Value::as_array_mut) {
        for child in content.iter_mut() {
            *child = embed_assets(child.take(), includes);
        }
    }
    node
}

/// Escapes text so Markdown shows it as written.
fn escape_md(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '\\' | '*' | '_' | '`' | '[' | ']' | '<') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Sanity Portable Text to Markdown: paragraphs, headings, quotes, lists,
/// bold, italic, code, strike-through, links, images, and code blocks.
pub fn portable_text(blocks: &[Value], sanity: Option<&Sanity>, images: &mut Vec<String>) -> String {
    let mut out = String::new();
    let mut in_list = false;
    for block in blocks {
        let kind = block["_type"].as_str().unwrap_or("");
        let list = block["listItem"].as_str();
        if in_list && list.is_none() {
            out.push('\n');
        }
        in_list = list.is_some();
        match kind {
            "block" => {
                let links: BTreeMap<&str, &str> = block["markDefs"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|d| d["_type"] == "link")
                    .filter_map(|d| Some((d["_key"].as_str()?, d["href"].as_str()?)))
                    .collect();
                let mut text = String::new();
                for span in block["children"].as_array().into_iter().flatten() {
                    let mut piece = escape_md(span["text"].as_str().unwrap_or(""));
                    if piece.trim().is_empty() {
                        text.push_str(&piece);
                        continue;
                    }
                    for mark in span["marks"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                        piece = match mark {
                            "strong" => format!("**{piece}**"),
                            "em" => format!("*{piece}*"),
                            "code" => format!("`{}`", span["text"].as_str().unwrap_or("").replace('`', "")),
                            "strike-through" => format!("~~{piece}~~"),
                            key => match links.get(key) {
                                Some(href) => format!("[{piece}]({href})"),
                                None => piece,
                            },
                        };
                    }
                    text.push_str(&piece);
                }
                let level = block["level"].as_u64().unwrap_or(1).max(1) as usize - 1;
                let line = match (list, block["style"].as_str().unwrap_or("normal")) {
                    (Some("number"), _) => format!("{}1. {text}\n", "   ".repeat(level)),
                    (Some(_), _) => format!("{}- {text}\n", "  ".repeat(level)),
                    (None, style @ ("h1" | "h2" | "h3" | "h4" | "h5" | "h6")) => {
                        format!("{} {text}\n\n", "#".repeat(style[1..].parse().unwrap_or(2)))
                    }
                    (None, "blockquote") => format!("> {text}\n\n"),
                    (None, _) => format!("{text}\n\n"),
                };
                out.push_str(&line);
            }
            "image" => {
                if let Some(url) = sanity.and_then(|s| sanity_image(s, block)) {
                    let alt = block["alt"].as_str().unwrap_or("");
                    out.push_str(&format!("![{}]({url})\n\n", escape_md(alt)));
                    images.push(url);
                }
            }
            "code" => {
                let lang = block["language"].as_str().unwrap_or("");
                out.push_str(&format!("```{lang}\n{}\n```\n\n", block["code"].as_str().unwrap_or("").trim_end()));
            }
            _ => {}
        }
    }
    out.trim_end().to_string() + "\n"
}

/// `image-abc123-1200x800-png` to its URL on Sanity's CDN.
fn sanity_image(s: &Sanity, block: &Value) -> Option<String> {
    let reference = block["asset"]["_ref"].as_str().or(block["asset"]["url"].as_str())?;
    if reference.starts_with("https://") {
        return Some(reference.to_string());
    }
    let rest = reference.strip_prefix("image-")?;
    let (stem, ext) = rest.rsplit_once('-')?;
    Some(format!("https://cdn.sanity.io/images/{}/{}/{stem}.{ext}", s.project, s.dataset))
}

/// Contentful Rich Text to Markdown.
pub fn rich_text(node: &Value, images: &mut Vec<String>) -> String {
    fn inline(node: &Value) -> String {
        match node["nodeType"].as_str().unwrap_or("") {
            "text" => {
                let mut text = escape_md(node["value"].as_str().unwrap_or(""));
                if text.trim().is_empty() {
                    return text;
                }
                for mark in node["marks"].as_array().into_iter().flatten().filter_map(|m| m["type"].as_str()) {
                    text = match mark {
                        "bold" => format!("**{text}**"),
                        "italic" => format!("*{text}*"),
                        "code" => format!("`{}`", node["value"].as_str().unwrap_or("").replace('`', "")),
                        "strikethrough" => format!("~~{text}~~"),
                        _ => text,
                    };
                }
                text
            }
            "hyperlink" => format!("[{}]({})", children_inline(node), node["data"]["uri"].as_str().unwrap_or("")),
            _ => children_inline(node),
        }
    }
    fn children_inline(node: &Value) -> String {
        node["content"].as_array().into_iter().flatten().map(inline).collect()
    }
    fn block(node: &Value, out: &mut String, images: &mut Vec<String>, list: Option<(bool, usize)>) {
        let kind = node["nodeType"].as_str().unwrap_or("");
        let children = || node["content"].as_array().into_iter().flatten();
        match kind {
            "document" => children().for_each(|c| block(c, out, images, None)),
            "paragraph" => out.push_str(&format!("{}\n\n", children_inline(node))),
            h if h.starts_with("heading-") => {
                out.push_str(&format!("{} {}\n\n", "#".repeat(h[8..].parse().unwrap_or(2)), children_inline(node)));
            }
            "blockquote" => {
                let mut inner = String::new();
                children().for_each(|c| block(c, &mut inner, images, None));
                for line in inner.trim_end().lines() {
                    out.push_str(&format!("> {line}\n").replace("> \n", ">\n"));
                }
                out.push('\n');
            }
            "hr" => out.push_str("---\n\n"),
            "unordered-list" | "ordered-list" => {
                let depth = list.map_or(0, |(_, d)| d + 1);
                children().for_each(|c| block(c, out, images, Some((kind == "ordered-list", depth))));
                if depth == 0 {
                    out.push('\n');
                }
            }
            "list-item" => {
                let (ordered, depth) = list.unwrap_or((false, 0));
                let indent = if ordered { "   " } else { "  " }.repeat(depth);
                let marker = if ordered { "1. " } else { "- " };
                let mut first = true;
                for child in children() {
                    match child["nodeType"].as_str() {
                        Some("unordered-list" | "ordered-list") => block(child, out, images, Some((ordered, depth))),
                        _ => {
                            let text = children_inline(child);
                            if first {
                                out.push_str(&format!("{indent}{marker}{text}\n"));
                                first = false;
                            } else {
                                out.push_str(&format!("{indent}  {text}\n"));
                            }
                        }
                    }
                }
            }
            "embedded-asset-block" => {
                let asset = &node["data"]["asset"];
                if let Some(url) = asset["file"]["url"].as_str() {
                    let url = if url.starts_with("//") { format!("https:{url}") } else { url.to_string() };
                    let alt = asset["description"].as_str().or(asset["title"].as_str()).unwrap_or("");
                    out.push_str(&format!("![{}]({url})\n\n", escape_md(alt)));
                    images.push(url);
                }
            }
            _ => {}
        }
    }
    let mut out = String::new();
    block(node, &mut out, images, None);
    out.trim_end().to_string() + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_portable_text() {
        let blocks = json!([
            {"_type": "block", "style": "h2", "children": [{"_type": "span", "text": "Hours"}], "markDefs": []},
            {"_type": "block", "style": "normal", "markDefs": [{"_key": "k1", "_type": "link", "href": "https://x.dev"}],
             "children": [{"_type": "span", "text": "Open ", "marks": []}, {"_type": "span", "text": "daily", "marks": ["strong"]},
                          {"_type": "span", "text": ", see ", "marks": []}, {"_type": "span", "text": "map", "marks": ["k1"]}, {"_type": "span", "text": " *now*", "marks": []}]},
            {"_type": "block", "listItem": "bullet", "level": 1, "children": [{"_type": "span", "text": "Mon"}]},
            {"_type": "block", "listItem": "bullet", "level": 2, "children": [{"_type": "span", "text": "9 to 5"}]},
            {"_type": "image", "alt": "Front door", "asset": {"_ref": "image-abc123-1200x800-jpg"}},
            {"_type": "code", "language": "js", "code": "let a = 1"}
        ]);
        let sanity = Sanity { project: "p1".into(), dataset: "production".into(), query: String::new(), token_env: None };
        let mut images = Vec::new();
        let md = portable_text(blocks.as_array().unwrap(), Some(&sanity), &mut images);
        assert_eq!(
            md,
            "## Hours\n\nOpen **daily**, see [map](https://x.dev) \\*now\\*\n\n- Mon\n  - 9 to 5\n\n![Front door](https://cdn.sanity.io/images/p1/production/abc123-1200x800.jpg)\n\n```js\nlet a = 1\n```\n"
        );
        assert_eq!(images, ["https://cdn.sanity.io/images/p1/production/abc123-1200x800.jpg"]);
    }

    #[test]
    fn converts_contentful_rich_text_and_links() {
        let includes = json!({
            "Asset": {"a1": {"title": "Door", "description": "The front door", "file": {"url": "//images.ctfassets.net/s/a1/door.png"}}},
            "Entry": {"e1": {"name": "Ada"}}
        });
        let item = json!({
            "title": "Hello",
            "author": {"sys": {"type": "Link", "linkType": "Entry", "id": "e1"}},
            "cover": {"sys": {"type": "Link", "linkType": "Asset", "id": "a1"}},
            "body": {"nodeType": "document", "content": [
                {"nodeType": "heading-2", "content": [{"nodeType": "text", "value": "Menu", "marks": []}]},
                {"nodeType": "paragraph", "content": [
                    {"nodeType": "text", "value": "Fresh ", "marks": []},
                    {"nodeType": "text", "value": "bread", "marks": [{"type": "italic"}]},
                    {"nodeType": "hyperlink", "data": {"uri": "https://x.dev"}, "content": [{"nodeType": "text", "value": " daily", "marks": []}]}
                ]},
                {"nodeType": "ordered-list", "content": [
                    {"nodeType": "list-item", "content": [{"nodeType": "paragraph", "content": [{"nodeType": "text", "value": "One", "marks": []}]}]},
                    {"nodeType": "list-item", "content": [{"nodeType": "paragraph", "content": [{"nodeType": "text", "value": "Two", "marks": [{"type": "bold"}]}]}]}
                ]},
                {"nodeType": "embedded-asset-block", "data": {"target": {"sys": {"type": "Link", "linkType": "Asset", "id": "a1"}}}, "content": []}
            ]}
        });
        let item = resolve_links(item, &includes, 0);
        assert_eq!(item["author"], json!({"name": "Ada"}));
        assert_eq!(item["cover"]["url"], "https://images.ctfassets.net/s/a1/door.png");
        let mut images = Vec::new();
        let md = rich_text(&item["body"], &mut images);
        assert_eq!(
            md,
            "## Menu\n\nFresh *bread*[ daily](https://x.dev)\n\n1. One\n1. **Two**\n\n![The front door](https://images.ctfassets.net/s/a1/door.png)\n"
        );
        assert_eq!(images.len(), 1);
    }

    #[test]
    fn checks_sources() {
        let source = |v: Value| -> Source { serde_json::from_value(v).unwrap() };
        source(json!({"sanity": {"project": "abc", "dataset": "production", "query": "*"}})).check("posts").unwrap();
        for bad in [
            json!({}),
            json!({"sanity": {"project": "abc", "dataset": "p", "query": "*"}, "json": {"url": "https://x.dev"}}),
            json!({"json": {"url": "http://x.dev"}}),
            json!({"json": {"url": "https://user:pass@x.dev"}}),
            json!({"contentful": {"space": "s/../x", "content_type": "post", "token_env": "T"}}),
        ] {
            assert!(source(bad.clone()).check("posts").is_err(), "{bad}");
        }
        assert_eq!(encode("*[_type == \"post\"]"), "%2A%5B_type%20%3D%3D%20%22post%22%5D");
    }
}
