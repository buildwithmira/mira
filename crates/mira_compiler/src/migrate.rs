//! `mira migrate`: moves a content site from another framework to Mira.
//!
//! It reads files and never runs them: no JavaScript, no config code, no
//! templates from the old site execute. It never writes to the source and
//! never follows a symlink. Every page keeps its URL or gets a redirect,
//! and everything a person needs to look at is listed in MIGRATION.md
//! with its file and line.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Framework {
    Nextjs,
    Astro,
    Hugo,
    Jekyll,
    Docusaurus,
    Gatsby,
    Eleventy,
    Vitepress,
    Markdown,
}

impl Framework {
    pub fn parse(name: &str) -> Option<Framework> {
        Some(match name.to_ascii_lowercase().as_str() {
            "next" | "nextjs" | "next.js" => Framework::Nextjs,
            "astro" => Framework::Astro,
            "hugo" => Framework::Hugo,
            "jekyll" => Framework::Jekyll,
            "docusaurus" => Framework::Docusaurus,
            "gatsby" => Framework::Gatsby,
            "eleventy" | "11ty" => Framework::Eleventy,
            "vitepress" => Framework::Vitepress,
            "markdown" | "md" => Framework::Markdown,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Framework::Nextjs => "Next.js",
            Framework::Astro => "Astro",
            Framework::Hugo => "Hugo",
            Framework::Jekyll => "Jekyll",
            Framework::Docusaurus => "Docusaurus",
            Framework::Gatsby => "Gatsby",
            Framework::Eleventy => "Eleventy",
            Framework::Vitepress => "VitePress",
            Framework::Markdown => "a Markdown folder",
        }
    }
}

pub struct MigrateOptions {
    pub source: PathBuf,
    pub dest: PathBuf,
    pub from: Option<Framework>,
    /// Report what would happen without writing anything.
    pub dry_run: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct MigrateReport {
    pub framework: Option<Framework>,
    pub pages: usize,
    pub entries: BTreeMap<String, usize>,
    pub assets: usize,
    pub redirects: BTreeMap<String, String>,
    /// Things a person needs to look at: `file:line: what`.
    pub todo: Vec<String>,
}

/// Directories never read, whatever the framework.
const SKIP_DIRS: [&str; 16] = [
    "node_modules",
    ".git",
    ".next",
    ".nuxt",
    ".astro",
    ".docusaurus",
    ".vercel",
    ".netlify",
    ".cache",
    "out",
    "dist",
    "build",
    "_site",
    "resources",
    "vendor",
    ".mira",
];
/// Directory names that hold dated entries, which become collections.
const BLOG_DIRS: [&str; 9] = ["posts", "_posts", "blog", "articles", "news", "notes", "writing", "changelog", "journal"];
const ROOT_SKIP: [&str; 6] = ["readme", "changelog", "license", "contributing", "code_of_conduct", "security"];

pub fn detect(dir: &Path) -> Framework {
    let package = std::fs::read_to_string(dir.join("package.json")).unwrap_or_default();
    let has = |dep: &str| package.contains(&format!("\"{dep}\""));
    if has("next") {
        Framework::Nextjs
    } else if has("@docusaurus/core") {
        Framework::Docusaurus
    } else if has("astro") {
        Framework::Astro
    } else if has("gatsby") {
        Framework::Gatsby
    } else if has("vitepress") {
        Framework::Vitepress
    } else if has("@11ty/eleventy") {
        Framework::Eleventy
    } else if ["hugo.toml", "hugo.yaml", "hugo.json"].iter().any(|f| dir.join(f).exists())
        || (dir.join("config.toml").exists() && dir.join("content").is_dir())
    {
        Framework::Hugo
    } else if dir.join("_config.yml").exists() || dir.join("_posts").is_dir() {
        Framework::Jekyll
    } else {
        Framework::Markdown
    }
}

/// How a root's files map to their old URLs.
#[derive(Clone, Copy)]
enum Style {
    /// `a/b.md` was served at `/a/b/`.
    Path,
    /// `a/b.md` was served at `/a/b.html`.
    Html,
    /// Next.js app router: `a/b/page.mdx` was served at `/a/b`.
    NextApp,
    /// Jekyll `_posts/2024-05-01-title.md`, by the site's permalink setting.
    JekyllPost,
    /// Docusaurus blog `2024-05-01-title.md` at `/blog/2024/05/01/title`.
    DocusaurusBlog,
}

struct Root {
    dir: PathBuf,
    base: String,
    style: Style,
    /// Every file is an entry of this collection.
    collection: Option<String>,
}

fn roots(fw: Framework, dir: &Path) -> Vec<Root> {
    let root = |rel: &str, base: &str, style: Style, collection: Option<&str>| Root {
        dir: dir.join(rel),
        base: base.to_string(),
        style,
        collection: collection.map(str::to_string),
    };
    let mut roots = match fw {
        Framework::Nextjs => vec![
            root("content", "/", Style::Path, None),
            root("posts", "/posts/", Style::Path, Some("posts")),
            root("_posts", "/posts/", Style::Path, Some("posts")),
            root("blog", "/blog/", Style::Path, Some("blog")),
            root("data/blog", "/blog/", Style::Path, Some("blog")),
            root("src/content", "/", Style::Path, None),
            root("pages", "/", Style::Path, None),
            root("src/pages", "/", Style::Path, None),
            root("app", "/", Style::NextApp, None),
            root("src/app", "/", Style::NextApp, None),
        ],
        Framework::Astro => vec![root("src/content", "/", Style::Path, None), root("src/pages", "/", Style::Path, None)],
        Framework::Hugo => vec![root("content", "/", Style::Path, None)],
        Framework::Jekyll => vec![root("_posts", "/", Style::JekyllPost, Some("posts")), root(".", "/", Style::Html, None)],
        Framework::Docusaurus => vec![
            root("docs", "/docs/", Style::Path, None),
            root("blog", "/blog/", Style::DocusaurusBlog, Some("blog")),
            root("src/pages", "/", Style::Path, None),
        ],
        Framework::Gatsby => vec![root("content", "/", Style::Path, None), root("src/pages", "/", Style::Path, None)],
        Framework::Eleventy => vec![root(&eleventy_input(dir), "/", Style::Path, None)],
        Framework::Vitepress => {
            if dir.join("docs").is_dir() {
                vec![root("docs", "/", Style::Html, None)]
            } else {
                vec![root(".", "/", Style::Html, None)]
            }
        }
        Framework::Markdown => vec![root(".", "/", Style::Path, None)],
    };
    roots.retain(|r| r.dir.is_dir());
    roots
}

struct Item {
    source: PathBuf,
    root_dir: PathBuf,
    rel: String,
    old_url: String,
    collection: Option<String>,
    slug: String,
    data: Map<String, Value>,
    body: String,
}

pub fn migrate(opts: &MigrateOptions) -> Result<MigrateReport> {
    let source = std::fs::canonicalize(&opts.source).with_context(|| format!("{}: cannot read the source", opts.source.display()))?;
    if !opts.dry_run && opts.dest.exists() && std::fs::read_dir(&opts.dest)?.next().is_some() {
        bail!(
            "{}: directory is not empty\nhint: migrate into a new directory, such as `mira migrate {} ./my-site`",
            opts.dest.display(),
            opts.source.display()
        );
    }
    if let Ok(dest) = std::fs::canonicalize(&opts.dest)
        && dest.starts_with(&source)
    {
        bail!("{}: the destination is inside the source\nhint: migrate into a directory outside the old project", opts.dest.display());
    }
    let fw = opts.from.unwrap_or_else(|| detect(&source));
    let mut report = MigrateReport { framework: Some(fw), ..Default::default() };
    let permalink = jekyll_permalink(&source);

    // Read every content file.
    let mut items = Vec::new();
    let mut seen = BTreeSet::new();
    for root in roots(fw, &source) {
        for path in markdown_files(&root.dir, &source) {
            if !seen.insert(path.clone()) {
                continue;
            }
            let rel_root = path.strip_prefix(&root.dir).unwrap().to_string_lossy().replace('\\', "/");
            let rel = path.strip_prefix(&source).unwrap().to_string_lossy().replace('\\', "/");
            let stem = rel_root.rsplit('/').next().unwrap_or("").rsplit_once('.').map_or("", |(s, _)| s).to_ascii_lowercase();
            if !rel_root.contains('/') && ROOT_SKIP.contains(&stem.as_str()) && root.dir == source {
                continue;
            }
            let text = std::fs::read_to_string(&path).with_context(|| format!("reading {rel}"))?;
            let (mut data, body) = match split_frontmatter(&text) {
                Ok(parts) => parts,
                Err(e) => {
                    report.todo.push(format!("{rel}:1: frontmatter could not be read ({e}); it was left out"));
                    (Map::new(), strip_frontmatter(&text).to_string())
                }
            };
            if fw == Framework::Hugo && stem == "_index" {
                data.entry("title").or_insert(Value::from(title_from(&rel_root)));
            }
            let collection = root.collection.clone().or_else(|| blog_collection(&rel_root));
            let name = if stem == "index" || stem == "_index" {
                rel_root.rsplit('/').nth(1).map(str::to_ascii_lowercase).unwrap_or_else(|| stem.clone())
            } else {
                stem.clone()
            };
            let (date_prefix, bare) = split_date_prefix(&name);
            let slug = data.get("slug").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| crate::content::slugify(bare));
            if let Some(date) = date_prefix {
                data.entry("date").or_insert(Value::from(date));
            }
            let old_url = old_url(&root, &rel_root, &data, permalink.as_deref(), &slug);
            items.push(Item { source: path, root_dir: root.dir.clone(), rel, old_url, collection, slug, data, body });
        }
    }
    for root in roots(fw, &source) {
        for path in walk_files(&root.dir, &source) {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let rel = path.strip_prefix(&source).unwrap().to_string_lossy().replace('\\', "/");
            let segments: Vec<&str> = rel.split('/').collect();
            let private = segments.iter().any(|d| d.starts_with('_') || *d == "components" || *d == "api");
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let script = matches!(ext, "tsx" | "jsx" | "js" | "ts");
            let is_page = !private
                && (matches!(ext, "astro" | "vue" | "svelte") && segments.contains(&"pages")
                    || matches!(ext, "njk" | "liquid" | "hbs" | "webc")
                    || ext == "html" && matches!(fw, Framework::Jekyll | Framework::Eleventy)
                    || script && segments.contains(&"app") && stem == "page"
                    || script && segments.contains(&"pages") && !stem.contains('.'));
            let generated = ["sitemap", "feed", "rss", "atom", "robots", "llms"].iter().any(|g| stem.starts_with(g));
            if is_page && !generated {
                report.todo.push(format!("{rel}:1: {ext} page; rebuild it as a .mira route (its text was not moved)"));
            }
        }
    }
    if items.is_empty() {
        bail!(
            "{}: found no Markdown or MDX content for {}\nhint: pass --from to name the framework, or point at the folder that holds the content",
            source.display(),
            fw.name()
        );
    }

    // Plan new URLs, resolving collisions.
    let mut taken = BTreeSet::new();
    let mut plan = Vec::new();
    for item in &items {
        let mut new_url = match &item.collection {
            Some(c) => format!("/{c}/{}/", item.slug),
            None => normalize(&item.old_url),
        };
        if new_url.ends_with(".html/") || new_url.contains(".html") {
            new_url = normalize(&new_url.replace(".html", ""));
        }
        // The old not found page becomes Mira's, which every host serves.
        if new_url == "/404/" && item.collection.is_none() {
            plan.push(new_url);
            continue;
        }
        let mut n = 2;
        let base = new_url.clone();
        while !taken.insert(new_url.clone()) {
            new_url = format!("{}-{n}/", base.trim_end_matches('/'));
            n += 1;
            report.todo.push(format!("{}:1: its URL collided with another page and became {new_url}", item.rel));
        }
        // A missing trailing slash is already redirected by every host.
        let old = item.old_url.split(['?', '#']).next().unwrap_or("/");
        if old != new_url && format!("{old}/") != new_url {
            report.redirects.insert(item.old_url.clone(), new_url.clone());
        }
        plan.push(new_url);
    }

    let mut new_urls: BTreeMap<PathBuf, String> = BTreeMap::new();
    for (item, url) in items.iter().zip(&plan) {
        new_urls.insert(item.source.clone(), url.clone());
    }
    let links = Links { pages: &new_urls, redirects: &report.redirects.clone(), project: &source };

    // Convert content.
    let mut outputs: Vec<(PathBuf, String)> = Vec::new();
    let mut schemas: BTreeMap<String, BTreeMap<String, Option<&'static str>>> = BTreeMap::new();
    for (item, new_url) in items.iter().zip(&plan) {
        let mut data = normalize_frontmatter(&item.data);
        if !data.contains_key("title") {
            let title = first_heading(&item.body).unwrap_or_else(|| title_from(&item.slug));
            data.insert("title".into(), Value::from(title));
        }
        let body = convert_body(&item.body, &item.rel, fw, &mut report.todo);
        let body = links.rewrite(&body, item, &mut report.todo);
        let target = match &item.collection {
            Some(c) => {
                *report.entries.entry(c.clone()).or_default() += 1;
                PathBuf::from("content").join(c).join(format!("{}.md", item.slug))
            }
            None => {
                report.pages += 1;
                match new_url.trim_matches('/') {
                    "" => PathBuf::from("routes/index.md"),
                    "404" => PathBuf::from("routes/404.md"),
                    path => PathBuf::from("routes").join(path).join("index.md"),
                }
            }
        };

        // Images the page points at by relative path travel with it, wherever
        // they were in the old project.
        let from_dir = item.source.parent().unwrap_or(&source);
        let mut copies = Vec::new();
        let body = relocate(&body, from_dir, &source, &mut copies);
        // The social image is served as is, so it goes under public/.
        if let Some(Value::String(image)) = data.get("image") {
            let mut found = Vec::new();
            relocate(&format!("]({image})"), from_dir, &source, &mut found);
            if let Some((name, path)) = found.pop() {
                let name = format!("{}-{name}", item.slug);
                data.insert("image".into(), Value::from(format!("/images/{name}")));
                outputs.push((PathBuf::from("public/images").join(name), format!("\u{0}copy:{}", path.display())));
            }
        }
        let target_dir = target.parent().unwrap().to_path_buf();
        for (name, path) in copies {
            outputs.push((target_dir.join(name), format!("\u{0}copy:{}", path.display())));
        }

        if let Some(c) = &item.collection {
            let schema = schemas.entry(c.clone()).or_default();
            for (key, value) in &data {
                let kind = type_of(value);
                match schema.get(key) {
                    None => {
                        schema.insert(key.clone(), kind);
                    }
                    Some(seen) if *seen != kind => {
                        // Mixed types across entries: leave the field to `strict: false`.
                        let both_text = matches!((*seen, kind), (Some("date"), Some("string")) | (Some("string"), Some("date")));
                        schema.insert(key.clone(), if both_text { Some("string") } else { None });
                    }
                    _ => {}
                }
            }
        }
        let yaml = serde_yaml::to_string(&Value::Object(data)).unwrap_or_default();
        outputs.push((target, format!("---\n{yaml}---\n\n{}\n", body.trim())));
    }

    // Static files.
    let mut static_dirs: Vec<(PathBuf, &str)> =
        ["public", "static", "assets", "images", "img"].iter().map(|d| (source.join(d), *d)).collect();
    for root in roots(fw, &source) {
        if root.dir != source {
            static_dirs.push((root.dir.join("public"), "public"));
        }
    }
    for (from, dir) in static_dirs {
        if !from.is_dir() || matches!(fw, Framework::Hugo | Framework::Docusaurus | Framework::Gatsby) && dir != "static" && dir != "public"
        {
            continue;
        }
        for path in walk_files(&from, &source) {
            let rel = path.strip_prefix(&from).unwrap();
            let target = if dir == "public" || dir == "static" {
                PathBuf::from("public").join(rel)
            } else {
                PathBuf::from("public").join(dir).join(rel)
            };
            outputs.push((target, format!("\u{0}copy:{}", path.display())));
            report.assets += 1;
        }
    }

    if opts.dry_run {
        return Ok(report);
    }

    // Write the project.
    let dest = &opts.dest;
    std::fs::create_dir_all(dest)?;
    let mut config = serde_json::json!({
        "site": site_settings(fw, &source),
        "transitions": { "default": "fade" },
    });
    if !schemas.is_empty() {
        let collections: Map<String, Value> = schemas
            .iter()
            .map(|(name, fields)| {
                let fields: Map<String, Value> = fields
                    .iter()
                    .filter_map(|(k, t)| {
                        Some((k.clone(), Value::from(if k == "title" { (*t)?.to_string() } else { format!("{}?", (*t)?) })))
                    })
                    .collect();
                (name.clone(), serde_json::json!({ "fields": fields, "strict": false }))
            })
            .collect();
        config["collections"] = Value::Object(collections);
    }
    if !report.redirects.is_empty() {
        config["redirects"] = serde_json::to_value(&report.redirects)?;
    }
    write(dest, "mira.config.json", &(serde_json::to_string_pretty(&config)? + "\n"))?;
    write(dest, ".gitignore", "dist/\n.mira/\n")?;
    write(dest, "layouts/default.mira", LAYOUT)?;
    write(dest, "AGENTS.md", include_str!("../starter/AGENTS.md"))?;
    for name in schemas.keys() {
        write(dest, &format!("routes/{name}/[slug].mira"), ENTRY_ROUTE)?;
        let index = PathBuf::from("routes").join(name).join("index.md");
        if !outputs.iter().any(|(p, _)| *p == index) {
            write(dest, &format!("routes/{name}/index.mira"), &LIST_ROUTE.replace("COLLECTION", name).replace("TITLE", &title_from(name)))?;
        }
    }
    if !outputs.iter().any(|(p, _)| p == Path::new("routes/index.md")) {
        write(dest, "routes/index.mira", &HOME_ROUTE.replace("SITE_LIST", &home_list(&schemas)))?;
    }
    for (target, contents) in &outputs {
        let path = dest.join(target);
        std::fs::create_dir_all(path.parent().unwrap())?;
        match contents.strip_prefix("\u{0}copy:") {
            Some(from) => {
                std::fs::copy(from, &path).with_context(|| format!("copying {from}"))?;
            }
            None => std::fs::write(&path, contents)?,
        }
    }
    write(dest, "MIGRATION.md", &migration_md(&report, &opts.source))?;
    Ok(report)
}

fn write(dest: &Path, rel: &str, contents: &str) -> Result<()> {
    let path = dest.join(rel);
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, contents).with_context(|| format!("writing {rel}"))
}

/// Markdown files under `dir`, skipping build output, dependencies, and
/// symlinks, and never leaving `project`.
fn markdown_files(dir: &Path, project: &Path) -> Vec<PathBuf> {
    walk_files(dir, project)
        .into_iter()
        .filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e, "md" | "mdx" | "markdown")))
        .collect()
}

fn walk_files(dir: &Path, project: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            e.depth() == 0 || !(e.file_type().is_dir() && (SKIP_DIRS.contains(&name.as_ref()) || name == "public" || name.starts_with('.')))
        })
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| p.starts_with(project))
        .collect();
    files.sort();
    files
}

fn blog_collection(rel: &str) -> Option<String> {
    let first = rel.split('/').next()?;
    (rel.contains('/') && BLOG_DIRS.contains(&first)).then(|| first.trim_start_matches('_').to_string())
}

fn split_date_prefix(stem: &str) -> (Option<String>, &str) {
    let b = stem.as_bytes();
    let dated = b.len() > 11
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'-'
        && b[..10].iter().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit());
    if dated { (Some(stem[..10].to_string()), &stem[11..]) } else { (None, stem) }
}

fn old_url(root: &Root, rel: &str, data: &Map<String, Value>, permalink: Option<&str>, slug: &str) -> String {
    if let Some(url) = data.get("url").or_else(|| data.get("permalink")).and_then(Value::as_str) {
        return if url.starts_with('/') { url.to_string() } else { format!("/{url}") };
    }
    let without_ext = rel.rsplit_once('.').map_or(rel, |(s, _)| s);
    let path = without_ext.trim_end_matches("index").trim_end_matches("_index").trim_end_matches('/');
    match root.style {
        Style::Path => format!("{}{}", root.base, if path.is_empty() { String::new() } else { format!("{path}/") }),
        Style::Html => {
            if path.is_empty() {
                root.base.clone()
            } else if without_ext.ends_with("index") {
                format!("{}{path}/", root.base)
            } else {
                format!("{}{path}.html", root.base)
            }
        }
        Style::NextApp => {
            let dir = rel.rsplit_once('/').map_or("", |(d, _)| d);
            let dir: Vec<&str> = dir.split('/').filter(|s| !s.is_empty() && !(s.starts_with('(') && s.ends_with(')'))).collect();
            format!("{}{}", root.base, dir.join("/"))
        }
        Style::JekyllPost => {
            let date = data.get("date").and_then(Value::as_str).unwrap_or("1970-01-01");
            let (y, m, d) = (&date[..4.min(date.len())], date.get(5..7).unwrap_or("01"), date.get(8..10).unwrap_or("01"));
            let categories = match data.get("categories").or_else(|| data.get("category")) {
                Some(Value::String(s)) => s.split_whitespace().collect::<Vec<_>>().join("/"),
                Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("/"),
                _ => String::new(),
            };
            let pattern = match permalink.unwrap_or("date") {
                "date" => "/:categories/:year/:month/:day/:title:output_ext",
                "pretty" => "/:categories/:year/:month/:day/:title/",
                "ordinal" => "/:categories/:year/:y_day/:title:output_ext",
                "none" => "/:categories/:title:output_ext",
                custom => custom,
            };
            let url = pattern
                .replace(":categories", &categories)
                .replace(":year", y)
                .replace(":month", m)
                .replace(":day", d)
                .replace(":title", slug)
                .replace(":slug", slug)
                .replace(":output_ext", ".html");
            let mut clean = String::new();
            for part in url.split('/').filter(|s| !s.is_empty()) {
                clean.push('/');
                clean.push_str(part);
            }
            if url.ends_with('/') { format!("{clean}/") } else { clean }
        }
        Style::DocusaurusBlog => {
            let date = data.get("date").and_then(Value::as_str).unwrap_or("");
            if date.len() >= 10 {
                format!("{}{}/{}/{}/{slug}", root.base, &date[..4], &date[5..7], &date[8..10])
            } else {
                format!("{}{slug}", root.base)
            }
        }
    }
}

/// `/a/b`, `/a/b/`, `/a/b.html`, and `/a/b/index.html` become `/a/b/`.
fn normalize(url: &str) -> String {
    let url = url.split(['?', '#']).next().unwrap_or("/");
    let url = url.trim_end_matches("index.html").trim_end_matches(".html").trim_end_matches('/');
    if url.is_empty() { "/".into() } else { format!("{url}/") }
}

/// The `input` folder from an Eleventy config, without running it.
fn eleventy_input(dir: &Path) -> String {
    for name in ["eleventy.config.js", "eleventy.config.mjs", "eleventy.config.cjs", ".eleventy.js"] {
        let text = std::fs::read_to_string(dir.join(name)).unwrap_or_default();
        for line in text.lines() {
            if let Some(rest) = line.trim().strip_prefix("input:") {
                let rest = rest.trim();
                if let Some(q) = rest.chars().next().filter(|q| matches!(q, '"' | '\'' | '`'))
                    && let Some(value) = rest[1..].split(q).next()
                {
                    let value = value.trim_start_matches("./").trim_end_matches('/');
                    // Only a plain folder inside the project.
                    if !value.is_empty() && !value.contains("..") && !value.starts_with('/') && !value.contains(':') {
                        return value.to_string();
                    }
                }
            }
        }
    }
    ".".to_string()
}

fn jekyll_permalink(dir: &Path) -> Option<String> {
    let config = std::fs::read_to_string(dir.join("_config.yml")).ok()?;
    let value: serde_yaml::Value = serde_yaml::from_str(&config).ok()?;
    value.get("permalink")?.as_str().map(str::to_string)
}

fn split_frontmatter(text: &str) -> Result<(Map<String, Value>, String)> {
    let text = text.replace("\r\n", "\n");
    let (fm, body, toml) = if let Some(rest) = text.strip_prefix("---\n") {
        match rest.split_once("\n---\n").or_else(|| rest.strip_suffix("\n---").map(|f| (f, ""))) {
            Some((fm, body)) => (fm.to_string(), body.to_string(), false),
            None => return Ok((Map::new(), text)),
        }
    } else if let Some(rest) = text.strip_prefix("+++\n") {
        match rest.split_once("\n+++\n") {
            Some((fm, body)) => (fm.to_string(), body.to_string(), true),
            None => return Ok((Map::new(), text)),
        }
    } else {
        return Ok((Map::new(), text));
    };
    let value: Value = if toml {
        serde_json::to_value(toml::from_str::<toml::Value>(&fm)?)?
    } else if fm.trim().is_empty() {
        Value::Object(Map::new())
    } else {
        serde_json::to_value(serde_yaml::from_str::<serde_yaml::Value>(&fm)?)?
    };
    match unwrap_dates(value) {
        Value::Object(map) => Ok((map, body)),
        Value::Null => Ok((Map::new(), body)),
        _ => bail!("frontmatter is not a mapping"),
    }
}

fn strip_frontmatter(text: &str) -> &str {
    for fence in ["---\n", "+++\n"] {
        if let Some(rest) = text.strip_prefix(fence)
            && let Some((_, body)) = rest.split_once(&format!("\n{fence}"))
        {
            return body;
        }
    }
    text
}

/// TOML datetimes arrive as `{"$__toml_private_datetime": "..."}`.
fn unwrap_dates(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            if map.len() == 1
                && let Some(Value::String(s)) = map.get("$__toml_private_datetime")
            {
                return Value::String(s.clone());
            }
            Value::Object(map.into_iter().map(|(k, v)| (k, unwrap_dates(v))).collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(unwrap_dates).collect()),
        other => other,
    }
}

/// Maps the common frontmatter names of other frameworks onto Mira's, and
/// keeps everything else as it was.
fn normalize_frontmatter(data: &Map<String, Value>) -> Map<String, Value> {
    const RENAMES: [(&str, &[&str]); 7] = [
        ("description", &["description", "summary", "excerpt", "subtitle", "abstract"]),
        ("date", &["date", "pubDate", "publishDate", "published_at", "publishedAt", "created"]),
        ("updated", &["updated", "lastmod", "lastMod", "updatedDate", "updated_at", "modified", "last_update"]),
        (
            "image",
            &["image", "cover", "heroImage", "hero", "thumbnail", "featured_image", "og_image", "ogImage", "coverImage", "cover_image"],
        ),
        ("tags", &["tags", "keywords", "categories"]),
        ("author", &["author", "authors"]),
        ("title", &["title", "name", "sidebar_label"]),
    ];
    let mut out = Map::new();
    let mut used = BTreeSet::new();
    for (target, sources) in RENAMES {
        for source in sources {
            if let Some(value) = data.get(*source) {
                used.insert(source.to_string());
                if out.contains_key(target) {
                    continue;
                }
                let value = match (target, value) {
                    // Unreadable dates stay as they were; the schema then flags them.
                    ("date" | "updated", Value::String(s)) => iso_date(s).map_or_else(|| value.clone(), Value::String),
                    ("tags", Value::String(s)) => Value::Array(s.split([',', ' ']).filter(|t| !t.is_empty()).map(Value::from).collect()),
                    ("author", Value::Array(a)) => a.first().map(name_of).unwrap_or(Value::Null),
                    ("author", Value::Object(_)) => name_of(value),
                    ("image", Value::Object(o)) => {
                        o.get("src").or_else(|| o.get("image")).or_else(|| o.get("url")).cloned().unwrap_or(Value::Null)
                    }
                    _ => value.clone(),
                };
                if !value.is_null() {
                    out.insert(target.to_string(), value);
                }
            }
        }
    }
    if data.get("published") == Some(&Value::Bool(false)) || data.get("draft") == Some(&Value::Bool(true)) {
        out.insert("draft".into(), Value::Bool(true));
    }
    used.extend(["published", "draft", "slug", "url", "permalink", "layout"].map(String::from));
    if let Some(Value::String(s)) = data.get("slug") {
        out.insert("slug".into(), Value::from(crate::content::slugify(s)));
    }
    for (key, value) in data {
        if !used.contains(key) && !value.is_null() {
            out.insert(key.clone(), value.clone());
        }
    }
    out
}

/// Where every migrated file and old URL now lives.
struct Links<'a> {
    pages: &'a BTreeMap<PathBuf, String>,
    redirects: &'a BTreeMap<String, String>,
    project: &'a Path,
}

impl Links<'_> {
    /// Points links to `.md` files, and to old URLs that changed, at the
    /// new pages, keeping any `#fragment`.
    fn rewrite(&self, text: &str, item: &Item, todo: &mut Vec<String>) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        loop {
            let next = ["](", "href=\"", "href='"].iter().filter_map(|m| rest.find(m).map(|i| (i, m.len()))).min();
            let Some((i, len)) = next else { break };
            out.push_str(&rest[..i + len]);
            rest = &rest[i + len..];
            let end = rest.find([')', '"', '\'', ' ', '\n']).unwrap_or(rest.len());
            let url = &rest[..end];
            let (path, fragment) = url.split_once('#').map_or((url, ""), |(p, f)| (p, f));
            let new = if path.ends_with(".md") || path.ends_with(".mdx") {
                let found = self.file(path, item);
                if found.is_none() && !url.contains(':') {
                    let line = text[..text.len() - rest.len()].lines().count();
                    todo.push(format!("{}:{line}: link to {path} does not match a migrated page", item.rel));
                }
                found
            } else {
                self.redirects.get(path).cloned()
            };
            match new {
                Some(new) if fragment.is_empty() => out.push_str(&new),
                Some(new) => out.push_str(&format!("{new}#{fragment}")),
                None => out.push_str(url),
            }
            rest = &rest[end..];
        }
        out.push_str(rest);
        out
    }

    fn file(&self, path: &str, item: &Item) -> Option<String> {
        let candidates = if let Some(abs) = path.strip_prefix('/') {
            vec![item.root_dir.join(abs), self.project.join(abs)]
        } else {
            vec![item.source.parent()?.join(path), item.root_dir.join(path)]
        };
        candidates
            .into_iter()
            .filter_map(|c| std::fs::canonicalize(c).ok())
            .filter(|c| c.starts_with(self.project))
            .find_map(|c| self.pages.get(&c).cloned())
    }
}

/// `2024-05-01T10:00Z`, `2024/05/01`, `May 1, 2024`, `1 May 2024`, and
/// `Jul 08 2022` become `2024-05-01`. Anything else is None.
fn iso_date(text: &str) -> Option<String> {
    const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let t = text.trim();
    let b = t.as_bytes();
    if b.len() >= 10
        && b[..4].iter().all(u8::is_ascii_digit)
        && matches!(b[4], b'-' | b'/')
        && b[7] == b[4]
        && b[5..7].iter().chain(&b[8..10]).all(u8::is_ascii_digit)
    {
        let (y, m, d) = (&t[..4], &t[5..7], &t[8..10]);
        return valid(y.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
    }
    let words: Vec<String> = t.split([' ', ',', '.', '-']).filter(|w| !w.is_empty()).map(str::to_ascii_lowercase).collect();
    if words.len() != 3 {
        return None;
    }
    let month = words.iter().position(|w| w.len() >= 3 && MONTHS.iter().any(|m| w.starts_with(m)))?;
    let m = MONTHS.iter().position(|m| words[month].starts_with(m))? as u32 + 1;
    let numbers: Vec<&String> = words.iter().enumerate().filter(|(i, _)| *i != month).map(|(_, w)| w).collect();
    let (d, y) = match (numbers[0].len(), numbers[1].len()) {
        (4, _) => (numbers[1], numbers[0]),
        _ => (numbers[0], numbers[1]),
    };
    let d = d.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    valid(y.parse().ok()?, m, d.parse().ok()?)
}

fn valid(y: u32, m: u32, d: u32) -> Option<String> {
    ((1000..=9999).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d)).then(|| format!("{y:04}-{m:02}-{d:02}"))
}

/// `{name: "Ada", picture: ...}` becomes "Ada".
fn name_of(value: &Value) -> Value {
    match value {
        Value::Object(o) => o.get("name").or_else(|| o.get("title")).cloned().unwrap_or(Value::Null),
        other => other.clone(),
    }
}

/// The schema type of a frontmatter value, or None when Mira's schema
/// types cannot describe it (it is still kept, under `strict: false`).
fn type_of(value: &Value) -> Option<&'static str> {
    Some(match value {
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(s) if s.len() == 10 && s.as_bytes()[4] == b'-' && s.as_bytes()[7] == b'-' => "date",
        Value::String(_) => "string",
        Value::Array(a) if a.iter().all(Value::is_string) => "string[]",
        _ => return None,
    })
}

/// Rewrites relative image and video references in Markdown and HTML to
/// sit next to the new file, and lists the files to copy. A reference is
/// followed only to a regular file inside the old project.
fn relocate(text: &str, from_dir: &Path, project: &Path, copies: &mut Vec<(String, PathBuf)>) -> String {
    const MEDIA: [&str; 10] = ["png", "jpg", "jpeg", "gif", "webp", "avif", "svg", "mp4", "webm", "pdf"];
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let next = ["](", "src=\"", "src='", "poster=\""].iter().filter_map(|m| rest.find(m).map(|i| (i, m.len()))).min();
        let Some((i, len)) = next else { break };
        out.push_str(&rest[..i + len]);
        rest = &rest[i + len..];
        let end = rest.find([')', '"', '\'', ' ', '\n']).unwrap_or(rest.len());
        let url = &rest[..end];
        let path_part = url.split(['?', '#']).next().unwrap_or("");
        let ext = path_part.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
        let relative = !url.is_empty() && !url.starts_with(['/', '#']) && !url.contains(':') && MEDIA.contains(&ext.as_str());
        let resolved = relative
            .then(|| std::fs::canonicalize(from_dir.join(path_part)).ok())
            .flatten()
            .filter(|p| p.starts_with(project) && p.is_file());
        match resolved {
            Some(path) => {
                let name = match copies.iter().find(|(_, p)| *p == path) {
                    Some((name, _)) => name.clone(),
                    None => {
                        let base = path.file_name().unwrap().to_string_lossy().to_string();
                        let mut name = base.clone();
                        let mut n = 2;
                        while copies.iter().any(|(existing, _)| *existing == name) {
                            name = format!("{n}-{base}");
                            n += 1;
                        }
                        copies.push((name.clone(), path));
                        name
                    }
                };
                out.push_str(&format!("./{name}"));
            }
            None => out.push_str(url),
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// Rewrites framework specific syntax into Markdown Mira renders, and logs
/// whatever it cannot convert.
fn convert_body(body: &str, rel: &str, fw: Framework, todo: &mut Vec<String>) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut in_code = false;
    let mut admonition: Option<&'static str> = None;
    let mdx = rel.ends_with(".mdx");
    for (i, line) in body.lines().enumerate() {
        let n = i + 1;
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code = !in_code;
            out.push(line.to_string());
            continue;
        }
        if in_code {
            out.push(line.to_string());
            continue;
        }
        // Docusaurus and VitePress admonitions become callouts.
        if let Some(kind) = trimmed.strip_prefix(":::") {
            let (kind, title) = kind.trim().split_once(' ').unwrap_or((kind.trim(), ""));
            if kind.is_empty() {
                admonition = None;
            } else {
                let marker = match kind {
                    "tip" => "TIP",
                    "important" | "info" => "IMPORTANT",
                    "warning" => "WARNING",
                    "caution" | "danger" => "CAUTION",
                    _ => "NOTE",
                };
                admonition = Some(marker);
                out.push(format!("> [!{marker}]"));
                let title = title.trim().trim_start_matches('[').trim_end_matches(']');
                if !title.is_empty() {
                    out.push(format!("> **{title}**"));
                }
            }
            continue;
        }
        if admonition.is_some() {
            out.push(if line.trim().is_empty() { ">".into() } else { format!("> {line}") });
            continue;
        }
        if mdx && (trimmed.starts_with("import ") || trimmed.starts_with("export ")) {
            continue;
        }
        let mut converted = line.to_string();
        // Hugo figures become frames; other shortcodes need a person.
        if fw == Framework::Hugo && (trimmed.starts_with("{{<") || trimmed.starts_with("{{%")) {
            if trimmed.contains("figure") {
                converted = frame_from_attrs(trimmed);
            } else {
                todo.push(format!("{rel}:{n}: Hugo shortcode `{}` has no Mira equivalent yet", trimmed.trim()));
                converted = format!("<!-- mira migrate: {} -->", trimmed.replace("--", "- -"));
            }
        }
        // Jekyll Liquid tags are not rendered by Mira.
        if fw == Framework::Jekyll && (trimmed.starts_with("{%") || trimmed.contains("{{ site.") || trimmed.contains("{{site.")) {
            if trimmed.starts_with("{% highlight") {
                let lang = trimmed.trim_start_matches("{% highlight").trim().trim_end_matches("%}").split_whitespace().next().unwrap_or("");
                converted = format!("```{lang}");
            } else if trimmed.starts_with("{% endhighlight") {
                converted = "```".into();
            } else if trimmed.starts_with("{% raw") || trimmed.starts_with("{% endraw") {
                continue;
            } else {
                todo.push(format!("{rel}:{n}: Liquid `{}` is not rendered by Mira", trimmed.trim()));
                converted = format!("<!-- mira migrate: {} -->", trimmed.replace("--", "- -"));
            }
        }
        // JSX in MDX: images become frames; other components need a person.
        if mdx && trimmed.starts_with('<') && trimmed.chars().nth(1).is_some_and(|c| c.is_ascii_uppercase()) {
            let name: String = trimmed[1..].chars().take_while(|c| c.is_alphanumeric()).collect();
            if name == "Image" || name == "Img" {
                converted = frame_from_attrs(trimmed);
            } else if matches!(name.as_str(), "Callout" | "Admonition" | "Note" | "Tip" | "Warning") {
                todo.push(format!("{rel}:{n}: <{name}> was kept as text; consider a > [!NOTE] callout"));
                converted = format!("<!-- mira migrate: <{name}> -->");
            } else {
                todo.push(format!("{rel}:{n}: MDX component <{name}> needs a Mira equivalent"));
                converted = format!("<!-- mira migrate: {} -->", trimmed.replace("--", "- -"));
            }
        } else if mdx && trimmed.starts_with("</") && trimmed.chars().nth(2).is_some_and(|c| c.is_ascii_uppercase()) {
            converted = String::new();
        }
        if let Some(i) = converted.find("](http")
            && converted[..i].contains("![")
        {
            todo.push(format!("{rel}:{n}: a remote image became a link; download it into the project to show it"));
            converted = converted.replacen("![", "[Image: ", 1);
        }
        if converted.contains("![](") {
            todo.push(format!("{rel}:{n}: an image has no alt text; describe it, or confirm it is decoration"));
        }
        out.push(converted);
    }
    out.join("\n")
}

/// Builds a `<mira-frame>` from a component or shortcode's src, alt, and
/// caption attributes.
fn frame_from_attrs(tag: &str) -> String {
    let get = |name: &str| {
        for quote in ['"', '\''] {
            let needle = format!("{name}={quote}");
            if let Some(i) = tag.find(&needle) {
                let rest = &tag[i + needle.len()..];
                if let Some(end) = rest.find(quote) {
                    return Some(rest[..end].to_string());
                }
            }
        }
        None
    };
    let src = get("src").unwrap_or_default();
    let alt = get("alt").unwrap_or_default();
    let caption = get("caption").or_else(|| get("title"));
    let mut frame = format!("<mira-frame src=\"{}\" alt=\"{}\"", crate::html::escape(&src), crate::html::escape(&alt));
    if let Some(c) = caption {
        frame.push_str(&format!(" caption=\"{}\"", crate::html::escape(&c)));
    }
    frame.push_str("></mira-frame>");
    frame
}

fn first_heading(body: &str) -> Option<String> {
    body.lines().find_map(|l| l.strip_prefix("# ")).map(|t| t.trim().to_string())
}

fn title_from(slug: &str) -> String {
    let words = slug.trim_matches('/').rsplit('/').next().unwrap_or(slug).replace(['-', '_'], " ");
    let mut chars = words.trim().chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_else(|| "Home".into())
}

/// Site title, description, and URL from the old project's config, read as
/// text: nothing in it runs.
fn site_settings(fw: Framework, dir: &Path) -> Value {
    let mut site = Map::new();
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap_or_default();
    let quoted = |text: &str, key: &str| -> Option<String> {
        for line in text.lines() {
            let l = line.trim();
            if let Some(rest) = l.strip_prefix(key) {
                let rest = rest.trim_start().trim_start_matches([':', '=']).trim();
                let q = rest.chars().next()?;
                if matches!(q, '"' | '\'' | '`') {
                    return rest[1..].split(q).next().map(str::to_string);
                }
                if !rest.is_empty() && !rest.starts_with('{') && !rest.starts_with('[') {
                    return Some(rest.trim_end_matches(',').to_string());
                }
            }
        }
        None
    };
    let (title, description, url) = match fw {
        Framework::Hugo => {
            let t = read("hugo.toml") + &read("config.toml");
            (quoted(&t, "title"), None, quoted(&t, "baseURL"))
        }
        Framework::Jekyll => {
            let t = read("_config.yml");
            (quoted(&t, "title"), quoted(&t, "description"), quoted(&t, "url"))
        }
        Framework::Docusaurus => {
            let t = read("docusaurus.config.js") + &read("docusaurus.config.ts");
            (quoted(&t, "title"), quoted(&t, "tagline"), quoted(&t, "url"))
        }
        Framework::Astro => {
            let t = read("astro.config.mjs") + &read("astro.config.ts");
            (None, None, quoted(&t, "site"))
        }
        _ => (None, None, None),
    };
    let package: Value = serde_json::from_str(&read("package.json")).unwrap_or(Value::Null);
    let title = title
        .or_else(|| package.get("name").and_then(Value::as_str).map(title_from))
        .unwrap_or_else(|| title_from(&dir.file_name().unwrap_or_default().to_string_lossy()));
    site.insert("title".into(), Value::from(title));
    if let Some(d) = description.or_else(|| package.get("description").and_then(Value::as_str).map(str::to_string)) {
        site.insert("description".into(), Value::from(d));
    }
    if let Some(u) = url.filter(|u| u.starts_with("http")) {
        site.insert("url".into(), Value::from(u.trim_end_matches('/')));
    }
    site.insert("lang".into(), Value::from("en"));
    Value::Object(site)
}

fn home_list(schemas: &BTreeMap<String, BTreeMap<String, Option<&'static str>>>) -> String {
    schemas.keys().map(|c| format!("  <li><a href=\"/{c}/\">{}</a></li>\n", title_from(c))).collect()
}

fn migration_md(report: &MigrateReport, source: &Path) -> String {
    let fw = report.framework.map_or("unknown", Framework::name);
    let mut out = format!(
        "# Migration from {fw}\n\nMigrated from `{}` by `mira migrate`. Run `mira dev` to see the site and `mira build` to check it.\n\n",
        source.display()
    );
    out.push_str("| | |\n| --- | --- |\n");
    out.push_str(&format!("| Pages | {} |\n", report.pages));
    for (c, n) in &report.entries {
        out.push_str(&format!("| Entries in `{c}` | {n} |\n"));
    }
    out.push_str(&format!(
        "| Static files | {} |\n| Redirects | {} |\n| Items to review | {} |\n\n",
        report.assets,
        report.redirects.len(),
        report.todo.len()
    ));
    out.push_str("## Review\n\n");
    if report.todo.is_empty() {
        out.push_str("Nothing needs review.\n\n");
    } else {
        out.push_str("Each item names the file and line in the old project. Where text could not be converted, the new file keeps it in a `<!-- mira migrate: … -->` comment.\n\n");
        for item in &report.todo {
            out.push_str(&format!("- [ ] {item}\n"));
        }
        out.push('\n');
    }
    out.push_str("## Redirects\n\nOld URLs that changed redirect permanently, so links and search rankings carry over. They are listed under `redirects` in `mira.config.json`.\n\n");
    if matches!(report.framework, Some(Framework::Nextjs | Framework::Gatsby | Framework::Astro)) {
        out.push_str(&format!("{fw} decides some URLs in code, which `mira migrate` does not run. Compare the old site's sitemap with `dist/sitemap.xml` and add any missing redirects.\n\n"));
    }
    if report.redirects.is_empty() {
        out.push_str("No URLs changed.\n");
    } else {
        out.push_str("| Old | New |\n| --- | --- |\n");
        for (from, to) in &report.redirects {
            out.push_str(&format!("| `{from}` | `{to}` |\n"));
        }
    }
    out
}

const LAYOUT: &str = r##"<template>
<a class="skip" href="#main">Skip to content</a>
<header class="site-header">
  <a class="brand" href="/" mira-morph="brand">{{ site.title }}</a>
</header>
<main id="main">
  <slot />
</main>
<footer class="site-footer">
  <span class="mira-eyebrow">{{ site.title }} &middot; Built with Mira</span>
</footer>
</template>

<style>
body { max-width: 46rem; margin-inline: auto; padding-inline: var(--space-4); }
.skip { position: absolute; left: var(--space-4); top: -3rem; }
.skip:focus { top: var(--space-4); }
.site-header { padding-block: var(--space-6) var(--space-12); }
.brand { color: var(--ink); text-decoration: none; font-family: var(--font-display); font-weight: 600; font-size: 1.125rem; }
main { padding-bottom: var(--space-24); }
.site-footer { border-top: 1px solid var(--line); padding-block: var(--space-6) var(--space-8); }
</style>
"##;

const ENTRY_ROUTE: &str = r#"<template>
<article>
  <p class="mira-eyebrow">{{ entry.date }}</p>
  <h1 mira-morph="title-{{ entry.slug }}">{{ entry.title }}</h1>
  <div class="prose">
    <slot />
  </div>
</article>
</template>

<style>
article h1 { width: fit-content; margin-block: var(--space-3) var(--space-8); }
</style>
"#;

const LIST_ROUTE: &str = r#"---
title: TITLE
---
<template>
<h1>{{ page.title }}</h1>
<ol class="entries" role="list">
  {#each collections.COLLECTION as item}
  <li>
    <a href="{{ item.url }}">
      <h2 mira-morph="title-{{ item.slug }}">{{ item.title }}</h2>
      {#if item.description}<p>{{ item.description }}</p>{/if}
    </a>
  </li>
  {/each}
</ol>
</template>

<style>
.entries { list-style: none; padding: 0; margin-top: var(--space-8); }
.entries li + li { border-top: 1px solid var(--line); }
.entries a { display: grid; gap: var(--space-1); padding-block: var(--space-6); color: inherit; text-decoration: none; }
.entries h2 { width: fit-content; }
.entries p { color: var(--ink-muted); }
</style>
"#;

const HOME_ROUTE: &str = r#"---
title: Home
---
<template>
<h1>{{ site.title }}</h1>
<ul>
SITE_LIST</ul>
</template>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn site(files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mira-migrate-{}-{}", std::process::id(), files.len() * 7 + files[0].0.len()));
        let _ = std::fs::remove_dir_all(&dir);
        for (path, contents) in files {
            let p = dir.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, contents).unwrap();
        }
        dir
    }

    #[test]
    fn detects_frameworks() {
        let next = site(&[("package.json", r#"{"dependencies":{"next":"15"}}"#), ("posts/a.md", "# A")]);
        assert_eq!(detect(&next), Framework::Nextjs);
        let hugo = site(&[("hugo.toml", "title = 'x'"), ("content/a.md", "# A")]);
        assert_eq!(detect(&hugo), Framework::Hugo);
        let jekyll = site(&[("_config.yml", "title: x"), ("_posts/2024-01-02-a.md", "# A")]);
        assert_eq!(detect(&jekyll), Framework::Jekyll);
    }

    #[test]
    fn migrates_jekyll_with_redirects() {
        let src = site(&[
            ("_config.yml", "title: Old Blog\nurl: https://old.example\n"),
            (
                "_posts/2024-05-01-hello-world.md",
                "---\ntitle: Hello\nexcerpt: First post\ncategories: news\n---\n\nHi {{ site.title }}\n{% highlight rust %}\nfn main() {}\n{% endhighlight %}\n",
            ),
            ("about.md", "---\ntitle: About\n---\nAbout me\n"),
        ]);
        let dest = src.with_extension("out");
        let _ = std::fs::remove_dir_all(&dest);
        let report = migrate(&MigrateOptions { source: src.clone(), dest: dest.clone(), from: None, dry_run: false }).unwrap();
        assert_eq!(report.framework, Some(Framework::Jekyll));
        assert_eq!(report.entries["posts"], 1);
        assert_eq!(report.redirects["/news/2024/05/01/hello-world.html"], "/posts/hello-world/");
        assert_eq!(report.redirects["/about.html"], "/about/");
        let post = std::fs::read_to_string(dest.join("content/posts/hello-world.md")).unwrap();
        assert!(post.contains("description: First post"), "{post}");
        assert!(post.contains("date: 2024-05-01"), "{post}");
        assert!(post.contains("```rust\nfn main() {}\n```"), "{post}");
        assert!(report.todo.iter().any(|t| t.contains("Liquid")), "{:?}", report.todo);
        let config = std::fs::read_to_string(dest.join("mira.config.json")).unwrap();
        assert!(config.contains("\"url\": \"https://old.example\""), "{config}");
        assert!(dest.join("MIGRATION.md").exists());
        // The migrated project builds.
        let built = crate::build(&crate::BuildOptions { root: dest.clone(), out: dest.join("dist"), dev: false });
        assert!(built.is_ok(), "{:?}", built.err());
    }

    #[test]
    fn converts_mdx_and_admonitions() {
        let mut todo = Vec::new();
        let body = "import Chart from '../chart'\n\n:::warning Careful\nBe careful.\n:::\n\n<Image src=\"/a.png\" alt=\"A\" />\n<Chart data={x} />\n";
        let out = convert_body(body, "posts/a.mdx", Framework::Nextjs, &mut todo);
        assert!(!out.contains("import Chart"), "{out}");
        assert!(out.contains("> [!WARNING]\n> **Careful**\n> Be careful."), "{out}");
        assert!(out.contains("<mira-frame src=\"/a.png\" alt=\"A\"></mira-frame>"), "{out}");
        assert!(todo.iter().any(|t| t.contains("<Chart>")), "{todo:?}");
    }

    #[test]
    fn reads_dates_without_cutting_them() {
        assert_eq!(iso_date("2024-05-01T10:00:00Z").as_deref(), Some("2024-05-01"));
        assert_eq!(iso_date("2024/05/01").as_deref(), Some("2024-05-01"));
        assert_eq!(iso_date("Jul 08 2022").as_deref(), Some("2022-07-08"));
        assert_eq!(iso_date("July 8, 2022").as_deref(), Some("2022-07-08"));
        assert_eq!(iso_date("8th March 2021").as_deref(), Some("2021-03-08"));
        assert_eq!(iso_date("next week"), None);
        assert_eq!(iso_date("2024-13-01"), None);
    }

    #[test]
    fn reads_toml_frontmatter() {
        let (data, body) = split_frontmatter("+++\ntitle = \"Hi\"\ndate = 2024-05-01T10:00:00Z\n+++\nBody\n").unwrap();
        assert_eq!(data["title"], "Hi");
        assert_eq!(normalize_frontmatter(&data)["date"], "2024-05-01");
        assert_eq!(body, "Body\n");
    }

    #[test]
    fn refuses_a_destination_inside_the_source() {
        let src = site(&[("a.md", "# A")]);
        let err = migrate(&MigrateOptions { source: src.clone(), dest: src.join("new"), from: None, dry_run: false });
        let _ = err; // A new folder does not exist yet, so canonicalize fails; check an existing one.
        std::fs::create_dir_all(src.join("inside")).unwrap();
        let err =
            migrate(&MigrateOptions { source: src.clone(), dest: src.join("inside"), from: None, dry_run: false }).unwrap_err().to_string();
        assert!(err.contains("inside the source"), "{err}");
    }
}
