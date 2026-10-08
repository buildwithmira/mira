//! `mira dev`: builds into `.mira/dev`, serves it, and rebuilds on change.
//! Pages long poll `/_mira/wait` and reload once the build id changes.

use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use mira_compiler::{BuildOptions, DEV_JS, OVERLAY_CSS, build};
use notify::{RecursiveMode, Watcher};
use serde_json::{Value, json};
use tiny_http::{Header, Request, Response, Server};

use crate::diagnostic::Diagnostic;
use crate::ui;

/// What pages see when they poll: a version bumped on every build attempt,
/// the id of the last good build, and the current build error if any.
#[derive(Default)]
struct Status {
    version: u64,
    build_id: String,
    error: Option<Value>,
}

struct State {
    status: Mutex<Status>,
    changed: Condvar,
}

impl State {
    fn update(&self, f: impl FnOnce(&mut Status)) {
        let mut status = self.status.lock().unwrap();
        status.version += 1;
        f(&mut status);
        self.changed.notify_all();
    }
}

pub fn run(root: &Path, port: u16) -> Result<()> {
    let root = std::path::absolute(root)?;
    let out = root.join(".mira").join("dev");
    let opts = BuildOptions { root: root.clone(), out: out.clone(), dev: true, host_config: false };
    ui::header("dev", &root.display().to_string());

    let state = Arc::new(State { status: Mutex::new(Status::default()), changed: Condvar::new() });
    let first = build(&opts);

    let server = Server::http(("127.0.0.1", port))
        .map_err(|e| anyhow::anyhow!("could not listen on port {port}: {e}\nhint: pass --port to use another port"))?;
    let url = format!("http://localhost:{port}");
    match first {
        Ok(report) => {
            state.update(|s| s.build_id = report.build_id.clone());
            eprintln!(
                "  {}  {}   {}",
                ui::bold("ready"),
                url,
                ui::dim(&format!("{} pages in {}", report.pages.len(), ui::ms(report.duration_ms)))
            );
        }
        Err(err) => {
            eprintln!("  {}  {}   {}", ui::bold("serving"), url, ui::dim("last build failed"));
            eprintln!();
            ui::error(&err, &root);
            let diagnostic = serde_json::to_value(Diagnostic::from_error(&err, &root)).ok();
            state.update(|s| s.error = diagnostic);
        }
    }
    eprintln!("  {}", ui::dim("watching for changes  ·  ctrl+c to stop"));
    eprintln!();

    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(tx)?;
    watcher.watch(&root, RecursiveMode::Recursive).context("watching the project")?;

    let watch_state = state.clone();
    let watch_root = root.clone();
    std::thread::spawn(move || {
        let ignored = |p: &Path| {
            let rel = p.strip_prefix(&watch_root).unwrap_or(p);
            matches!(
                rel.components().next(),
                Some(Component::Normal(first)) if [".mira", "dist", ".git", "node_modules", "target"].iter().any(|i| first == *i)
            )
        };
        while let Ok(event) = rx.recv() {
            let mut changed: Vec<PathBuf> = Vec::new();
            let mut collect = |event: notify::Result<notify::Event>| {
                if let Ok(event) = event {
                    changed.extend(event.paths.into_iter().filter(|p| !ignored(p)));
                }
            };
            collect(event);
            // Editors save in bursts; settle before rebuilding.
            while let Ok(event) = rx.recv_timeout(Duration::from_millis(60)) {
                collect(event);
            }
            let Some(first) = changed.first() else { continue };
            let what = first.strip_prefix(&watch_root).unwrap_or(first).display().to_string().replace('\\', "/");
            match build(&opts) {
                Ok(report) => {
                    eprintln!("  {} rebuilt {} pages in {}  {}", ui::DOT, report.pages.len(), ui::ms(report.duration_ms), ui::dim(&what));
                    watch_state.update(|s| {
                        s.build_id = report.build_id.clone();
                        s.error = None;
                    });
                }
                Err(err) => {
                    eprintln!();
                    ui::error(&err, &watch_root);
                    let diagnostic = serde_json::to_value(Diagnostic::from_error(&err, &watch_root)).ok();
                    watch_state.update(|s| s.error = diagnostic);
                }
            }
        }
    });

    for request in server.incoming_requests() {
        let state = state.clone();
        let out = out.clone();
        std::thread::spawn(move || {
            let _ = respond(request, &state, &out);
        });
    }
    Ok(())
}

fn respond(request: Request, state: &State, out: &Path) -> std::io::Result<()> {
    let url = request.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((&url, ""));

    if path == "/_mira/wait" {
        // An unknown version (a fresh page) answers immediately.
        let seen: Option<u64> = query.strip_prefix("v=").and_then(|v| v.parse().ok());
        let guard = state.status.lock().unwrap();
        let (guard, _) = state.changed.wait_timeout_while(guard, Duration::from_secs(25), |s| Some(s.version) == seen).unwrap();
        let body = json!({ "version": guard.version, "build_id": guard.build_id, "error": guard.error }).to_string();
        drop(guard);
        return request.respond(
            Response::from_string(body)
                .with_header(header("Content-Type", "application/json"))
                .with_header(header("Cache-Control", "no-store")),
        );
    }
    if path == "/_mira/dev.js" || path == "/_mira/overlay.css" {
        let (body, kind) =
            if path.ends_with(".js") { (DEV_JS, "text/javascript; charset=utf-8") } else { (OVERLAY_CSS, "text/css; charset=utf-8") };
        return request.respond(
            Response::from_string(body).with_header(header("Content-Type", kind)).with_header(header("Cache-Control", "no-cache")),
        );
    }

    let Some(file) = resolve(out, path) else {
        return request.respond(Response::from_string("Bad request").with_status_code(400));
    };
    if file.is_dir() {
        let location = format!("{}/", path.trim_end_matches('/'));
        return request.respond(Response::empty(308).with_header(header("Location", &location)));
    }
    if !file.is_file() && !out.join("index.html").is_file() {
        // No output yet because the first build failed: serve a page that
        // shows the error overlay and reloads once a build succeeds.
        return request.respond(
            Response::from_string(ERROR_PAGE).with_status_code(500).with_header(header("Content-Type", "text/html; charset=utf-8")),
        );
    }
    let (file, status) = if file.is_file() { (file, 200) } else { (out.join("404.html"), 404) };
    match std::fs::read(&file) {
        Ok(bytes) => request.respond(
            Response::from_data(bytes)
                .with_status_code(status)
                .with_header(header("Content-Type", content_type(&file)))
                .with_header(header("Cache-Control", "no-cache")),
        ),
        Err(_) => request.respond(Response::from_string("Not found").with_status_code(404)),
    }
}

const ERROR_PAGE: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="mira-build" content="">
<title>Build failed · Mira</title>
<script type="module" src="/_mira/dev.js"></script>
</head>
<body style="margin:0;min-height:100vh;background:#0b0b0d;color:#a8a8b0;font:500 12px/1.4 'Geist Mono',ui-monospace,monospace;letter-spacing:.08em;text-transform:uppercase;display:grid;place-items:center">
<p>Build failed &middot; waiting for a fix</p>
</body>
</html>
"#;

/// Maps a URL path to a file under `out`, rejecting traversal.
fn resolve(out: &Path, path: &str) -> Option<PathBuf> {
    let decoded = percent_decode(path)?;
    let mut file = out.to_path_buf();
    for segment in decoded.split('/').filter(|s| !s.is_empty()) {
        if segment == ".." || segment == "." || segment.contains('\\') || segment.contains(':') {
            return None;
        }
        file.push(segment);
    }
    if decoded.ends_with('/') {
        file.push("index.html");
    }
    Some(file)
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "woff2" => "font/woff2",
        "txt" | "md" => "text/plain; charset=utf-8",
        "xml" => "application/xml",
        _ => "application/octet-stream",
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_paths_safely() {
        let out = Path::new("out");
        assert_eq!(resolve(out, "/"), Some(out.join("index.html")));
        assert_eq!(resolve(out, "/posts/a%20b/"), Some(out.join("posts").join("a b").join("index.html")));
        assert_eq!(resolve(out, "/../secret"), None);
        assert_eq!(resolve(out, "/%2e%2e/secret"), None);
    }
}
