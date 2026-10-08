use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, anyhow, bail};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::{Map, Value, json};
use walkdir::WalkDir;

use crate::assets::{BASE_CSS, FRAME_JS, NOT_FOUND_MIRA, RUNTIME_JS, SEARCH_JS, compact_css, components_for};
use crate::config::Config;
use crate::content::{parse_document, reading_time, render_markdown};
use crate::html::{compile_morphs, csp_hash, escape, mark_current_links, size_images};
use crate::outputs::{PageMeta, twin_url};
use crate::template::{Component, parse_component};
use crate::twin::html_to_markdown;

/// Marker written into the output directory. Mira only clears an existing
/// output directory that carries it, so a mistyped `--out` never deletes
/// unrelated files.
const OUTPUT_MARKER: &str = ".mira-output";

pub struct BuildOptions {
    pub root: PathBuf,
    pub out: PathBuf,
    /// Dev builds include drafts and the reload client.
    pub dev: bool,
    /// Write the hosts' config files, such as `vercel.json`, to the project
    /// root. Only `mira build` does: dev and MCP builds use a private output
    /// folder, which those files must never point at.
    pub host_config: bool,
}

#[derive(Debug, Serialize)]
pub struct BuildReport {
    pub build_id: String,
    pub pages: Vec<PageReport>,
    pub collections: BTreeMap<String, usize>,
    pub runtime_gzip_bytes: usize,
    pub duration_ms: f64,
    pub out_dir: PathBuf,
    /// Site level files written alongside the pages.
    pub outputs: Vec<String>,
    pub warnings: Vec<String>,
    /// Wall time per build step, in order.
    pub timings: Vec<Timing>,
}

#[derive(Debug, Serialize)]
pub struct Timing {
    pub step: &'static str,
    pub ms: f64,
}

struct Clock {
    last: Instant,
    steps: Vec<Timing>,
}

impl Clock {
    fn lap(&mut self, step: &'static str) {
        let now = Instant::now();
        self.steps.push(Timing { step, ms: (now - self.last).as_secs_f64() * 1000.0 });
        self.last = now;
    }
}

#[derive(Debug, Serialize)]
pub struct PageReport {
    pub url: String,
    pub source: String,
    pub html_bytes: usize,
    pub gzip_bytes: usize,
    pub morphs: usize,
}

struct Entry {
    slug: String,
    source: PathBuf,
    data: Map<String, Value>,
    html: String,
    /// Markdown source of the body, reused for the twin.
    markdown: String,
    url: Option<String>,
}

enum RouteBody {
    /// Rendered HTML and the Markdown source.
    Markdown(String, String),
    Component(Component),
}

struct Route {
    source: PathBuf,
    data: Map<String, Value>,
    body: RouteBody,
    /// URL segments; `None` marks the dynamic segment filled by entry slugs.
    segments: Vec<Option<String>>,
    collection: Option<String>,
    not_found: bool,
}

impl Route {
    fn url(&self, slug: Option<&str>) -> String {
        if self.not_found {
            return "/404.html".into();
        }
        let parts: Vec<&str> = self.segments.iter().map(|s| s.as_deref().or(slug).unwrap_or_default()).collect();
        if parts.is_empty() { "/".into() } else { format!("/{}/", parts.join("/")) }
    }

    /// The URL of the static part of a dynamic route: `/posts/` for
    /// `routes/posts/[slug].mira`.
    fn base_url(&self) -> String {
        let parts: Vec<&str> = self.segments.iter().flatten().map(String::as_str).collect();
        if parts.is_empty() { "/".into() } else { format!("/{}/", parts.join("/")) }
    }
}

struct Rendered {
    url: String,
    source: String,
    html: String,
    morphs: usize,
    meta: PageMeta,
}

pub fn build(opts: &BuildOptions) -> Result<BuildReport> {
    let started = Instant::now();
    let mut clock = Clock { last: started, steps: Vec::new() };
    let root = &opts.root;
    let config = Config::load(root)?;
    let build_id = format!("{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis());

    let out_rel = opts.out.strip_prefix(root).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default();
    if opts.host_config && !config.hosts.is_empty() && out_rel.is_empty() {
        bail!("mira.config.json: hosts need the output folder inside the project\nhint: use an --out path under the project root");
    }
    let host_files = crate::hosts::files(&config, &out_rel)?;

    let layouts = load_layouts(root)?;
    let mut collections = load_collections(root, opts.dev, &config)?;
    for name in config.collections.keys() {
        if !collections.contains_key(name) {
            bail!(
                "mira.config.json: collections.{name} has a schema but content/{name}/ does not exist\nhint: create the folder or remove the schema"
            );
        }
    }
    let routes = load_routes(root, opts.dev)?;
    clock.lap("load");

    // Dynamic routes give collection entries their URLs.
    for route in &routes {
        let Some(name) = &route.collection else { continue };
        let Some(entries) = collections.get_mut(name) else {
            bail!(
                "{}: no collection named \"{name}\"\nhint: create content/{name}/ or set `collection:` in the route frontmatter",
                rel(root, &route.source)
            );
        };
        for entry in entries {
            entry.url = Some(route.url(Some(&entry.slug)));
        }
    }

    // Neighbors in collection order, for previous and next links.
    for entries in collections.values_mut() {
        let links: Vec<Value> = entries
            .iter()
            .map(|e| json!({ "title": e.data.get("title").cloned().unwrap_or(Value::from(e.slug.clone())), "url": e.url }))
            .collect();
        for (i, entry) in entries.iter_mut().enumerate() {
            if i > 0 {
                entry.data.insert("prev".into(), links[i - 1].clone());
            }
            if let Some(next) = links.get(i + 1) {
                entry.data.insert("next".into(), next.clone());
            }
        }
    }

    let collections_json: Map<String, Value> =
        collections.iter().map(|(name, entries)| (name.clone(), Value::Array(entries.iter().map(entry_json).collect()))).collect();
    let collections_json = Value::Object(collections_json);

    let mut jobs: Vec<(&Route, Option<&Entry>)> = Vec::new();
    for route in &routes {
        match &route.collection {
            Some(name) => jobs.extend(collections[name].iter().map(|e| (route, Some(e)))),
            None => jobs.push((route, None)),
        }
    }

    let mut seen = HashMap::new();
    for (route, entry) in &jobs {
        let url = route.url(entry.map(|e| e.slug.as_str()));
        let source = entry.map_or(&route.source, |e| &e.source);
        if let Some(prev) = seen.insert(url.clone(), source) {
            bail!(
                "{}: {url} is also produced by {}\nhint: rename one of the files or set a different `slug:`",
                rel(root, source),
                rel(root, prev)
            );
        }
    }

    let mut warnings = Vec::new();
    let feeds: Vec<(String, String)> = if config.site.url.is_some() {
        routes.iter().filter_map(|r| r.collection.as_ref().map(|c| (c.clone(), format!("{}rss.xml", r.base_url())))).collect()
    } else {
        warnings.push("site.url is not set, so canonical URLs, sitemap.xml, and RSS feeds were skipped".to_string());
        Vec::new()
    };
    let mut shared = Shared::new(&config, root, opts.dev, &build_id, feeds)?;
    shared.titles = routes
        .iter()
        .filter(|r| r.collection.is_none() && !r.not_found)
        .filter_map(|r| r.data.get("title").and_then(Value::as_str).map(|t| (r.url(None), t.to_string())))
        .collect();
    let rendered: Vec<Rendered> =
        jobs.par_iter().map(|(route, entry)| render_page(&shared, &layouts, &collections_json, route, *entry)).collect::<Result<_>>()?;
    clock.lap("render");

    // Links and assets must resolve before anything is written.
    {
        let lint_pages: Vec<crate::lint::Page> =
            rendered.iter().map(|p| crate::lint::Page { url: &p.url, source: &p.source, html: &p.html }).collect();
        let mut extra: Vec<String> =
            ["/_mira/search.json", "/_mira/search.js", "/llms.txt", "/llms-full.txt", "/robots.txt", "/sitemap.xml", "/_headers"]
                .iter()
                .map(|s| s.to_string())
                .collect();
        extra.extend(shared.feeds.iter().map(|(_, feed)| feed.clone()));
        extra.extend(shared.media.outputs());
        extra.extend(config.redirects.keys().cloned());
        extra.extend(["/media.json".to_string(), "/_mira/frame.js".to_string(), "/_mira/content.json".to_string()]);
        extra.extend(rendered.iter().filter(|p| p.meta.twin.is_some()).map(|p| twin_url(&p.url)));
        let known = crate::lint::known_paths(&lint_pages, extra, &root.join("public"));
        warnings.extend(crate::lint::check(&lint_pages, &known, root)?);
        warnings.extend(unsafe_uses(root));
    }
    clock.lap("check");

    prepare_out_dir(&opts.out)?;
    clock.lap("clean");
    let search_docs: Vec<crate::search::Doc> = rendered
        .iter()
        .filter(|p| p.meta.listed())
        .map(|p| crate::search::doc(&p.url, &p.meta.title, p.meta.description.as_deref(), &p.html))
        .collect();
    let uses_search = rendered.iter().any(|p| p.html.contains("<mira-search"));
    clock.lap("index");
    let mira_dir = opts.out.join("_mira");
    std::fs::create_dir_all(&mira_dir)?;
    std::fs::write(mira_dir.join("search.json"), serde_json::to_string(&search_docs)?)?;
    if uses_search {
        std::fs::write(mira_dir.join("search.js"), SEARCH_JS)?;
    }
    let media_files = shared.media.write(&opts.out)?;
    if rendered.iter().any(|p| crate::media::Pipeline::uses_video(&p.html)) {
        std::fs::write(mira_dir.join("frame.js"), FRAME_JS)?;
    }
    let (content_files, structured) = content_index(&config, &collections, &shared.data, media_files > 0)?;
    for (path, contents) in content_files {
        let file = mira_dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap())?;
        std::fs::write(file, contents)?;
    }

    let mut pages = Vec::with_capacity(rendered.len());
    let mut metas = Vec::with_capacity(rendered.len());
    for page in rendered {
        if let Some(twin) = &page.meta.twin {
            let file = opts.out.join(twin_url(&page.url).trim_start_matches('/'));
            std::fs::create_dir_all(file.parent().unwrap())?;
            std::fs::write(&file, twin)?;
        }
        let file = if page.url.ends_with(".html") {
            opts.out.join(page.url.trim_start_matches('/'))
        } else {
            opts.out.join(page.url.trim_matches('/')).join("index.html")
        };
        std::fs::create_dir_all(file.parent().unwrap())?;
        std::fs::write(&file, &page.html).with_context(|| format!("writing {}", file.display()))?;
        pages.push(PageReport {
            gzip_bytes: gzip_len(page.html.as_bytes()),
            html_bytes: page.html.len(),
            url: page.url,
            source: page.source,
            morphs: page.morphs,
        });
        metas.push(page.meta);
    }
    pages.sort_by(|a, b| a.url.cmp(&b.url));
    metas.sort_by(|a, b| a.url.cmp(&b.url));
    warnings.extend(crate::lint::seo(&metas));
    let mut outputs = write_site_files(&config, root, &opts.out, &metas, &shared.feeds, &structured, &mut warnings)?;
    outputs.push("_mira/search.json".into());
    outputs.push("_mira/content.json".into());
    for file in host_files.iter().filter(|f| matches!(f.place, crate::hosts::Place::Output)) {
        std::fs::write(opts.out.join(file.name), &file.contents)?;
        outputs.push(file.name.to_string());
    }
    let mut redirect_pages = 0;
    for (from, to) in &config.redirects {
        let url = if from.ends_with('/') { from.clone() } else { format!("{from}/") };
        if pages.iter().any(|p| p.url == url) {
            bail!("mira.config.json: redirects.{from} would replace the page at {url}\nhint: remove the redirect or the page");
        }
        if to.starts_with('/') && !pages.iter().any(|p| p.url == *to) && !opts.out.join(to.trim_start_matches('/')).exists() {
            warnings.push(format!("redirects.{from} points to {to}, which this build does not produce"));
        }
        // `/old.html` is written as that file; `/old` as `/old/index.html`.
        let last = from.trim_end_matches('/').rsplit('/').next().unwrap_or("");
        let file = if last.contains('.') && !from.ends_with('/') {
            opts.out.join(from.trim_start_matches('/'))
        } else {
            opts.out.join(url.trim_matches('/')).join("index.html")
        };
        if file.exists() {
            bail!("mira.config.json: redirects.{from} would replace {}, which the build writes\nhint: remove the redirect", from);
        }
        std::fs::create_dir_all(file.parent().unwrap())?;
        std::fs::write(&file, crate::hosts::redirect_page(to))?;
        redirect_pages += 1;
    }
    if redirect_pages > 0 {
        outputs.push(format!("{redirect_pages} redirect {}", if redirect_pages == 1 { "page" } else { "pages" }));
    }
    if opts.host_config {
        for name in crate::hosts::write_root(&host_files, root)? {
            outputs.push(format!("{name} (project root)"));
        }
    }
    if media_files > 0 {
        outputs.push(format!("media.json ({media_files} {})", if media_files == 1 { "file" } else { "files" }));
    }
    clock.lap("write");

    copy_public(root, &opts.out)?;

    if let Some(budget) = config.budgets.page_kb
        && let Some(page) = pages.iter().find(|p| p.gzip_bytes as f64 > budget * 1024.0)
    {
        bail!(
            "{}: page {} is {:.1}KB gzipped, over the {budget}KB budget\nhint: raise budgets.page_kb in mira.config.json or trim the page",
            page.source,
            page.url,
            page.gzip_bytes as f64 / 1024.0
        );
    }

    Ok(BuildReport {
        build_id,
        pages,
        collections: collections.iter().map(|(k, v)| (k.clone(), v.len())).collect(),
        runtime_gzip_bytes: gzip_len(RUNTIME_JS.as_bytes()),
        duration_ms: started.elapsed().as_secs_f64() * 1000.0,
        out_dir: opts.out.clone(),
        outputs,
        warnings,
        timings: clock.steps,
    })
}

/// Everything identical across pages, computed once per build.
struct Shared<'a> {
    config: &'a Config,
    site: Value,
    base_css: String,
    theme_css: String,
    font_css: String,
    font_preloads: String,
    feeds: Vec<(String, String)>,
    data: Value,
    /// Page titles by URL, for breadcrumbs.
    titles: HashMap<String, String>,
    config_script: String,
    speculation: String,
    favicon: bool,
    public: PathBuf,
    root: PathBuf,
    media: crate::media::Pipeline,
    dev: bool,
    build_id: &'a str,
}

impl<'a> Shared<'a> {
    fn new(config: &'a Config, root: &Path, dev: bool, build_id: &'a str, feeds: Vec<(String, String)>) -> Result<Shared<'a>> {
        let pairs: Vec<Value> = config.transition_pairs()?.into_iter().map(|(from, to, name)| json!([from, to, name])).collect();
        let config_script = json!({ "default": config.transitions.default, "pairs": pairs }).to_string().replace("</", "<\\/");

        let rule = |eagerness: &str| {
            json!({
                "where": { "and": [
                    { "href_matches": "/*" },
                    { "not": { "selector_matches": "[data-mira-no-prefetch], [data-mira-no-prefetch] a, a[download]" } }
                ]},
                "eagerness": eagerness
            })
        };
        let mut speculation = json!({ "prefetch": [rule(&config.prefetch.eagerness)] });
        if config.prefetch.prerender {
            speculation["prerender"] = json!([rule("conservative")]);
        }

        let site = json!({
            "title": config.site.title,
            "description": config.site.description,
            "url": config.site.url,
            "lang": config.site.lang,
        });

        Ok(Shared {
            config,
            site,
            base_css: compact_css(BASE_CSS),
            theme_css: format!("{}{}", scheme_css(config.scheme), theme_css(&config.theme)?),
            font_css: font_css(config, root)?,
            font_preloads: font_preloads(config),
            feeds,
            data: load_data(root)?,
            titles: HashMap::new(),
            config_script,
            speculation: speculation.to_string(),
            favicon: root.join("public/favicon.svg").exists(),
            public: root.join("public"),
            root: root.to_path_buf(),
            media: crate::media::Pipeline::new(root),
            dev,
            build_id,
        })
    }
}

fn render_page(
    shared: &Shared,
    layouts: &HashMap<String, (PathBuf, Component)>,
    collections: &Value,
    route: &Route,
    entry: Option<&Entry>,
) -> Result<Rendered> {
    let source = entry.map_or(&route.source, |e| &e.source);
    let source_name = source.display().to_string();
    let url = route.url(entry.map(|e| e.slug.as_str()));

    let mut page = route.data.clone();
    if let Some(entry) = entry {
        page.extend(entry_json(entry).as_object().cloned().unwrap_or_default());
    }
    page.insert("url".into(), Value::from(url.clone()));
    let ctx = json!({
        "site": shared.site,
        "page": page,
        "entry": entry.map(entry_json),
        "collections": collections,
        "data": shared.data,
    });

    let mut styles = vec![shared.base_css.clone(), shared.font_css.clone(), shared.theme_css.clone()];
    let body = match &route.body {
        RouteBody::Markdown(html, _) => format!("<article class=\"prose\">\n{html}</article>"),
        RouteBody::Component(c) => {
            if let Some(style) = &c.style {
                styles.push(format!("@layer page{{{}}}", compact_css(style)));
            }
            crate::pixel::expand_code(&c.template.render(&ctx, entry.map_or("", |e| &e.html)))
        }
    };
    let route_dir = shared.root.join(route.source.parent().unwrap_or(Path::new("")));
    let (body, frame_css) = shared.media.expand(&body, &route_dir, &url).map_err(|e| anyhow!("{source_name}: {e}"))?;
    styles.push(frame_css);

    let layout_name = match page.get("layout") {
        Some(Value::String(name)) => Some(name.as_str()),
        Some(Value::Bool(false)) | Some(Value::Null) => None,
        None if layouts.contains_key("default") => Some("default"),
        None => None,
        Some(other) => bail!("{source_name}: layout must be a name or false, got {other}"),
    };
    let page_title = page.get("title").and_then(Value::as_str).unwrap_or(&shared.config.site.title).to_string();
    let twin = (shared.config.agents.twins && !route.not_found).then(|| {
        let markdown = match (&route.body, entry) {
            (_, Some(e)) => e.markdown.clone(),
            (RouteBody::Markdown(_, md), None) => md.clone(),
            (RouteBody::Component(_), None) => html_to_markdown(&body),
        };
        twin_document(shared, &page, &page_title, &url, &markdown)
    });
    let html = match layout_name {
        Some(name) => {
            let Some((layout_path, layout)) = layouts.get(name) else {
                let known: Vec<&str> = layouts.keys().map(String::as_str).collect();
                bail!(
                    "{source_name}: layout \"{name}\" does not exist\nhint: add layouts/{name}.mira, or use one of: {}",
                    if known.is_empty() { "(none)".into() } else { known.join(", ") }
                );
            };
            if let Some(style) = &layout.style {
                styles.insert(3, format!("@layer layout{{{}}}", compact_css(style)));
            }
            let html = layout.template.render(&ctx, &body);
            let layout_dir = layout_path.parent().unwrap_or(&shared.root);
            let (html, css) =
                shared.media.expand(&html, layout_dir, &url).map_err(|e| anyhow!("{}: {e}", rel(&shared.root, layout_path)))?;
            styles.push(css);
            html
        }
        None => body,
    };

    let html = crate::pixel::expand_code(&html);
    let html = crate::pixel::expand(&mark_current_links(&html, &url));
    let html = size_images(&html, &shared.public);
    let (html, morph_css) = compile_morphs(&html).map_err(|e| anyhow!("{source_name}: {e}"))?;
    let morphs = morph_css.matches("view-transition-name").count();
    styles.insert(1, components_for(&html));
    styles.push(morph_css);

    let title = match page.get("title").and_then(Value::as_str) {
        Some(t) if t != shared.config.site.title => format!("{t} · {}", shared.config.site.title),
        _ => shared.config.site.title.clone(),
    };
    let description = page.get("description").and_then(Value::as_str).or(shared.config.site.description.as_deref());
    let transition = page.get("transition").and_then(Value::as_str);
    let date = page.get("date").and_then(Value::as_str).map(str::to_string);
    let image = social_image(shared, &page).map_err(|e| anyhow!("{source_name}: {e}"))?;
    let robots = page.get("robots").and_then(Value::as_str).map(str::to_string);
    let noindex = robots.as_deref().is_some_and(|r| r.contains("noindex")) || route.not_found;
    let updated = page.get("updated").and_then(Value::as_str);
    let canonical = page.get("canonical").and_then(Value::as_str);
    let head = extra_head(
        shared,
        &PageInfo {
            image: image.as_ref(),
            robots: robots.as_deref(),
            updated,
            author: page.get("author").and_then(Value::as_str),
            faq: page.get("faq"),
            url: &url,
            title: &page_title,
            description,
            date: date.as_deref(),
            is_entry: entry.is_some(),
            has_twin: twin.is_some(),
        },
    );

    let html = document(
        shared,
        &DocParts { title: &title, description, url: &url, transition, style: &styles.concat(), head: &head, body: &html, canonical },
    );
    let meta = PageMeta {
        url: url.clone(),
        title: page_title,
        description: description.map(str::to_string),
        lastmod: updated.map(str::to_string).or(date.clone()),
        date,
        collection: route.collection.clone(),
        twin,
        not_found: route.not_found,
        noindex,
        own_description: page.get("description").and_then(Value::as_str).map(str::to_string),
        document_title: title.clone(),
    };
    Ok(Rendered { url, source: source_name, html, morphs, meta })
}

struct DocParts<'a> {
    title: &'a str,
    description: Option<&'a str>,
    url: &'a str,
    transition: Option<&'a str>,
    style: &'a str,
    head: &'a str,
    body: &'a str,
    /// A `canonical` frontmatter override, absolute or a site path.
    canonical: Option<&'a str>,
}

fn document(shared: &Shared, p: &DocParts) -> String {
    let runtime: &str = &RUNTIME_JS;
    let csp = format!(
        "default-src 'self'; script-src 'self' {} {}; style-src 'self' {}; img-src 'self' data: https:; object-src 'none'; base-uri 'self'; form-action 'self'",
        csp_hash(runtime),
        csp_hash(&shared.speculation),
        csp_hash(p.style),
    );

    let mut head = String::new();
    let mut line = |s: &str| {
        head.push_str(s);
        head.push('\n');
    };
    line("<meta charset=\"utf-8\">");
    line("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    line(&format!("<meta http-equiv=\"Content-Security-Policy\" content=\"{csp}\">"));
    line(&format!("<title>{}</title>", escape(p.title)));
    if let Some(d) = p.description {
        line(&format!("<meta name=\"description\" content=\"{}\">", escape(d)));
    }
    match (p.canonical, &shared.config.site.url) {
        (Some(c), _) if c.starts_with("http") => line(&format!("<link rel=\"canonical\" href=\"{}\">", escape(c))),
        (Some(c), Some(origin)) => {
            line(&format!("<link rel=\"canonical\" href=\"{}{}\">", escape(origin.trim_end_matches('/')), escape(c)))
        }
        (None, Some(origin)) => {
            line(&format!("<link rel=\"canonical\" href=\"{}{}\">", escape(origin.trim_end_matches('/')), escape(p.url)))
        }
        _ => {}
    }
    if shared.favicon {
        line("<link rel=\"icon\" href=\"/favicon.svg\" type=\"image/svg+xml\">");
    }
    if shared.public.join("apple-touch-icon.png").is_file() {
        line("<link rel=\"apple-touch-icon\" href=\"/apple-touch-icon.png\">");
    }
    if let Some(color) = &shared.config.site.theme_color {
        line(&format!("<meta name=\"theme-color\" content=\"{}\">", escape(color)));
    }
    line(p.head.trim_end());
    if let Some(t) = p.transition {
        line(&format!("<meta name=\"mira-transition\" content=\"{}\">", escape(t)));
    }
    if shared.dev {
        line(&format!("<meta name=\"mira-build\" content=\"{}\">", shared.build_id));
    }
    line(&format!("<style>{}</style>", p.style));
    line(&format!("<script type=\"application/json\" id=\"mira-config\">{}</script>", shared.config_script));
    line(&format!("<script>{runtime}</script>"));
    line(&format!("<script type=\"speculationrules\">{}</script>", shared.speculation));
    // Hold the first render until <main> is parsed so shared elements exist
    // when the incoming view transition snapshots the page.
    if p.body.contains("id=\"main\"") {
        line("<link rel=\"expect\" href=\"#main\" blocking=\"render\">");
    }
    if p.body.contains("mira-frame--video") {
        line("<script type=\"module\" src=\"/_mira/frame.js\"></script>");
    }
    if p.body.contains("<mira-search") {
        line("<script type=\"module\" src=\"/_mira/search.js\"></script>");
    }
    if shared.dev {
        line("<script type=\"module\" src=\"/_mira/dev.js\"></script>");
    }

    format!(
        "<!doctype html>\n<html lang=\"{}\">\n<head>\n{head}</head>\n<body>\n{}\n</body>\n</html>\n",
        escape(&shared.config.site.lang),
        p.body.trim()
    )
}

struct PageInfo<'a> {
    image: Option<&'a SocialImage>,
    robots: Option<&'a str>,
    updated: Option<&'a str>,
    author: Option<&'a str>,
    faq: Option<&'a Value>,
    url: &'a str,
    title: &'a str,
    description: Option<&'a str>,
    date: Option<&'a str>,
    is_entry: bool,
    has_twin: bool,
}

/// Agent and social metadata: Markdown twin and feed links, Open Graph,
/// and JSON-LD for collection entries.
fn extra_head(shared: &Shared, p: &PageInfo) -> String {
    let config = shared.config;
    let mut head = String::new();
    let mut add = |s: String| {
        head.push_str(&s);
        head.push('\n');
    };
    if p.has_twin {
        add(format!("<link rel=\"alternate\" type=\"text/markdown\" href=\"{}\">", escape(&twin_url(p.url))));
    }
    for (collection, feed) in &shared.feeds {
        add(format!(
            "<link rel=\"alternate\" type=\"application/rss+xml\" title=\"{} · {}\" href=\"{}\">",
            escape(&config.site.title),
            escape(collection),
            escape(feed)
        ));
    }
    if let Some(robots) = p.robots {
        add(format!("<meta name=\"robots\" content=\"{}\">", escape(robots)));
    }
    add(format!("<meta property=\"og:title\" content=\"{}\">", escape(p.title)));
    add(format!("<meta property=\"og:site_name\" content=\"{}\">", escape(&config.site.title)));
    add(format!("<meta property=\"og:type\" content=\"{}\">", if p.is_entry { "article" } else { "website" }));
    if let Some(d) = p.description {
        add(format!("<meta property=\"og:description\" content=\"{}\">", escape(d)));
    }
    let absolute = crate::outputs::absolute(config, p.url);
    if let Some(abs) = &absolute {
        add(format!("<meta property=\"og:url\" content=\"{}\">", escape(abs)));
    }
    match p.image {
        Some(image) => {
            let src = crate::outputs::absolute(config, &image.path).unwrap_or_else(|| image.path.clone());
            add(format!("<meta property=\"og:image\" content=\"{}\">", escape(&src)));
            if let Some((w, h)) = image.size {
                add(format!("<meta property=\"og:image:width\" content=\"{w}\">"));
                add(format!("<meta property=\"og:image:height\" content=\"{h}\">"));
            }
            if let Some(alt) = &image.alt {
                add(format!("<meta property=\"og:image:alt\" content=\"{}\">", escape(alt)));
                add(format!("<meta name=\"twitter:image:alt\" content=\"{}\">", escape(alt)));
            }
            add("<meta name=\"twitter:card\" content=\"summary_large_image\">".into());
            add(format!("<meta name=\"twitter:image\" content=\"{}\">", escape(&src)));
        }
        None => add("<meta name=\"twitter:card\" content=\"summary\">".into()),
    }
    if let Some(handle) = &config.site.twitter {
        add(format!("<meta name=\"twitter:site\" content=\"{}\">", escape(handle)));
        add(format!("<meta name=\"twitter:creator\" content=\"{}\">", escape(handle)));
    }
    if p.url == "/"
        && let Some(home) = &absolute
    {
        let mut org = json!({ "@type": "Organization", "name": config.site.title, "url": home });
        if !config.site.same_as.is_empty() {
            org["sameAs"] = json!(config.site.same_as);
        }
        if shared.favicon {
            org["logo"] = json!(crate::outputs::absolute(config, "/favicon.svg"));
        }
        let mut site = json!({ "@type": "WebSite", "name": config.site.title, "url": home });
        if let Some(d) = &config.site.description {
            site["description"] = json!(d);
        }
        let ld = json!({ "@context": "https://schema.org", "@graph": [site, org] });
        add(format!("<script type=\"application/ld+json\">{}</script>", ld.to_string().replace("</", "<\\/")));
    }
    if p.is_entry {
        let mut ld = json!({
            "@context": "https://schema.org",
            "@type": "BlogPosting",
            "headline": p.title,
        });
        if let Some(d) = p.description {
            ld["description"] = json!(d);
        }
        if let Some(date) = p.date {
            ld["datePublished"] = json!(date);
        }
        if let Some(updated) = p.updated.or(p.date) {
            ld["dateModified"] = json!(updated);
        }
        if let Some(abs) = &absolute {
            ld["url"] = json!(abs);
            ld["mainEntityOfPage"] = json!(abs);
        }
        if let Some(image) = p.image {
            ld["image"] = json!(crate::outputs::absolute(config, &image.path).unwrap_or_else(|| image.path.clone()));
        }
        ld["author"] = match p.author {
            Some(name) => json!({ "@type": "Person", "name": name }),
            None => json!({ "@type": "Organization", "name": config.site.title }),
        };
        add(format!("<script type=\"application/ld+json\">{}</script>", ld.to_string().replace("</", "<\\/")));
    }
    if let Some(ld) = breadcrumbs(shared, p.url, p.title) {
        add(format!("<script type=\"application/ld+json\">{}</script>", ld.to_string().replace("</", "<\\/")));
    }
    if let Some(ld) = faq(p.faq) {
        add(format!("<script type=\"application/ld+json\">{}</script>", ld.to_string().replace("</", "<\\/")));
    }
    head.push_str(&shared.font_preloads);
    head
}

/// BreadcrumbList for pages below the root: Home, each section, the page.
fn breadcrumbs(shared: &Shared, url: &str, title: &str) -> Option<Value> {
    let config = shared.config;
    crate::outputs::absolute(config, "/")?;
    let parts: Vec<&str> = url.trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() || url.ends_with(".html") {
        return None;
    }
    let mut items =
        vec![json!({ "@type": "ListItem", "position": 1, "name": config.site.title, "item": crate::outputs::absolute(config, "/") })];
    let mut path = String::from("/");
    for (i, part) in parts.iter().enumerate() {
        path.push_str(part);
        path.push('/');
        let last = i + 1 == parts.len();
        let name = if last {
            title.to_string()
        } else {
            shared.titles.get(&path).cloned().unwrap_or_else(|| {
                let mut c = part.chars();
                c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
            })
        };
        items.push(json!({ "@type": "ListItem", "position": i + 2, "name": name, "item": crate::outputs::absolute(config, &path) }));
    }
    Some(json!({ "@context": "https://schema.org", "@type": "BreadcrumbList", "itemListElement": items }))
}

/// FAQPage from a `faq` frontmatter list of `{ q, a }`.
fn faq(value: Option<&Value>) -> Option<Value> {
    let items = value?.as_array()?;
    let questions: Vec<Value> = items
        .iter()
        .filter_map(|item| {
            let q = item.get("q").or_else(|| item.get("question"))?.as_str()?;
            let a = item.get("a").or_else(|| item.get("answer"))?.as_str()?;
            Some(json!({ "@type": "Question", "name": q, "acceptedAnswer": { "@type": "Answer", "text": a } }))
        })
        .collect();
    (!questions.is_empty()).then(|| json!({ "@context": "https://schema.org", "@type": "FAQPage", "mainEntity": questions }))
}

pub(crate) struct SocialImage {
    path: String,
    size: Option<(usize, usize)>,
    alt: Option<String>,
}

/// The page's `image` frontmatter, or `site.image`. The file must exist
/// under `public/`; its pixel size is read for the Open Graph tags.
fn social_image(shared: &Shared, page: &Map<String, Value>) -> Result<Option<SocialImage>> {
    let site = &shared.config.site;
    let (path, alt) = match page.get("image").and_then(Value::as_str) {
        Some(p) => (p.to_string(), page.get("image_alt").and_then(Value::as_str).map(str::to_string)),
        None => match &site.image {
            Some(p) => (p.clone(), site.image_alt.clone()),
            None => return Ok(None),
        },
    };
    if !path.starts_with('/') || path.contains("..") {
        bail!("image {path:?} must be a path under public/ starting with /");
    }
    let file = shared.public.join(path.trim_start_matches('/'));
    if !file.is_file() {
        bail!("image {path} does not exist\nhint: put the file at public{path}");
    }
    let size = imagesize::size(&file).ok().map(|s| (s.width, s.height));
    Ok(Some(SocialImage { path, size, alt }))
}

/// A Markdown twin: frontmatter with the canonical URL, then the body,
/// titled when the source does not start with a heading.
fn twin_document(shared: &Shared, page: &Map<String, Value>, title: &str, url: &str, markdown: &str) -> String {
    let canonical = crate::outputs::absolute(shared.config, url).unwrap_or_else(|| url.to_string());
    let mut out = String::from("---\n");
    out.push_str(&format!("title: {}\n", Value::from(title)));
    out.push_str(&format!("url: {}\n", Value::from(canonical)));
    for key in ["description", "date"] {
        if let Some(Value::String(v)) = page.get(key) {
            out.push_str(&format!("{key}: {}\n", Value::from(v.as_str())));
        }
    }
    out.push_str("---\n\n");
    let body = markdown.trim();
    // Look for a top level heading outside fenced code.
    let mut fenced = false;
    let has_h1 = body.lines().any(|l| {
        if l.trim_start().starts_with("```") || l.trim_start().starts_with("~~~") {
            fenced = !fenced;
        }
        !fenced && l.starts_with("# ")
    });
    if !has_h1 {
        out.push_str(&format!("# {title}\n\n"));
    }
    out.push_str(body);
    out.push('\n');
    out
}

fn font_css(config: &Config, root: &Path) -> Result<String> {
    let mut css = String::new();
    for font in &config.fonts {
        let file = root.join("public").join(font.src.trim_start_matches('/'));
        if !file.is_file() {
            bail!("mira.config.json: font file {} does not exist\nhint: put the file at public{} or fix fonts[].src", font.src, font.src);
        }
        let format = if font.src.ends_with(".woff2") {
            "woff2"
        } else if font.src.ends_with(".woff") {
            "woff"
        } else {
            "truetype"
        };
        css.push_str(&format!(
            "@font-face{{font-family:\"{}\";src:url(\"{}\") format(\"{format}\");font-weight:{};font-style:{};font-display:swap}}",
            font.family, font.src, font.weight, font.style
        ));
    }
    Ok(css)
}

fn font_preloads(config: &Config) -> String {
    config
        .fonts
        .iter()
        .filter(|f| f.preload)
        .map(|f| format!("<link rel=\"preload\" href=\"{}\" as=\"font\" type=\"font/woff2\" crossorigin>\n", escape(&f.src)))
        .collect()
}

/// Writes sitemap, feeds, llms.txt, robots.txt, and headers. Files the
/// project provides in `public/` win over generated ones.
fn write_site_files(
    config: &Config,
    root: &Path,
    out: &Path,
    pages: &[PageMeta],
    feeds: &[(String, String)],
    structured: &[(String, String)],
    warnings: &mut Vec<String>,
) -> Result<Vec<String>> {
    let mut files: Vec<(String, String)> = Vec::new();
    let sitemap = crate::outputs::sitemap(config, pages);
    let has_sitemap = sitemap.is_some();
    if let Some(xml) = sitemap {
        files.push(("sitemap.xml".into(), xml));
    }
    for (collection, feed) in feeds {
        if let Some(xml) = crate::outputs::rss(config, collection, feed, pages) {
            files.push((feed.trim_start_matches('/').to_string(), xml));
        }
    }
    if config.agents.twins {
        let full = crate::outputs::llms_full_txt(config, pages);
        let has_media = out.join("media.json").is_file();
        files.push(("llms.txt".into(), crate::outputs::llms_txt(config, pages, has_media, crate::outputs::tokens(&full), structured)));
        files.push(("llms-full.txt".into(), full));
    }
    files.push(("robots.txt".into(), crate::outputs::robots_txt(config, has_sitemap)));
    if config.headers.emit {
        files.push(("_headers".into(), crate::outputs::headers_file(config)));
        files.push(("vercel.json".into(), crate::hosts::vercel(config, None)));
    }

    let mut written = Vec::new();
    for (path, contents) in files {
        if root.join("public").join(&path).exists() {
            warnings.push(format!("public/{path} exists, so the generated {path} was skipped"));
            continue;
        }
        let file = out.join(&path);
        std::fs::create_dir_all(file.parent().unwrap())?;
        std::fs::write(&file, contents)?;
        written.push(path);
    }
    Ok(written)
}

/// Warns on every `{{ unsafe ... }}` in templates, so raw HTML stays a
/// visible, deliberate choice.
fn unsafe_uses(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    for dir in ["routes", "layouts"] {
        for entry in WalkDir::new(root.join(dir)).into_iter().filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "mira") {
                continue;
            }
            let Ok(src) = std::fs::read_to_string(path) else { continue };
            for (i, line) in src.lines().enumerate() {
                if line.contains("{{ unsafe ") || line.contains("{{unsafe ") {
                    found.push(format!("{}:{}: renders raw HTML with unsafe; make sure the value is trusted", rel(root, path), i + 1));
                }
            }
        }
    }
    found
}

/// Loads `data/*.json`, `*.yaml`, and `*.yml` into `data.<file stem>`.
fn load_data(root: &Path) -> Result<Value> {
    let mut data = Map::new();
    let dir = root.join("data");
    if !dir.is_dir() {
        return Ok(Value::Object(data));
    }
    for item in std::fs::read_dir(&dir)? {
        let path = item?.path();
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else { continue };
        let shown = rel(root, &path);
        let src = std::fs::read_to_string(&path)?;
        let value: Value = match ext {
            "json" => serde_json::from_str(&src).map_err(|e| anyhow!("{shown}:{}: invalid JSON: {e}", e.line()))?,
            "yaml" | "yml" => {
                let yaml: serde_yaml::Value = serde_yaml::from_str(&src)
                    .map_err(|e| anyhow!("{shown}:{}: invalid YAML: {e}", e.location().map_or(1, |l| l.line())))?;
                serde_json::to_value(yaml)?
            }
            _ => continue,
        };
        let name = path.file_stem().unwrap().to_string_lossy().replace('-', "_");
        data.insert(name, value);
    }
    Ok(Value::Object(data))
}

/// Files as `(path, contents)`, or listings as `(path, description)`.
type Files = Vec<(String, String)>;

/// The site's structured content for agents, as files under `_mira/`:
/// `content.json` describes the site and lists every collection and data
/// file, `collections/<name>.json` holds published entries with their
/// fields, and `data/<name>.json` each data file. `mira mcp` reads the same
/// files locally and from a deployed site, so both answer alike.
///
/// Also returns `(path, description)` for each JSON file, for llms.txt.
fn content_index(config: &Config, collections: &BTreeMap<String, Vec<Entry>>, data: &Value, has_media: bool) -> Result<(Files, Files)> {
    let mut files = Vec::new();
    let mut listed = Vec::new();
    let mut listed_collections = Vec::new();
    let mut listed_data = Vec::new();
    if config.agents.content {
        for (name, entries) in collections {
            let published: Vec<Value> = entries
                .iter()
                .filter(|e| !e.data.get("robots").and_then(Value::as_str).is_some_and(|r| r.contains("noindex")))
                .map(|e| {
                    let mut map = entry_json(e);
                    if let Value::Object(m) = &mut map {
                        // Derived for templates; agents get the source fields.
                        for derived in ["prev", "next", "toc", "reading_time"] {
                            m.remove(derived);
                        }
                        // Entries without a page of their own carry their body here.
                        if e.url.is_none() {
                            m.insert("markdown".into(), Value::from(e.markdown.trim().to_string()));
                        }
                    }
                    map
                })
                .collect();
            let schema = config.collections.get(name);
            let fields = schema.map(|s| serde_json::to_value(&s.fields)).transpose()?;
            let path = format!("/_mira/collections/{name}.json");
            let field_names: Vec<&str> = schema.map(|s| s.fields.keys().map(String::as_str).collect()).unwrap_or_default();
            let count = published.len();
            listed.push((
                path.clone(),
                match field_names.is_empty() {
                    true => format!("{name}, {count} entries"),
                    false => format!("{name}, {count} entries with {}", field_names.join(", ")),
                },
            ));
            listed_collections.push(json!({ "name": name, "count": count, "fields": fields, "url": path }));
            files.push((format!("collections/{name}.json"), serde_json::to_string(&published)?));
        }
        if let Value::Object(map) = data {
            for (name, value) in map {
                let path = format!("/_mira/data/{name}.json");
                listed.push((path.clone(), format!("{name}, a data file")));
                listed_data.push(json!({ "name": name, "url": path }));
                files.push((format!("data/{name}.json"), serde_json::to_string(value)?));
            }
        }
    }
    let index = json!({
        "schema": 1,
        "generator": format!("mira {}", env!("CARGO_PKG_VERSION")),
        "site": {
            "title": config.site.title,
            "description": config.site.description,
            "url": config.site.url,
            "lang": config.site.lang,
        },
        "pages": "/_mira/search.json",
        "media": has_media.then_some("/media.json"),
        "collections": listed_collections,
        "data": listed_data,
    });
    files.push(("content.json".into(), serde_json::to_string(&index)?));
    Ok((files, listed))
}

fn entry_json(entry: &Entry) -> Value {
    let mut map = entry.data.clone();
    map.insert("slug".into(), Value::from(entry.slug.clone()));
    map.insert("url".into(), entry.url.clone().map_or(Value::Null, Value::from));
    Value::Object(map)
}

fn scheme_css(scheme: crate::config::Scheme) -> &'static str {
    use crate::config::Scheme;
    match scheme {
        Scheme::Dark => "",
        Scheme::Light => {
            "@layer mira.theme{:root{color-scheme:light;--lift:0 1px 0 rgba(255,255,255,.9) inset,0 18px 44px rgba(11,11,13,.14)}}"
        }
        Scheme::System => "@layer mira.theme{:root{color-scheme:light dark}}",
    }
}

fn theme_css(theme: &Map<String, Value>) -> Result<String> {
    fn walk(prefix: &str, value: &Value, out: &mut String) -> Result<()> {
        match value {
            Value::Object(map) => {
                for (k, v) in map {
                    let name = if prefix.is_empty() { k.clone() } else { format!("{prefix}-{k}") };
                    walk(&name, v, out)?;
                }
            }
            Value::String(_) | Value::Number(_) => {
                let v = match value {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                if v.contains(['{', '}', ';', '<']) || !prefix.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                    bail!("mira.config.json: theme token \"{prefix}\" has an invalid name or value");
                }
                out.push_str(&format!("--{prefix}:{v};"));
            }
            _ => bail!("mira.config.json: theme token \"{prefix}\" must be a string, number, or group"),
        }
        Ok(())
    }
    if theme.is_empty() {
        return Ok(String::new());
    }
    let mut vars = String::new();
    walk("", &Value::Object(theme.clone()), &mut vars)?;
    Ok(format!("@layer mira.theme{{:root{{{vars}}}}}"))
}

fn load_layouts(root: &Path) -> Result<HashMap<String, (PathBuf, Component)>> {
    let dir = root.join("layouts");
    let mut layouts = HashMap::new();
    if !dir.exists() {
        return Ok(layouts);
    }
    for item in std::fs::read_dir(&dir)? {
        let path = item?.path();
        if path.extension().is_some_and(|e| e == "mira") {
            let name = path.file_stem().unwrap().to_string_lossy().to_string();
            let src = std::fs::read_to_string(&path)?;
            let component = parse_component(&src, &rel_path(root, &path))?;
            layouts.insert(name, (path, component));
        }
    }
    Ok(layouts)
}

fn load_collections(root: &Path, include_drafts: bool, config: &Config) -> Result<BTreeMap<String, Vec<Entry>>> {
    let dir = root.join("content");
    let mut collections = BTreeMap::new();
    if !dir.exists() {
        return Ok(collections);
    }
    for item in std::fs::read_dir(&dir)? {
        let item = item?;
        if !item.file_type()?.is_dir() {
            continue;
        }
        let name = item.file_name().to_string_lossy().to_string();
        let files: Vec<PathBuf> = WalkDir::new(item.path())
            .into_iter()
            .filter_map(Result::ok)
            .map(|e| e.into_path())
            .filter(|p| p.extension().is_some_and(|e| e == "md"))
            .collect();
        let mut entries: Vec<Entry> = files
            .par_iter()
            .map(|path| load_entry(root, path, config.collections.get(&name)))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .filter(|e| include_drafts || e.data.get("draft") != Some(&Value::Bool(true)))
            .collect();
        // Entries with an `order` come first, ascending; the rest follow
        // newest first by `date`, then by slug.
        entries.sort_by(|a, b| {
            let order = |e: &Entry| e.data.get("order").and_then(Value::as_f64).unwrap_or(f64::INFINITY);
            let date = |e: &Entry| e.data.get("date").and_then(Value::as_str).unwrap_or("").to_string();
            order(a).total_cmp(&order(b)).then_with(|| date(b).cmp(&date(a))).then_with(|| a.slug.cmp(&b.slug))
        });
        collections.insert(name, entries);
    }
    Ok(collections)
}

fn load_entry(root: &Path, path: &Path, schema: Option<&crate::schema::Schema>) -> Result<Entry> {
    let src = std::fs::read_to_string(path)?;
    let shown = rel_path(root, path);
    let doc = parse_document(&src, &shown)?;
    if let Some(schema) = schema {
        schema.validate(&doc.data, &src.replace("\r\n", "\n"), &shown.display().to_string())?;
    }
    let md = render_markdown(&doc.body);
    let slug = match doc.data.get("slug") {
        Some(Value::String(s)) => s.clone(),
        Some(_) => bail!("{}:2: slug must be a string", shown.display()),
        None => path.file_stem().unwrap().to_string_lossy().to_string(),
    };
    if slug.is_empty() || slug.contains(['/', '\\', '?', '#', ' ']) {
        bail!("{}: slug {slug:?} must be non-empty with no spaces, slashes, ? or #", shown.display());
    }
    let mut data = doc.data;
    data.entry("reading_time").or_insert(Value::from(reading_time(md.words)));
    data.insert("toc".into(), serde_json::to_value(&md.toc)?);
    let html = crate::media::rebase(&md.html, shown.parent().unwrap_or(Path::new("")));
    Ok(Entry { slug, source: shown, data, html, markdown: doc.body, url: None })
}

/// Loads every route. Routes with `draft: true` are kept only in dev.
fn load_routes(root: &Path, include_drafts: bool) -> Result<Vec<Route>> {
    let dir = root.join("routes");
    if !dir.exists() {
        bail!("{}: no routes/ directory\nhint: run `mira new` to scaffold a project, or create routes/index.md", root.display());
    }
    let mut routes = Vec::new();
    for item in WalkDir::new(&dir).sort_by_file_name() {
        let path = item?.into_path();
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else { continue };
        if !matches!(ext, "md" | "mira") || !path.is_file() {
            continue;
        }
        let shown = rel_path(root, &path);
        let src = std::fs::read_to_string(&path)?;
        let rel = path.strip_prefix(&dir)?;
        let mut parts: Vec<String> = rel.parent().into_iter().flat_map(|p| p.iter()).map(|s| s.to_string_lossy().to_string()).collect();
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let not_found = parts.is_empty() && stem == "404";
        let dynamic = stem.starts_with('[') && stem.ends_with(']');
        if stem != "index" && !dynamic {
            parts.push(stem.clone());
        }
        let mut segments: Vec<Option<String>> = parts.into_iter().map(Some).collect();
        if dynamic {
            segments.push(None);
        }

        let (data, body) = if ext == "md" {
            let doc = parse_document(&src, &shown)?;
            let md = render_markdown(&doc.body);
            let mut data = doc.data;
            data.insert("toc".into(), serde_json::to_value(&md.toc)?);
            let html = crate::media::rebase(&md.html, shown.parent().unwrap_or(Path::new("")));
            (data, RouteBody::Markdown(html, doc.body))
        } else {
            let mut c = parse_component(&src, &shown)?;
            (std::mem::take(&mut c.data), RouteBody::Component(c))
        };

        let collection = if dynamic {
            match data.get("collection").and_then(Value::as_str) {
                Some(name) => Some(name.to_string()),
                None => match rel.parent().and_then(|p| p.file_name()) {
                    Some(dir) => Some(dir.to_string_lossy().to_string()),
                    None => {
                        bail!("{}: dynamic route needs a collection\nhint: set `collection: posts` in its frontmatter", shown.display())
                    }
                },
            }
        } else {
            None
        };
        if dynamic && ext == "md" {
            bail!("{}: dynamic routes must be .mira components", shown.display());
        }
        if !include_drafts && data.get("draft") == Some(&Value::Bool(true)) {
            continue;
        }
        routes.push(Route { source: shown, data, body, segments, collection, not_found });
    }
    if !routes.iter().any(|r| r.not_found) {
        let source = PathBuf::from("(mira) 404.mira");
        let mut c = parse_component(NOT_FOUND_MIRA, &source)?;
        let data = std::mem::take(&mut c.data);
        routes.push(Route { source, data, body: RouteBody::Component(c), segments: Vec::new(), collection: None, not_found: true });
    }
    Ok(routes)
}

fn prepare_out_dir(out: &Path) -> Result<()> {
    if out.exists() {
        let empty = std::fs::read_dir(out)?.next().is_none();
        if !empty && !out.join(OUTPUT_MARKER).exists() {
            bail!(
                "{}: refusing to clear a directory Mira did not create\nhint: pick an empty --out directory or delete it yourself",
                out.display()
            );
        }
        std::fs::remove_dir_all(out).with_context(|| format!("clearing {}", out.display()))?;
    }
    std::fs::create_dir_all(out)?;
    std::fs::write(out.join(OUTPUT_MARKER), "Created by mira build. Safe to delete.\n")?;
    Ok(())
}

fn copy_public(root: &Path, out: &Path) -> Result<()> {
    let dir = root.join("public");
    if !dir.exists() {
        return Ok(());
    }
    for item in WalkDir::new(&dir) {
        let path = item?.into_path();
        let target = out.join(path.strip_prefix(&dir)?);
        if path.is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            std::fs::copy(&path, &target).with_context(|| format!("copying {}", path.display()))?;
        }
    }
    Ok(())
}

fn gzip_len(bytes: &[u8]) -> usize {
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(bytes).unwrap();
    gz.finish().unwrap().len()
}

fn rel_path(root: &Path, path: &Path) -> PathBuf {
    let rel = path.strip_prefix(root).unwrap_or(path);
    PathBuf::from(rel.to_string_lossy().replace('\\', "/"))
}

fn rel(root: &Path, path: &Path) -> String {
    rel_path(root, path).display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_full_builds_write_host_config() {
        let root = std::env::temp_dir().join(format!("mira-build-{}-host-config", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        crate::scaffold::scaffold(&root).unwrap();
        let config = root.join("mira.config.json");
        let mut value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&config).unwrap()).unwrap();
        value["hosts"] = serde_json::json!({ "netlify": {} });
        std::fs::write(&config, value.to_string()).unwrap();

        // A dev build into a private folder leaves the project root alone.
        build(&BuildOptions { root: root.clone(), out: root.join(".mira/dev"), dev: true, host_config: false }).unwrap();
        assert!(!root.join("netlify.toml").exists());

        build(&BuildOptions { root: root.clone(), out: root.join("dist"), dev: false, host_config: true }).unwrap();
        let toml = std::fs::read_to_string(root.join("netlify.toml")).unwrap();
        assert!(toml.contains("publish = \"dist\""), "{toml}");
    }

    #[test]
    fn publishes_content_for_agents() {
        let root = std::env::temp_dir().join(format!("mira-build-{}-content", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        crate::scaffold::scaffold(&root).unwrap();
        std::fs::create_dir_all(root.join("data")).unwrap();
        std::fs::write(root.join("data/team.json"), r#"[{"name": "Ada"}]"#).unwrap();
        let out = root.join("dist");
        build(&BuildOptions { root: root.clone(), out: out.clone(), dev: false, host_config: true }).unwrap();

        let index: Value = serde_json::from_str(&std::fs::read_to_string(out.join("_mira/content.json")).unwrap()).unwrap();
        assert_eq!(index["pages"], "/_mira/search.json");
        let posts = &index["collections"][0];
        assert_eq!(posts["name"], "posts");
        assert_eq!(posts["count"], 2);
        assert_eq!(posts["fields"]["title"], "string");
        assert_eq!(index["data"][0]["url"], "/_mira/data/team.json");

        let entries: Vec<Value> =
            serde_json::from_str(&std::fs::read_to_string(out.join("_mira/collections/posts.json")).unwrap()).unwrap();
        assert!(entries.iter().all(|e| e["url"].as_str().is_some_and(|u| u.starts_with("/posts/")) && e.get("toc").is_none()));
        let team: Value = serde_json::from_str(&std::fs::read_to_string(out.join("_mira/data/team.json")).unwrap()).unwrap();
        assert_eq!(team[0]["name"], "Ada");

        // Turned off, only the site description and pages remain.
        let config = root.join("mira.config.json");
        let mut value: Value = serde_json::from_str(&std::fs::read_to_string(&config).unwrap()).unwrap();
        value["agents"] = json!({ "content": false });
        std::fs::write(&config, value.to_string()).unwrap();
        build(&BuildOptions { root: root.clone(), out: out.clone(), dev: false, host_config: true }).unwrap();
        let index: Value = serde_json::from_str(&std::fs::read_to_string(out.join("_mira/content.json")).unwrap()).unwrap();
        assert_eq!(index["collections"], json!([]));
        assert!(!out.join("_mira/data/team.json").exists());
    }
}
