//! Media frames: `<mira-frame>` for images and video.
//!
//! The frame is the decoration, and each part does a job: reserved space
//! (width and height from the file), an 8×8 mosaic of the image's real
//! colors as the loading state, corner handles only when the image opens
//! full size, a hairline border that doubles as the focus ring, and a mono
//! caption line for the real caption and credit.
//!
//! Images are written to `/media/<name>-<hash>.<ext>` with an AVIF beside
//! the original. Encodes are cached by content hash in `.mira/cache/media`,
//! so each image is encoded once.

use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::html::escape;

/// Widest image encoded to AVIF; larger sources are scaled down for it.
const AVIF_MAX_WIDTH: u32 = 2560;
const AVIF_QUALITY: f32 = 72.0;
const AVIF_SPEED: u8 = 6;
/// Crockford style base32, lowercase, for short readable hashes.
const ALPHABET: &[u8] = b"0123456789abcdefghjkmnpqrstvwxyz";

#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Image,
    Vector,
    Video,
}

/// One source file, processed once per build however many pages use it.
pub struct MediaFile {
    pub kind: Kind,
    pub hash: String,
    source: PathBuf,
    /// Output path of the original, e.g. `/media/pipeline-7kq2.png`.
    pub original: String,
    pub original_format: String,
    pub original_bytes: u64,
    /// Output path and bytes of the AVIF, when it is smaller.
    pub avif: Option<(String, Arc<Vec<u8>>)>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// An 8×8 PNG of the image's colors as a data URI.
    pub mosaic: Option<String>,
}

#[derive(Default, Serialize)]
struct Usage {
    alt: Option<String>,
    caption: Option<String>,
    credit: Option<String>,
    pages: Vec<String>,
}

type Slot = Arc<OnceLock<std::result::Result<Arc<MediaFile>, String>>>;

pub struct Pipeline {
    root: PathBuf,
    cache: PathBuf,
    files: Mutex<HashMap<PathBuf, Slot>>,
    usage: Mutex<BTreeMap<String, Usage>>,
}

impl Pipeline {
    pub fn new(root: &Path) -> Pipeline {
        Pipeline {
            root: root.to_path_buf(),
            cache: root.join(".mira").join("cache").join("media"),
            files: Mutex::new(HashMap::new()),
            usage: Mutex::new(BTreeMap::new()),
        }
    }

    fn process(&self, source: &Path) -> Result<Arc<MediaFile>> {
        let slot = {
            let mut files = self.files.lock().unwrap();
            files.entry(source.to_path_buf()).or_default().clone()
        };
        slot.get_or_init(|| self.load(source).map(Arc::new).map_err(|e| format!("{e:#}"))).clone().map_err(|e| anyhow!(e))
    }

    fn load(&self, source: &Path) -> Result<MediaFile> {
        let bytes = std::fs::read(source).with_context(|| format!("reading {}", source.display()))?;
        let digest = Sha256::digest(&bytes);
        let hash: String = digest.iter().take(4).map(|b| ALPHABET[(*b as usize) % 32] as char).collect();
        let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        let stem = crate::content::slugify(&source.file_stem().unwrap_or_default().to_string_lossy());
        let original = format!("/media/{stem}-{hash}.{ext}");
        let kind = match ext.as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" => Kind::Image,
            "svg" => Kind::Vector,
            "mp4" | "webm" | "mov" => Kind::Video,
            "avif" => Kind::Image,
            other => bail!("{} is a .{other} file; frames take png, jpg, gif, webp, avif, svg, mp4, webm, or mov", source.display()),
        };

        let mut file = MediaFile {
            kind,
            hash: format!("{stem}-{hash}"),
            source: source.to_path_buf(),
            original,
            original_format: ext.clone(),
            original_bytes: bytes.len() as u64,
            avif: None,
            width: None,
            height: None,
            mosaic: None,
        };

        match kind {
            Kind::Vector => {
                if let Ok(size) = imagesize::blob_size(&bytes) {
                    file.width = Some(size.width as u32);
                    file.height = Some(size.height as u32);
                }
            }
            Kind::Video => {}
            Kind::Image if ext == "avif" => {
                let size = imagesize::blob_size(&bytes).context("reading AVIF dimensions")?;
                file.width = Some(size.width as u32);
                file.height = Some(size.height as u32);
            }
            Kind::Image => {
                let image = image::load_from_memory(&bytes).with_context(|| format!("decoding {}", source.display()))?;
                file.width = Some(image.width());
                file.height = Some(image.height());
                file.mosaic = Some(mosaic(&image)?);
                let encoded = self.avif(&image, &digest)?;
                if (encoded.len() as u64) < file.original_bytes {
                    file.avif = Some((format!("/media/{stem}-{hash}.avif"), Arc::new(encoded)));
                }
            }
        }
        Ok(file)
    }

    /// Encodes to AVIF, or reads the cached encode of identical bytes.
    fn avif(&self, image: &image::DynamicImage, digest: &[u8]) -> Result<Vec<u8>> {
        let key: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        let cached = self.cache.join(format!("{key}-q{AVIF_QUALITY}-s{AVIF_SPEED}.avif"));
        if let Ok(bytes) = std::fs::read(&cached) {
            return Ok(bytes);
        }
        let image = if image.width() > AVIF_MAX_WIDTH {
            let height = (image.height() as u64 * AVIF_MAX_WIDTH as u64 / image.width() as u64) as u32;
            image.resize_exact(AVIF_MAX_WIDTH, height.max(1), image::imageops::FilterType::Lanczos3)
        } else {
            image.clone()
        };
        let rgba = image.to_rgba8();
        let (w, h) = rgba.dimensions();
        let pixels: &[rgb::RGBA8] = rgb::FromSlice::as_rgba(rgba.as_raw().as_slice());
        let encoded = ravif::Encoder::new()
            .with_quality(AVIF_QUALITY)
            .with_alpha_quality(AVIF_QUALITY + 8.0)
            .with_speed(AVIF_SPEED)
            .encode_rgba(ravif::Img::new(pixels, w as usize, h as usize))
            .map_err(|e| anyhow!("encoding AVIF: {e}"))?
            .avif_file;
        std::fs::create_dir_all(&self.cache)?;
        std::fs::write(&cached, &encoded)?;
        Ok(encoded)
    }

    /// Expands every `<mira-frame>` in `html`. Relative sources resolve
    /// against `base`, the directory of the file that contains them.
    /// Returns the HTML and the CSS the frames need (mosaics and ratios).
    pub fn expand(&self, html: &str, base: &Path, page_url: &str) -> Result<(String, String)> {
        if !html.contains("<mira-frame") {
            return Ok((html.to_string(), String::new()));
        }
        let mut out = String::with_capacity(html.len());
        let mut css = String::new();
        let mut rest = html;
        while let Some(i) = rest.find("<mira-frame") {
            out.push_str(&rest[..i]);
            let tag_rest = &rest[i..];
            let open_end = tag_rest.find('>').ok_or_else(|| anyhow!("<mira-frame is never closed"))?;
            let tag = tag_rest[..open_end].trim_end_matches('/');
            let mut after = &tag_rest[open_end + 1..];
            if let Some(close) = after.strip_prefix("</mira-frame>") {
                after = close;
            }
            let attrs = Attrs(tag);
            let src = attrs.get("src").ok_or_else(|| anyhow!("<mira-frame> needs a src"))?;
            let alt = attrs.get("alt");
            let caption = attrs.get("caption");
            let credit = attrs.get("credit");
            let zoom = attrs.has("zoom");

            let path = self.resolve(&src, base)?;
            let file = self.process(&path)?;
            if alt.is_none() && file.kind != Kind::Video {
                bail!(
                    "<mira-frame src=\"{src}\"> has no alt text\nhint: describe the image with alt=\"…\", or use alt=\"\" if it is decoration"
                );
            }

            {
                let mut usage = self.usage.lock().unwrap();
                let entry = usage.entry(file.original.clone()).or_default();
                entry.alt = entry.alt.take().or(alt.clone());
                entry.caption = entry.caption.take().or(caption.clone());
                entry.credit = entry.credit.take().or(credit.clone());
                if !entry.pages.iter().any(|p| p == page_url) {
                    entry.pages.push(page_url.to_string());
                }
            }

            let (width, height) = match (file.width, file.height) {
                (Some(w), Some(h)) => (w, h),
                _ => match (attrs.get("width").and_then(|w| w.parse().ok()), attrs.get("height").and_then(|h| h.parse().ok())) {
                    (Some(w), Some(h)) => (w, h),
                    _ => bail!(
                        "<mira-frame src=\"{src}\"> needs width and height\nhint: Mira cannot read a video's size; add width=\"1920\" height=\"1080\""
                    ),
                },
            };

            let id = &file.hash;
            css.push_str(&format!("[data-mira-frame=\"{id}\"] .mira-frame__media{{aspect-ratio:{width}/{height}}}"));
            let poster = match (&file.mosaic, attrs.get("poster")) {
                (Some(m), _) => Some(m.clone()),
                (None, Some(p)) => {
                    let poster = self.process(&self.resolve(&p, base)?)?;
                    poster.mosaic.clone()
                }
                _ => None,
            };
            if let Some(m) = &poster {
                css.push_str(&format!("[data-mira-frame=\"{id}\"] .mira-frame__media{{background-image:url({m})}}"));
            }

            out.push_str(&frame_html(&file, alt.as_deref().unwrap_or(""), caption.as_deref(), credit.as_deref(), zoom, width, height));
            rest = after;
        }
        out.push_str(rest);
        Ok((out, css))
    }

    /// `/x` is under `public/`, `@/x` under the project root, and anything
    /// else is relative to `base`. Sources must stay inside the project.
    fn resolve(&self, src: &str, base: &Path) -> Result<PathBuf> {
        if src.starts_with("http://") || src.starts_with("https://") || src.starts_with("//") {
            bail!(
                "<mira-frame src=\"{src}\"> points at another site\nhint: download the file into the project so Mira can size and encode it"
            );
        }
        let path = if let Some(rest) = src.strip_prefix("@/") {
            self.root.join(rest)
        } else if let Some(rest) = src.strip_prefix('/') {
            self.root.join("public").join(rest)
        } else {
            base.join(src)
        };
        let normal = normalize(&path);
        if !normal.starts_with(normalize(&self.root)) {
            bail!("<mira-frame src=\"{src}\"> points outside the project");
        }
        if !normal.is_file() {
            bail!("<mira-frame src=\"{src}\"> does not exist\nhint: looked for {}", normal.display());
        }
        Ok(normal)
    }

    pub fn uses_video(html: &str) -> bool {
        html.contains("mira-frame--video")
    }

    /// Output paths the build will write, for the link checker.
    pub fn outputs(&self) -> Vec<String> {
        let files = self.files.lock().unwrap();
        let mut paths = Vec::new();
        for slot in files.values() {
            if let Some(Ok(file)) = slot.get() {
                paths.push(file.original.clone());
                if let Some((avif, _)) = &file.avif {
                    paths.push(avif.clone());
                }
            }
        }
        paths
    }

    /// Writes every processed file under `out` and the `media.json` manifest.
    /// Returns the number of files.
    pub fn write(&self, out: &Path) -> Result<usize> {
        let files: Vec<Arc<MediaFile>> = {
            let files = self.files.lock().unwrap();
            files.values().filter_map(|s| s.get().and_then(|r| r.as_ref().ok()).cloned()).collect()
        };
        if files.is_empty() {
            return Ok(0);
        }
        let dir = out.join("media");
        std::fs::create_dir_all(&dir)?;
        let usage = self.usage.lock().unwrap();
        let mut manifest = Vec::new();
        for file in &files {
            std::fs::copy(&file.source, out.join(file.original.trim_start_matches('/')))
                .with_context(|| format!("copying {}", file.source.display()))?;
            if let Some((path, bytes)) = &file.avif {
                std::fs::write(out.join(path.trim_start_matches('/')), bytes.as_slice())?;
            }
            let used = usage.get(&file.original);
            let mut formats =
                vec![serde_json::json!({ "src": file.original, "format": file.original_format, "bytes": file.original_bytes })];
            if let Some((path, bytes)) = &file.avif {
                formats.insert(0, serde_json::json!({ "src": path, "format": "avif", "bytes": bytes.len() }));
            }
            manifest.push(serde_json::json!({
                "id": file.hash,
                "kind": file.kind,
                "width": file.width,
                "height": file.height,
                "formats": formats,
                "alt": used.and_then(|u| u.alt.clone()),
                "caption": used.and_then(|u| u.caption.clone()),
                "credit": used.and_then(|u| u.credit.clone()),
                "pages": used.map(|u| u.pages.clone()).unwrap_or_default(),
            }));
        }
        manifest.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        std::fs::write(out.join("media.json"), serde_json::to_string_pretty(&manifest)?)?;
        Ok(files.len())
    }
}

/// Makes relative frame sources in Markdown absolute to the project, as
/// `@/<dir>/<src>`, so they resolve against the Markdown file wherever the
/// rendered HTML ends up.
pub fn rebase(html: &str, dir: &Path) -> String {
    if !html.contains("<mira-frame") {
        return html.to_string();
    }
    let dir = dir.to_string_lossy().replace('\\', "/");
    let prefix = if dir.is_empty() { "@/".to_string() } else { format!("@/{dir}/") };
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find("<mira-frame") {
        out.push_str(&rest[..i]);
        let end = rest[i..].find('>').map_or(rest.len(), |e| i + e + 1);
        let mut tag = rest[i..end].to_string();
        for attr in [" src=\"", " poster=\""] {
            if let Some(at) = tag.find(attr) {
                let value = &tag[at + attr.len()..];
                let relative =
                    !(value.starts_with('/') || value.starts_with("@/") || value.starts_with("http:") || value.starts_with("https:"));
                if relative {
                    tag.insert_str(at + attr.len(), &prefix);
                }
            }
        }
        out.push_str(&tag);
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

fn frame_html(file: &MediaFile, alt: &str, caption: Option<&str>, credit: Option<&str>, zoom: bool, width: u32, height: u32) -> String {
    let id = &file.hash;
    let video = file.kind == Kind::Video;
    let handles = if zoom || video {
        "<i class=\"mira-frame__h\"></i><i class=\"mira-frame__h\"></i><i class=\"mira-frame__h\"></i><i class=\"mira-frame__h\"></i>"
    } else {
        ""
    };
    let media = if video {
        format!(
            "<video src=\"{}\" width=\"{width}\" height=\"{height}\" preload=\"metadata\" playsinline{}></video>\
<button class=\"mira-frame__play\" type=\"button\" aria-label=\"Play\">Play</button>\
<span class=\"mira-frame__progress\" aria-hidden=\"true\"></span>",
            escape(&file.original),
            if alt.is_empty() { String::new() } else { format!(" aria-label=\"{}\"", escape(alt)) }
        )
    } else {
        let img = format!(
            "<img src=\"{}\" alt=\"{}\" width=\"{width}\" height=\"{height}\" loading=\"lazy\" decoding=\"async\">",
            escape(&file.original),
            escape(alt)
        );
        match &file.avif {
            Some((avif, _)) => format!("<picture><source srcset=\"{}\" type=\"image/avif\">{img}</picture>", escape(avif)),
            None => img,
        }
    };
    let inner = if zoom && !video {
        let label = if alt.is_empty() { "Open the image full size".to_string() } else { format!("Open full size: {alt}") };
        format!("<a class=\"mira-frame__media\" href=\"{}\" aria-label=\"{}\">{media}{handles}</a>", escape(&file.original), escape(&label))
    } else {
        format!("<div class=\"mira-frame__media\">{media}{handles}</div>")
    };
    let cap = match (caption, credit) {
        (None, None) => String::new(),
        (c, r) => {
            let mut text = escape(c.unwrap_or(""));
            if let Some(r) = r {
                if !text.is_empty() {
                    text.push_str(" &middot; ");
                }
                text.push_str(&format!("<span class=\"mira-frame__credit\">{}</span>", escape(r)));
            }
            format!("<figcaption class=\"mira-frame__cap\">{text}</figcaption>")
        }
    };
    let class = if video {
        "mira-frame mira-frame--video"
    } else if zoom {
        "mira-frame mira-frame--zoom"
    } else {
        "mira-frame"
    };
    format!("<figure class=\"{class}\" data-mira-frame=\"{id}\">{inner}{cap}</figure>")
}

/// An 8×8 PNG of the image's average colors, as a data URI.
fn mosaic(image: &image::DynamicImage) -> Result<String> {
    let small = image.resize_exact(8, 8, image::imageops::FilterType::Triangle).to_rgb8();
    let mut png = Vec::new();
    image::DynamicImage::ImageRgb8(small).write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)?;
    Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)))
}

struct Attrs<'a>(&'a str);

impl Attrs<'_> {
    fn get(&self, name: &str) -> Option<String> {
        let needle = format!("{name}=\"");
        let mut search = self.0;
        loop {
            let i = search.find(&needle)?;
            let preceded = search[..i].ends_with(char::is_whitespace);
            let rest = &search[i + needle.len()..];
            if preceded {
                return rest.find('"').map(|end| crate::twin::decode_entities(&rest[..end]));
            }
            search = rest;
        }
    }

    fn has(&self, name: &str) -> bool {
        self.0.split(|c: char| c.is_whitespace() || c == '>').any(|w| w == name || w.starts_with(&format!("{name}=")))
    }
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mira-media-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("public")).unwrap();
        std::fs::create_dir_all(dir.join("content")).unwrap();
        let mut img = image::RgbImage::new(40, 20);
        for (x, _, p) in img.enumerate_pixels_mut() {
            *p = if x < 20 { image::Rgb([244, 86, 30]) } else { image::Rgb([10, 31, 250]) };
        }
        img.save(dir.join("content").join("Build Pipeline.png")).unwrap();
        dir
    }

    #[test]
    fn frames_an_image() {
        let root = project();
        let pipeline = Pipeline::new(&root);
        let html =
            r#"<p><mira-frame src="./Build Pipeline.png" alt="Build pipeline" caption="Cold build" credit="Mira" zoom></mira-frame></p>"#;
        let (out, css) = pipeline.expand(html, &root.join("content"), "/").unwrap();
        assert!(out.contains(r#"class="mira-frame mira-frame--zoom""#), "{out}");
        assert!(out.contains(r#"width="40" height="20""#), "{out}");
        assert!(out.contains("/media/build-pipeline-"), "{out}");
        assert!(
            out.contains(
                r#"<figcaption class="mira-frame__cap">Cold build &middot; <span class="mira-frame__credit">Mira</span></figcaption>"#
            ),
            "{out}"
        );
        assert_eq!(out.matches("mira-frame__h").count(), 4, "{out}");
        assert!(css.contains("aspect-ratio:40/20") && css.contains("data:image/png;base64,"), "{css}");

        let dist = root.join("dist");
        assert_eq!(pipeline.write(&dist).unwrap(), 1);
        let manifest = std::fs::read_to_string(dist.join("media.json")).unwrap();
        assert!(manifest.contains("\"alt\": \"Build pipeline\""), "{manifest}");
        // A second pipeline reads the AVIF from the cache.
        let again = Pipeline::new(&root);
        again.expand(html, &root.join("content"), "/").unwrap();
        assert!(root.join(".mira/cache/media").read_dir().unwrap().count() >= 1);
    }

    #[test]
    fn rebases_relative_sources() {
        let html = rebase(
            r#"<mira-frame src="./a.mp4" poster="b.png" alt=""></mira-frame><mira-frame src="/c.png" alt="">"#,
            Path::new("content/posts"),
        );
        assert_eq!(
            html,
            r#"<mira-frame src="@/content/posts/./a.mp4" poster="@/content/posts/b.png" alt=""></mira-frame><mira-frame src="/c.png" alt="">"#
        );
    }

    #[test]
    fn requires_alt_text() {
        let root = project();
        let err = Pipeline::new(&root)
            .expand(r#"<mira-frame src="@/content/Build Pipeline.png"></mira-frame>"#, &root, "/")
            .unwrap_err()
            .to_string();
        assert!(err.contains("has no alt text"), "{err}");
    }

    #[test]
    fn rejects_paths_outside_the_project() {
        let root = project();
        let err = Pipeline::new(&root)
            .expand(r#"<mira-frame src="../../secret.png" alt=""></mira-frame>"#, &root.join("content"), "/")
            .unwrap_err()
            .to_string();
        assert!(err.contains("outside the project"), "{err}");
    }
}
