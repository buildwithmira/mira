//! `mira mcp`: a Model Context Protocol server over stdio that exposes the
//! site to agents. It rebuilds into `.mira/mcp` before answering, so
//! answers always match the current source.
//!
//! Tools: `list_pages`, `read_page` (the Markdown twin), and `search`.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use mira_compiler::{BuildOptions, build};
use serde_json::{Value, json};

const PROTOCOL: &str = "2025-06-18";

pub fn run(root: &Path) -> Result<()> {
    let root = std::path::absolute(root)?;
    let out = root.join(".mira").join("mcp");
    eprintln!("mira mcp: serving {} over stdio", root.display());

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
                "instructions": "Read this Mira site. Call list_pages or search to find pages, then read_page for a page's Markdown."
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools() })),
            "tools/call" => Ok(call(&root, &out, params)),
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
    json!([
        {
            "name": "list_pages",
            "description": "List every page on the site with its URL, title, and description.",
            "inputSchema": { "type": "object", "properties": {} }
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
        }
    ])
}

fn call(root: &Path, out: &Path, params: &Value) -> Value {
    let result = (|| -> Result<String> {
        build(&BuildOptions { root: root.to_path_buf(), out: out.to_path_buf(), dev: false, host_config: false })?;
        let index: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(out.join("_mira/search.json"))?)?;
        let args = &params["arguments"];
        match params["name"].as_str().unwrap_or("") {
            "list_pages" => {
                let pages: Vec<Value> =
                    index.iter().map(|d| json!({ "url": d["url"], "title": d["title"], "description": d["description"] })).collect();
                Ok(serde_json::to_string_pretty(&pages)?)
            }
            "read_page" => {
                let url = args["url"].as_str().unwrap_or("/");
                let path = twin_path(out, url);
                std::fs::read_to_string(&path).map_err(|_| anyhow::anyhow!("no page at {url}; call list_pages to see every URL"))
            }
            "search" => {
                let query = args["query"].as_str().unwrap_or("");
                let limit = args["limit"].as_u64().unwrap_or(8).clamp(1, 25) as usize;
                Ok(serde_json::to_string_pretty(&search(&index, query, limit))?)
            }
            other => anyhow::bail!("unknown tool {other}"),
        }
    })();
    match result {
        Ok(text) => json!({ "content": [{ "type": "text", "text": text }] }),
        Err(err) => json!({ "content": [{ "type": "text", "text": err.to_string() }], "isError": true }),
    }
}

/// Maps `/docs/install/` to `<out>/docs/install.md` and `/` to `index.md`,
/// refusing paths that leave the output directory.
fn twin_path(out: &Path, url: &str) -> PathBuf {
    let clean: Vec<&str> = url.split(['/', '\\']).filter(|s| !s.is_empty() && *s != "." && *s != "..").collect();
    match clean.split_last() {
        None => out.join("index.md"),
        Some((last, dirs)) => {
            let mut path = out.to_path_buf();
            path.extend(dirs);
            path.push(format!("{}.md", last.trim_end_matches(".md")));
            path
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
        let out = Path::new("out");
        assert_eq!(twin_path(out, "/"), out.join("index.md"));
        assert_eq!(twin_path(out, "/docs/install/"), out.join("docs").join("install.md"));
        assert_eq!(twin_path(out, "/../../etc/passwd"), out.join("etc").join("passwd.md"));
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
}
