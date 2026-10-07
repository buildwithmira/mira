use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

pub const CONFIG_FILE: &str = "mira.config.json";

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub site: Site,
    /// `dark` (the default), `light`, or `system` to follow the reader.
    pub scheme: Scheme,
    pub transitions: Transitions,
    pub prefetch: Prefetch,
    /// Nested token overrides, flattened to CSS custom properties:
    /// `{"color": {"accent": "oklch(..)"}}` becomes `--color-accent`.
    pub theme: serde_json::Map<String, serde_json::Value>,
    pub budgets: Budgets,
    pub agents: Agents,
    pub fonts: Vec<FontFace>,
    pub headers: Headers,
    /// Hosts to write native config for, keyed by host name, each with
    /// optional settings: `{ "vercel": {}, "cloudflare": { "project": "x" } }`.
    pub hosts: BTreeMap<String, serde_json::Value>,
    /// Permanent redirects from old paths to new paths or URLs, written in
    /// every host's native form, plus redirect pages for hosts without one.
    pub redirects: BTreeMap<String, String>,
    /// Frontmatter schemas keyed by collection name.
    pub collections: BTreeMap<String, crate::schema::Schema>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    #[default]
    Dark,
    Light,
    System,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Agents {
    /// Emit a Markdown twin for every page at `<page>.md`.
    pub twins: bool,
    /// Crawler policy emitted as robots.txt, keyed by user agent with the
    /// value `allow` or `disallow`. `*` covers every other agent.
    pub robots: BTreeMap<String, String>,
    /// Search engine indexing: `allow` or `disallow`.
    pub search: String,
    /// AI answer engines that fetch pages to answer questions and cite
    /// them (ChatGPT search, Claude, Perplexity): `allow` or `disallow`.
    pub answers: String,
    /// Crawlers that collect pages to train AI models: `allow` or `disallow`.
    pub training: String,
}

impl Default for Agents {
    fn default() -> Self {
        Agents {
            twins: true,
            robots: BTreeMap::from([("*".into(), "allow".into())]),
            search: "allow".into(),
            answers: "allow".into(),
            training: "allow".into(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontFace {
    pub family: String,
    /// Path under `public/`, served from the site root, e.g. `/fonts/Geist.woff2`.
    pub src: String,
    /// A single weight (`400`) or a variable range (`100 900`).
    #[serde(default = "default_weight")]
    pub weight: String,
    #[serde(default = "default_style")]
    pub style: String,
    /// Preload the file. Use for the one or two faces above the fold.
    #[serde(default)]
    pub preload: bool,
}

fn default_weight() -> String {
    "400".into()
}

fn default_style() -> String {
    "normal".into()
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Headers {
    /// Write a `_headers` file with security headers for hosts that read it.
    pub emit: bool,
    /// Send `Strict-Transport-Security`. Only enable once HTTPS is permanent.
    pub hsts: bool,
}

impl Default for Headers {
    fn default() -> Self {
        Headers { emit: true, hsts: true }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Site {
    pub title: String,
    pub description: Option<String>,
    /// Absolute origin used for canonical URLs, e.g. `https://example.com`.
    pub url: Option<String>,
    pub lang: String,
    /// Social preview image under `public/`, e.g. `/og.png` (1200 by 630).
    pub image: Option<String>,
    /// Alt text for the social preview image.
    pub image_alt: Option<String>,
    /// The site's X (Twitter) handle, e.g. `@mira`.
    pub twitter: Option<String>,
    /// Browser UI color, e.g. `#0b0b0d`.
    pub theme_color: Option<String>,
    /// Profile URLs for the site's structured data, such as GitHub or X.
    pub same_as: Vec<String>,
}

impl Default for Site {
    fn default() -> Self {
        Site {
            title: "Mira site".into(),
            description: None,
            url: None,
            lang: "en".into(),
            image: None,
            image_alt: None,
            twitter: None,
            theme_color: None,
            same_as: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Transitions {
    /// Transition used when no pair or page override applies.
    pub default: String,
    /// Route pair overrides keyed `"<from> -> <to>"`. Patterns are exact
    /// paths or a prefix ending in `*`. Navigating the pair in reverse plays
    /// the transition backwards.
    pub pairs: BTreeMap<String, String>,
}

impl Default for Transitions {
    fn default() -> Self {
        Transitions { default: "fade".into(), pairs: BTreeMap::new() }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Prefetch {
    /// Speculation Rules eagerness: `conservative`, `moderate`, or `eager`.
    pub eagerness: String,
    /// Prerender on pointer down in addition to prefetching.
    pub prerender: bool,
}

impl Default for Prefetch {
    fn default() -> Self {
        Prefetch { eagerness: "moderate".into(), prerender: true }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Budgets {
    /// Maximum gzipped size of a single HTML page, in kilobytes.
    pub page_kb: Option<f64>,
}

impl Config {
    pub fn load(root: &Path) -> Result<Config> {
        let path = root.join(CONFIG_FILE);
        if !path.exists() {
            return Ok(Config::default());
        }
        let src = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let config: Config =
            serde_json::from_str(&src).map_err(|e| anyhow::anyhow!("{}:{}:{}: {}", path.display(), e.line(), e.column(), e))?;
        config.validate()?;
        Ok(config)
    }

    /// Pairs as `(from, to, name)` triples.
    pub fn transition_pairs(&self) -> Result<Vec<(String, String, String)>> {
        self.transitions
            .pairs
            .iter()
            .map(|(key, name)| match key.split_once("->") {
                Some((from, to)) => Ok((from.trim().into(), to.trim().into(), name.clone())),
                None => bail!("transitions.pairs key {key:?} must look like \"/from -> /to\""),
            })
            .collect()
    }

    fn validate(&self) -> Result<()> {
        self.transition_pairs()?;
        if !matches!(self.prefetch.eagerness.as_str(), "conservative" | "moderate" | "eager") {
            bail!("prefetch.eagerness must be conservative, moderate, or eager");
        }
        crate::hosts::check_redirects(&self.redirects)?;
        for (name, schema) in &self.collections {
            schema.check_types(name)?;
        }
        for (key, value) in [("search", &self.agents.search), ("answers", &self.agents.answers), ("training", &self.agents.training)] {
            if !matches!(value.as_str(), "allow" | "disallow") {
                bail!("agents.{key} must be \"allow\" or \"disallow\"");
            }
        }
        for (agent, rule) in &self.agents.robots {
            if !matches!(rule.as_str(), "allow" | "disallow") || agent.contains(['\n', '\r']) {
                bail!("agents.robots.{agent} must be \"allow\" or \"disallow\"");
            }
        }
        for font in &self.fonts {
            let ok = |s: &str| !s.contains(['"', '\'', ';', '{', '}', '<', '\\', '\n']);
            if !font.src.starts_with('/') || ![&font.family, &font.src, &font.weight, &font.style].iter().all(|s| ok(s)) {
                bail!("fonts: {:?} needs a plain family name and a src starting with /", font.family);
            }
        }
        if self.site.image.as_ref().is_some_and(|i| !i.starts_with('/')) {
            bail!("site.image must be a path under public/ starting with /, like /og.png");
        }
        if self.site.twitter.as_ref().is_some_and(|h| !h.starts_with('@') || h.contains(char::is_whitespace)) {
            bail!("site.twitter must be a handle starting with @, like @mira");
        }
        if let Some(url) = &self.site.url
            && !(url.starts_with("https://") || url.starts_with("http://"))
        {
            bail!("site.url must be an absolute origin like https://example.com");
        }
        Ok(())
    }
}
