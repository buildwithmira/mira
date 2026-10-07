//! Host adapters: the native config each host reads, written by
//! `mira build` for every host listed under `hosts` in `mira.config.json`.
//!
//! A host that clones the repository looks for its config at the project
//! root and needs to be told the site is already built in the output
//! folder. Every adapter carries the same behavior, expressed in that
//! host's own format:
//!
//! - security headers on every response
//! - `text/markdown` for Markdown twins, and a page's twin for requests
//!   that accept `text/markdown`, where the host can route on headers
//! - immutable caching for `/fonts/` and `/media/`
//! - trailing slash URLs and the site's own 404 page
//! - the redirects listed under `redirects`

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Result, bail};
use serde_json::{Value, json};

use crate::config::Config;
use crate::outputs::{IMMUTABLE, IMMUTABLE_DIRS, MARKDOWN, security_headers};

/// Every host Mira writes config for.
pub const HOSTS: [&str; 10] = ["vercel", "netlify", "cloudflare", "github", "firebase", "render", "azure", "docker", "deno", "s3"];

/// Marks the files Mira writes, so a hand written file is never overwritten.
/// Hosts reject unknown keys in their JSON configs, so those are recognized
/// by the Permissions-Policy value Mira writes instead.
const STAMP: &str = "Written by mira build from mira.config.json";
const JSON_FINGERPRINT: &str = "interest-cohort=()";
const ACCEPT_MARKDOWN: &str = "(.*)text/markdown(.*)";

fn written_by_mira(contents: &str) -> bool {
    contents.contains(STAMP) || contents.contains(JSON_FINGERPRINT)
}

/// Where a file goes: the project root, or inside the output folder.
pub enum Place {
    Root,
    Output,
}

pub struct HostFile {
    pub host: &'static str,
    pub name: &'static str,
    pub place: Place,
    pub contents: String,
}

/// A redirect from an old path to a new path or URL.
pub struct Redirect<'a> {
    pub from: &'a str,
    pub to: &'a str,
}

pub fn redirects(config: &Config) -> Vec<Redirect<'_>> {
    config.redirects.iter().map(|(from, to)| Redirect { from, to }).collect()
}

/// The files for every configured host. `out_dir` is the output folder
/// relative to the project root, with forward slashes.
pub fn files(config: &Config, out_dir: &str) -> Result<Vec<HostFile>> {
    let mut files = Vec::new();
    let mut add = |host: &'static str, name: &'static str, place: Place, contents: String| {
        files.push(HostFile { host, name, place, contents });
    };
    let setting = |host: &str, key: &str| config.hosts.get(host).and_then(|s| s.get(key)).and_then(Value::as_str).map(str::to_string);
    let name = |host: &str, key: &str| setting(host, key).unwrap_or_else(|| crate::content::slugify(&config.site.title));
    let mut wants_redirects_file = false;
    for host in config.hosts.keys() {
        match host.as_str() {
            "vercel" => add("vercel", "vercel.json", Place::Root, vercel(config, Some(out_dir))),
            "netlify" => {
                add("netlify", "netlify.toml", Place::Root, netlify(out_dir));
                wants_redirects_file = true;
            }
            "cloudflare" => {
                add("cloudflare", "wrangler.toml", Place::Root, cloudflare(&name("cloudflare", "project"), out_dir));
                wants_redirects_file = true;
            }
            "github" => add("github", ".github/workflows/mira-pages.yml", Place::Root, github(out_dir)),
            "firebase" => add("firebase", "firebase.json", Place::Root, firebase(config, out_dir)),
            "render" => add("render", "render.yaml", Place::Root, render(config, out_dir, &name("render", "service"))),
            "azure" => add("azure", "staticwebapp.config.json", Place::Output, azure(config)),
            "docker" => {
                add("docker", "nginx.conf", Place::Root, nginx(config));
                add("docker", "Dockerfile", Place::Root, dockerfile(out_dir));
            }
            "deno" => add("deno", "main.ts", Place::Root, deno(config, out_dir)),
            "s3" => add("s3", "cloudfront-function.js", Place::Root, cloudfront(config)),
            other => bail!("mira.config.json: hosts.{other} is not a host Mira knows\nhint: use one or more of {}", HOSTS.join(", ")),
        }
    }
    if wants_redirects_file && !config.redirects.is_empty() {
        add("netlify", "_redirects", Place::Output, redirects_file(config));
    }
    Ok(files)
}

/// Writes root level host files, refusing to replace a file Mira did not
/// write. Returns the names written.
pub fn write_root(files: &[HostFile], root: &Path) -> Result<Vec<String>> {
    let mut written = Vec::new();
    for file in files.iter().filter(|f| matches!(f.place, Place::Root)) {
        let path = root.join(file.name);
        if let Ok(existing) = std::fs::read_to_string(&path) {
            if existing == file.contents {
                written.push(file.name.to_string());
                continue;
            }
            if !written_by_mira(&existing) {
                bail!(
                    "{}: exists and was not written by Mira, so hosts.{} cannot manage it\nhint: delete or rename it, or remove {} from hosts in mira.config.json",
                    file.name,
                    file.host,
                    file.host
                );
            }
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &file.contents)?;
        written.push(file.name.to_string());
    }
    Ok(written)
}

/// An HTML page that sends readers and crawlers to `to`, for hosts with no
/// redirect config. It carries a canonical link so search engines move
/// ranking to the new URL.
pub fn redirect_page(to: &str) -> String {
    let to = crate::html::escape(to);
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>Moved</title>\n<meta http-equiv=\"refresh\" content=\"0; url={to}\">\n<link rel=\"canonical\" href=\"{to}\">\n<meta name=\"robots\" content=\"noindex\">\n</head>\n<body>\n<p>This page moved to <a href=\"{to}\">{to}</a>.</p>\n</body>\n</html>\n"
    )
}

// ------------------------------------------------------------------ Vercel

/// Vercel config. Rewrites in `vercel.json` only apply when no file
/// matches, which would skip Markdown negotiation for every page, so this
/// uses ordered `routes`: headers, negotiation, redirects, and trailing
/// slashes run before the filesystem, then the 404 page. `routes` cannot be
/// combined with `headers`, `rewrites`, or `trailingSlash`, so all of it is
/// expressed here.
pub fn vercel(config: &Config, out_dir: Option<&str>) -> String {
    let security: serde_json::Map<String, Value> = security_headers(config).into_iter().map(|(k, v)| (k.to_string(), json!(v))).collect();
    let mut routes = vec![
        json!({ "src": "^/(.*)$", "headers": security, "continue": true }),
        json!({ "src": "^/(?:.*/)?$", "headers": { "Vary": "Accept" }, "continue": true }),
        json!({ "src": "^/(.*)\\.md$", "headers": { "Content-Type": MARKDOWN }, "continue": true }),
    ];
    for dir in IMMUTABLE_DIRS {
        routes.push(json!({ "src": format!("^/{dir}/(.*)$"), "headers": { "Cache-Control": IMMUTABLE }, "continue": true }));
    }
    for r in redirects(config) {
        routes.push(
            json!({ "src": format!("^{}/?$", regex_escape(r.from.trim_end_matches('/'))), "status": 308, "headers": { "Location": r.to } }),
        );
    }
    if config.agents.twins {
        let accept = json!([{ "type": "header", "key": "accept", "value": ACCEPT_MARKDOWN }]);
        routes.push(json!({ "src": "^/$", "has": accept, "dest": "/index.md" }));
        routes.push(json!({ "src": "^/(.+)/$", "has": accept, "dest": "/$1.md" }));
    }
    // A path without a slash or a file extension is a page: add the slash.
    routes.push(json!({ "src": "^/((?:[^/]+/)*[^/.]+)$", "status": 308, "headers": { "Location": "/$1/" } }));
    routes.push(json!({ "handle": "filesystem" }));
    routes.push(json!({ "src": "^/(.*)$", "status": 404, "dest": "/404.html" }));

    let mut value = json!({ "$schema": "https://openapi.vercel.sh/vercel.json" });
    if let Some(out) = out_dir {
        value["framework"] = Value::Null;
        value["outputDirectory"] = json!(out);
    }
    value["routes"] = json!(routes);
    serde_json::to_string_pretty(&value).unwrap_or_default() + "\n"
}

// ------------------------------------------------- Netlify and Cloudflare

fn netlify(out_dir: &str) -> String {
    format!(
        "# {STAMP}.\n# The site is built locally with mira build; Netlify publishes {out_dir}/ as is.\n# Headers come from {out_dir}/_headers and redirects from {out_dir}/_redirects.\n\n[build]\n  publish = \"{out_dir}\"\n  command = \"\"\n"
    )
}

fn cloudflare(name: &str, out_dir: &str) -> String {
    format!(
        "# {STAMP}.\n# Cloudflare Pages publishes {out_dir}/ as is; headers come from {out_dir}/_headers\n# and redirects from {out_dir}/_redirects. name must match the Pages project;\n# set hosts.cloudflare.project to change it.\n\nname = \"{name}\"\npages_build_output_dir = \"{out_dir}\"\ncompatibility_date = \"2026-10-01\"\n"
    )
}

/// `_redirects`, read by Netlify and Cloudflare Pages. `!` forces the rule
/// even when a file exists at the old path.
fn redirects_file(config: &Config) -> String {
    let mut out = format!("# {STAMP}.\n");
    for r in redirects(config) {
        out.push_str(&format!("{} {} 301!\n", r.from, r.to));
        if r.from.ends_with('/') && r.from.len() > 1 {
            out.push_str(&format!("{} {} 301!\n", r.from.trim_end_matches('/'), r.to));
        }
    }
    out
}

// ---------------------------------------------------------- GitHub Pages

fn github(out_dir: &str) -> String {
    format!(
        "# {STAMP}.
# Publishes the committed {out_dir}/ to GitHub Pages. Build with mira build
# and commit {out_dir}/; GitHub's runners do not build the site.
name: Deploy to GitHub Pages

on:
  push:
    branches: [main]
    paths: [\"{out_dir}/**\"]
  workflow_dispatch:

permissions:
  contents: read
  pages: write
  id-token: write

concurrency:
  group: pages
  cancel-in-progress: false

jobs:
  deploy:
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{{{ steps.deployment.outputs.page_url }}}}
    steps:
      - uses: actions/checkout@v4
      - name: Check the build output
        run: test -f {out_dir}/index.html && test -f {out_dir}/.mira-output
      - uses: actions/configure-pages@v5
        with:
          enablement: true
      - uses: actions/upload-pages-artifact@v3
        with:
          path: {out_dir}
      - id: deployment
        uses: actions/deploy-pages@v4
"
    )
}

// -------------------------------------------------------------- Firebase

fn firebase(config: &Config, out_dir: &str) -> String {
    let all: Vec<Value> = security_headers(config).into_iter().map(|(k, v)| json!({ "key": k, "value": v })).collect();
    let mut headers = vec![
        json!({ "source": "**", "headers": all }),
        json!({ "source": "**/*.md", "headers": [{ "key": "Content-Type", "value": MARKDOWN }] }),
    ];
    for dir in IMMUTABLE_DIRS {
        headers.push(json!({ "source": format!("/{dir}/**"), "headers": [{ "key": "Cache-Control", "value": IMMUTABLE }] }));
    }
    let redirects: Vec<Value> = redirects(config)
        .iter()
        .map(|r| json!({ "source": r.from.trim_end_matches('/').to_string() + "{,/}", "destination": r.to, "type": 301 }))
        .collect();
    let value = json!({
        "hosting": {
            "public": out_dir,
            "trailingSlash": true,
            "ignore": ["**/.*"],
            "headers": headers,
            "redirects": redirects,
        }
    });
    serde_json::to_string_pretty(&value).unwrap_or_default() + "\n"
}

// ---------------------------------------------------------------- Render

fn render(config: &Config, out_dir: &str, name: &str) -> String {
    let mut out = format!(
        "# {STAMP}.\n# A Render static site that publishes {out_dir}/ as is.\n\nservices:\n  - type: web\n    runtime: static\n    name: {name}\n    buildCommand: \"\"\n    staticPublishPath: ./{out_dir}\n    headers:\n"
    );
    for (key, value) in security_headers(config) {
        out.push_str(&format!("      - path: /*\n        name: {key}\n        value: \"{}\"\n", value.replace('"', "\\\"")));
    }
    out.push_str(&format!("      - path: /*.md\n        name: Content-Type\n        value: \"{MARKDOWN}\"\n"));
    for dir in IMMUTABLE_DIRS {
        out.push_str(&format!("      - path: /{dir}/*\n        name: Cache-Control\n        value: \"{IMMUTABLE}\"\n"));
    }
    if !config.redirects.is_empty() {
        out.push_str("    routes:\n");
        for r in redirects(config) {
            out.push_str(&format!("      - type: redirect\n        source: {}\n        destination: {}\n", r.from, r.to));
        }
    }
    out
}

// ----------------------------------------------------------------- Azure

fn azure(config: &Config) -> String {
    let global: serde_json::Map<String, Value> = security_headers(config).into_iter().map(|(k, v)| (k.to_string(), json!(v))).collect();
    let mut routes: Vec<Value> =
        redirects(config).iter().map(|r| json!({ "route": r.from, "redirect": r.to, "statusCode": 301 })).collect();
    routes.extend(IMMUTABLE_DIRS.iter().map(|dir| json!({ "route": format!("/{dir}/*"), "headers": { "Cache-Control": IMMUTABLE } })));
    let value = json!({
        "trailingSlash": "always",
        "globalHeaders": global,
        "mimeTypes": { ".md": MARKDOWN, ".avif": "image/avif", ".woff2": "font/woff2", ".txt": "text/plain; charset=utf-8" },
        "responseOverrides": { "404": { "rewrite": "/404.html" } },
        "routes": routes,
    });
    serde_json::to_string_pretty(&value).unwrap_or_default() + "\n"
}

// ---------------------------------------------------------- Docker, nginx

fn nginx(config: &Config) -> String {
    let mut headers = String::new();
    for (key, value) in security_headers(config) {
        headers.push_str(&format!("    add_header {key} \"{}\" always;\n", value.replace('"', "\\\"")));
    }
    let mut redirects_block = String::new();
    for r in redirects(config) {
        let from = r.from.trim_end_matches('/');
        redirects_block.push_str(&format!("    location ~ ^{}/?$ {{ return 301 {}; }}\n", regex_escape(from), r.to));
    }
    // Requests that accept text/markdown get the page's Markdown twin.
    let accepts_markdown = if config.agents.twins { "    \"~*text/markdown\" 1;\n" } else { "" };
    format!(
        "# {STAMP}.
# nginx serving the Mira output folder. Used by the Dockerfile; also works on
# any server that runs nginx.

types {{
    text/markdown md;
    image/avif avif;
}}

map $http_accept $mira_twin {{
    default 0;
{accepts_markdown}}}

server {{
    listen 8080;
    root /usr/share/nginx/html;
    absolute_redirect off;
    charset utf-8;
    charset_types text/markdown text/plain application/json;

{headers}    add_header Vary Accept always;

    location ~ ^/(fonts|media)/ {{
{headers}        add_header Cache-Control \"{IMMUTABLE}\" always;
        try_files $uri =404;
    }}

{redirects_block}    location = / {{
        if ($mira_twin) {{ rewrite ^ /index.md last; }}
        try_files /index.html =404;
    }}

    location / {{
        if ($mira_twin) {{ rewrite ^/(.+)/$ /$1.md last; }}
        try_files $uri $uri/index.html =404;
    }}

    error_page 404 /404.html;
}}
"
    )
}

fn dockerfile(out_dir: &str) -> String {
    format!(
        "# {STAMP}.
# Serves the committed {out_dir}/ with nginx on port 8080. Works on Fly.io,
# Railway, Render, Google Cloud Run, Kubernetes, and any host that runs
# containers. Build the site with mira build first.
FROM nginx:1.27-alpine
COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY {out_dir}/ /usr/share/nginx/html/
EXPOSE 8080
"
    )
}

// ------------------------------------------------------------ Deno Deploy

fn deno(config: &Config, out_dir: &str) -> String {
    let headers: Vec<String> = security_headers(config).into_iter().map(|(k, v)| format!("  [{k:?}, {v:?}],")).collect();
    let mut redirect_lines = String::new();
    for r in redirects(config) {
        redirect_lines.push_str(&format!("  [{:?}, {:?}],\n", r.from.trim_end_matches('/'), r.to));
    }
    format!(
        "// {STAMP}.
// Serves the committed {out_dir}/ on Deno Deploy, or locally with
// `deno run --allow-net --allow-read main.ts`.
import {{ serveDir }} from \"jsr:@std/http@1/file-server\";

const ROOT = \"{out_dir}\";
const SECURITY: [string, string][] = [
{}
];
const REDIRECTS = new Map<string, string>([
{redirect_lines}]);
const IMMUTABLE = {IMMUTABLE:?};

Deno.serve(async (request) => {{
  const url = new URL(request.url);
  const moved = REDIRECTS.get(url.pathname.replace(/\\/$/, \"\"));
  if (moved) return Response.redirect(new URL(moved, url), 301);
  if (!url.pathname.endsWith(\"/\") && !url.pathname.split(\"/\").pop()!.includes(\".\")) {{
    return Response.redirect(new URL(url.pathname + \"/\" + url.search, url), 308);
  }}
  const wantsMarkdown = {} && (request.headers.get(\"accept\") ?? \"\").includes(\"text/markdown\");
  if (wantsMarkdown && url.pathname.endsWith(\"/\")) {{
    url.pathname = url.pathname === \"/\" ? \"/index.md\" : url.pathname.slice(0, -1) + \".md\";
    request = new Request(url, request);
  }}
  let response = await serveDir(request, {{ fsRoot: ROOT, quiet: true }});
  if (response.status === 404) {{
    response = new Response(await Deno.readFile(`${{ROOT}}/404.html`), {{ status: 404, headers: {{ \"content-type\": \"text/html; charset=utf-8\" }} }});
  }}
  const headers = new Headers(response.headers);
  for (const [key, value] of SECURITY) headers.set(key, value);
  headers.append(\"vary\", \"accept\");
  if (url.pathname.endsWith(\".md\")) headers.set(\"content-type\", {MARKDOWN:?});
  if (/^\\/(fonts|media)\\//.test(url.pathname)) headers.set(\"cache-control\", IMMUTABLE);
  return new Response(response.body, {{ status: response.status, headers }});
}});
",
        headers.join("\n"),
        config.agents.twins
    )
}

// ------------------------------------------------------ S3 and CloudFront

/// A CloudFront viewer request function for a site in S3: directory URLs
/// map to their index.html, Markdown negotiation, trailing slashes, and
/// redirects. Security headers belong in a CloudFront response headers
/// policy; see the deploying docs.
fn cloudfront(config: &Config) -> String {
    let mut redirect_lines = String::new();
    for r in redirects(config) {
        redirect_lines.push_str(&format!("  {:?}: {:?},\n", r.from.trim_end_matches('/'), r.to));
    }
    format!(
        "// {STAMP}.
// CloudFront Functions (viewer request) for a Mira site stored in S3.
var REDIRECTS = {{
{redirect_lines}}};
var TWINS = {};

function handler(event) {{
  var request = event.request;
  var uri = request.uri;
  var moved = REDIRECTS[uri.replace(/\\/$/, \"\")];
  if (moved) {{
    return {{ statusCode: 301, statusDescription: \"Moved Permanently\", headers: {{ location: {{ value: moved }} }} }};
  }}
  var last = uri.split(\"/\").pop();
  if (!uri.endsWith(\"/\") && last.indexOf(\".\") === -1) {{
    return {{ statusCode: 308, statusDescription: \"Permanent Redirect\", headers: {{ location: {{ value: uri + \"/\" }} }} }};
  }}
  var accept = request.headers.accept ? request.headers.accept.value : \"\";
  if (TWINS && uri.endsWith(\"/\") && accept.indexOf(\"text/markdown\") !== -1) {{
    request.uri = uri === \"/\" ? \"/index.md\" : uri.slice(0, -1) + \".md\";
    return request;
  }}
  if (uri.endsWith(\"/\")) request.uri = uri + \"index.html\";
  return request;
}}
",
        config.agents.twins
    )
}

fn regex_escape(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        if ".+*?()[]{}|^$\\".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Validates `redirects` in config: paths start with `/`, targets are paths
/// or http(s) URLs, and nothing redirects to itself.
pub fn check_redirects(redirects: &BTreeMap<String, String>) -> Result<()> {
    for (from, to) in redirects {
        if !from.starts_with('/') || from.contains(['*', ' ', '"']) {
            bail!("mira.config.json: redirects key {from:?} must be a path starting with /, without wildcards");
        }
        if !(to.starts_with('/') || to.starts_with("https://") || to.starts_with("http://")) || to.contains([' ', '"']) {
            bail!("mira.config.json: redirects.{from} must point to a path or an http(s) URL");
        }
        if from.trim_end_matches('/') == to.trim_end_matches('/') {
            bail!("mira.config.json: redirects.{from} points to itself");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(hosts: &[&str]) -> Config {
        let mut config = Config::default();
        for h in hosts {
            config.hosts.insert(h.to_string(), json!({}));
        }
        config.redirects.insert("/old-post/".into(), "/posts/new-post/".into());
        config
    }

    #[test]
    fn vercel_routes_negotiate_before_the_filesystem() {
        let files = files(&config(&["vercel"]), "dist").unwrap();
        let v: Value = serde_json::from_str(&files[0].contents).unwrap();
        assert_eq!(v["outputDirectory"], "dist");
        let routes = v["routes"].as_array().unwrap();
        let filesystem = routes.iter().position(|r| r["handle"] == "filesystem").unwrap();
        let negotiation = routes.iter().position(|r| r["dest"] == "/$1.md").unwrap();
        let redirect = routes.iter().position(|r| r["headers"]["Location"] == "/posts/new-post/").unwrap();
        assert!(negotiation < filesystem && redirect < filesystem);
        assert!(v.get("headers").is_none() && v.get("rewrites").is_none() && v.get("trailingSlash").is_none());
        assert_eq!(routes.last().unwrap()["dest"], "/404.html");
    }

    #[test]
    fn every_host_writes_config() {
        let files = files(&config(&HOSTS), "dist").unwrap();
        for host in HOSTS {
            assert!(files.iter().any(|f| f.host == host), "{host}");
        }
        for file in &files {
            assert!(written_by_mira(&file.contents), "{}", file.name);
            if file.name.ends_with(".json") {
                serde_json::from_str::<Value>(&file.contents).unwrap_or_else(|e| panic!("{}: {e}", file.name));
            }
        }
        let redirects = files.iter().find(|f| f.name == "_redirects").unwrap();
        assert!(redirects.contents.contains("/old-post/ /posts/new-post/ 301!"), "{}", redirects.contents);
        assert!(redirects.contents.contains("/old-post /posts/new-post/ 301!"), "{}", redirects.contents);
        let nginx = files.iter().find(|f| f.name == "nginx.conf").unwrap();
        assert!(nginx.contents.contains("return 301 /posts/new-post/;"), "{}", nginx.contents);
    }

    #[test]
    fn unknown_hosts_fail() {
        let err = files(&config(&["heroku"]), "dist").err().unwrap().to_string();
        assert!(err.contains("hosts.heroku"), "{err}");
    }

    #[test]
    fn validates_redirects() {
        let mut r = BTreeMap::new();
        r.insert("/a/".to_string(), "/b/".to_string());
        assert!(check_redirects(&r).is_ok());
        r.insert("no-slash".to_string(), "/b/".to_string());
        assert!(check_redirects(&r).is_err());
        let mut loops = BTreeMap::new();
        loops.insert("/a/".to_string(), "/a".to_string());
        assert!(check_redirects(&loops).is_err());
    }

    #[test]
    fn never_replaces_a_hand_written_file() {
        let dir = std::env::temp_dir().join(format!("mira-hosts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _ = std::fs::remove_file(dir.join("vercel.json"));
        std::fs::write(dir.join("vercel.json"), "{\"mine\": true}").unwrap();
        let files = files(&config(&["vercel"]), "dist").unwrap();
        assert!(write_root(&files, &dir).is_err());
        std::fs::remove_file(dir.join("vercel.json")).unwrap();
        assert_eq!(write_root(&files, &dir).unwrap(), vec!["vercel.json"]);
        assert!(write_root(&files, &dir).is_ok());
    }
}
