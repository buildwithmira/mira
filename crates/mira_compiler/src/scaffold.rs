use std::path::Path;

use anyhow::{Context, Result, bail};

const STARTER: &[(&str, &str)] = &[
    ("mira.config.json", include_str!("../starter/mira.config.json")),
    ("AGENTS.md", include_str!("../starter/AGENTS.md")),
    (".gitignore", include_str!("../starter/gitignore")),
    ("layouts/default.mira", include_str!("../starter/layouts/default.mira")),
    ("routes/index.mira", include_str!("../starter/routes/index.mira")),
    ("routes/about.md", include_str!("../starter/routes/about.md")),
    ("routes/posts/index.mira", include_str!("../starter/routes/posts/index.mira")),
    ("routes/posts/[slug].mira", include_str!("../starter/routes/posts/[slug].mira")),
    ("content/posts/motion-is-navigation.md", include_str!("../starter/content/posts/motion-is-navigation.md")),
    ("content/posts/ship-less.md", include_str!("../starter/content/posts/ship-less.md")),
    ("public/favicon.svg", include_str!("../starter/public/favicon.svg")),
];

/// Writes the starter project into `dir` and returns the created paths.
pub fn scaffold(dir: &Path) -> Result<Vec<&'static str>> {
    if dir.exists() && std::fs::read_dir(dir)?.next().is_some() {
        bail!("{}: directory is not empty\nhint: pick a new directory name, e.g. `mira new my-site`", dir.display());
    }
    for (path, contents) in STARTER {
        let target = dir.join(path);
        std::fs::create_dir_all(target.parent().unwrap())?;
        std::fs::write(&target, contents).with_context(|| format!("writing {}", target.display()))?;
    }
    Ok(STARTER.iter().map(|(p, _)| *p).collect())
}
