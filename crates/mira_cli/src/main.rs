mod dev;
mod diagnostic;
mod mcp;
mod ui;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};
use mira_compiler::{BuildOptions, BuildReport, Config, build, scaffold::scaffold};

/// Mira builds content sites to static HTML.
#[derive(Parser)]
#[command(name = "mira", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new Mira site from the starter template.
    New {
        /// Directory to create.
        dir: PathBuf,
        /// Skip the next steps hint, for tools that print their own.
        #[arg(long, hide = true)]
        no_hints: bool,
    },
    /// Build the site to static HTML.
    Build {
        /// Project root.
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// Output directory, relative to the project root.
        #[arg(long, default_value = "dist")]
        out: PathBuf,
        /// Print a machine readable report instead of the summary.
        #[arg(long)]
        json: bool,
        /// Show how long each build step took.
        #[arg(long)]
        timings: bool,
    },
    /// Serve the site locally, rebuilding and reloading on change.
    Dev {
        /// Project root.
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long, default_value_t = 4321)]
        port: u16,
    },
    /// Move an existing site into a new Mira project.
    ///
    /// Supports Next.js, Astro, Hugo, Jekyll, Docusaurus, Gatsby, Eleventy,
    /// VitePress, and plain Markdown folders. The old project is read as
    /// text: none of its code runs, and it is never changed.
    Migrate {
        /// The old project.
        source: PathBuf,
        /// A new, empty directory for the Mira project.
        dest: PathBuf,
        /// The old framework: nextjs, astro, hugo, jekyll, docusaurus,
        /// gatsby, eleventy, vitepress, or markdown. Detected when left out.
        #[arg(long)]
        from: Option<String>,
        /// Show what would move without writing anything.
        #[arg(long)]
        dry_run: bool,
        /// Print a machine readable report instead of the summary.
        #[arg(long)]
        json: bool,
    },
    /// Serve the site's content over MCP on standard input and output.
    Mcp {
        /// Project root.
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let (root, json) = match &cli.command {
        Command::New { dir, .. } => (dir.clone(), false),
        Command::Build { root, json, .. } => (root.clone(), *json),
        Command::Migrate { dest, json, .. } => (dest.clone(), *json),
        Command::Dev { root, .. } | Command::Mcp { root } => (root.clone(), false),
    };
    let result = match cli.command {
        Command::New { dir, no_hints } => new(&dir, no_hints),
        Command::Build { root, out, json, timings } => run_build(&root, &out, json, timings),
        Command::Migrate { source, dest, from, dry_run, json } => run_migrate(source, dest, from.as_deref(), dry_run, json),
        Command::Dev { root, port } => dev::run(&root, port),
        Command::Mcp { root } => mcp::run(&root),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) if json => {
            println!("{}", json_error(&err, &root));
            ExitCode::FAILURE
        }
        Err(err) => {
            ui::error(&err, &root);
            ExitCode::FAILURE
        }
    }
}

fn new(dir: &Path, no_hints: bool) -> Result<()> {
    ui::header("new", &dir.display().to_string());
    let files = scaffold(dir)?;
    for file in files {
        eprintln!("    {} {}", ui::dim(ui::DOT), file);
    }
    eprintln!();
    ui::success(&format!("created {}", dir.display()));
    eprintln!();
    if no_hints {
        return Ok(());
    }
    eprintln!("    {}", ui::dim("next"));
    eprintln!("    cd {}", dir.display());
    eprintln!("    mira dev");
    eprintln!();
    Ok(())
}

fn run_build(root: &Path, out: &Path, json: bool, timings: bool) -> Result<()> {
    if !json {
        ui::header("build", &root.display().to_string());
    }
    let report = build(&BuildOptions { root: root.to_path_buf(), out: root.join(out), dev: false })?;
    if json {
        println!("{}", serde_json::json!({ "ok": true, "schema": 1, "report": report }));
    } else {
        if timings {
            print_timings(&report);
        }
        print_summary(&report, Config::load(root)?.budgets.page_kb, out);
    }
    Ok(())
}

fn run_migrate(source: PathBuf, dest: PathBuf, from: Option<&str>, dry_run: bool, json: bool) -> Result<()> {
    use mira_compiler::migrate::{Framework, MigrateOptions, migrate};
    let from = match from {
        Some(name) => Some(Framework::parse(name).ok_or_else(|| {
            anyhow::anyhow!(
                "--from {name}: unknown framework
hint: use nextjs, astro, hugo, jekyll, docusaurus, gatsby, eleventy, vitepress, or markdown"
            )
        })?),
        None => None,
    };
    if !json {
        ui::header("migrate", &source.display().to_string());
    }
    let report = migrate(&MigrateOptions { source: source.clone(), dest: dest.clone(), from, dry_run })?;
    // A migration is only done when the new project builds.
    let built = if dry_run { None } else { Some(build(&BuildOptions { root: dest.clone(), out: dest.join("dist"), dev: false })) };
    if json {
        let build = built.as_ref().map(|b| match b {
            Ok(r) => serde_json::json!({ "ok": true, "pages": r.pages.len(), "warnings": r.warnings }),
            Err(e) => serde_json::json!({ "ok": false, "error": diagnostic::Diagnostic::from_error(e, &dest) }),
        });
        println!("{}", serde_json::json!({ "ok": true, "schema": 1, "dry_run": dry_run, "report": report, "build": build }));
        return Ok(());
    }
    let fw = report.framework.map_or("unknown", Framework::name);
    ui::section(&format!("from {fw}"));
    eprintln!("    {}  {}", ui::leader("pages", 16), report.pages);
    for (c, n) in &report.entries {
        eprintln!("    {}  {n}", ui::leader(&format!("{c} entries"), 16));
    }
    eprintln!("    {}  {}", ui::leader("static files", 16), report.assets);
    eprintln!("    {}  {}", ui::leader("redirects", 16), report.redirects.len());
    eprintln!("    {}  {}", ui::leader("to review", 16), report.todo.len());
    eprintln!();
    for item in report.todo.iter().take(8) {
        ui::warning(item);
    }
    if report.todo.len() > 8 {
        eprintln!("    {}", ui::dim(&format!("and {} more in MIGRATION.md", report.todo.len() - 8)));
    }
    if !report.todo.is_empty() {
        eprintln!();
    }
    match built {
        None => ui::success("dry run: nothing was written"),
        Some(Ok(r)) => {
            ui::success(&format!("migrated to {}, and it builds: {} pages", dest.display(), r.pages.len()));
            eprintln!();
            eprintln!("    {}", ui::dim("next"));
            eprintln!("    read {}", dest.join("MIGRATION.md").display());
            eprintln!("    cd {}", dest.display());
            eprintln!("    mira dev");
        }
        Some(Err(e)) => {
            ui::warning(&format!("migrated to {}, but the build failed; fix this, then run `mira build`", dest.display()));
            eprintln!();
            ui::error(&e, &dest);
        }
    }
    eprintln!();
    Ok(())
}

fn print_timings(report: &BuildReport) {
    let total: f64 = report.timings.iter().map(|t| t.ms).sum::<f64>().max(0.001);
    let slowest = report.timings.iter().max_by(|a, b| a.ms.total_cmp(&b.ms)).map(|t| t.step);
    ui::section("timings");
    for t in &report.timings {
        let note = if Some(t.step) == slowest { ui::dim("  slowest") } else { String::new() };
        eprintln!("    {}  {}  {:>8}{note}", ui::leader(t.step, 12), ui::bar(t.ms / total, 16), ui::ms(t.ms));
    }
    eprintln!();
}

/// Pages are drawn against the page budget, or against 14 KB (one TCP
/// round trip of initial congestion window) when no budget is set.
fn print_summary(report: &BuildReport, budget_kb: Option<f64>, out: &Path) {
    let budget = budget_kb.unwrap_or(14.0) * 1024.0;
    let width = report.pages.iter().map(|p| p.url.chars().count()).max().unwrap_or(0).max(16) + 4;

    ui::section("pages");
    for page in &report.pages {
        let morphs = match page.morphs {
            0 => String::new(),
            1 => ui::dim("  1 morph"),
            n => ui::dim(&format!("  {n} morphs")),
        };
        eprintln!(
            "    {}  {}  {:>8}{}",
            ui::leader(&page.url, width),
            ui::bar(page.gzip_bytes as f64 / budget, 10),
            ui::kb(page.gzip_bytes),
            morphs
        );
    }
    eprintln!(
        "    {}",
        ui::dim(&format!(
            "gzipped, bars against the {:.0} KB {}",
            budget / 1024.0,
            if budget_kb.is_some() { "page budget" } else { "first round trip" }
        ))
    );
    eprintln!();

    if !report.collections.is_empty() {
        let list: Vec<String> = report.collections.iter().map(|(name, n)| format!("{name} {}", ui::dim(&n.to_string()))).collect();
        eprintln!("  {}  {}", ui::bold("collections"), list.join(ui::dim("  ·  ").as_str()));
    }
    eprintln!(
        "  {}      {}  {}  {}",
        ui::bold("runtime"),
        ui::kb(report.runtime_gzip_bytes),
        ui::bar(report.runtime_gzip_bytes as f64 / 2048.0, 10),
        ui::dim("of 2 KB")
    );
    if !report.outputs.is_empty() {
        eprintln!("  {}      {}", ui::bold("agents"), report.outputs.join(&ui::dim("  ·  ")));
    }
    eprintln!();
    if !report.warnings.is_empty() {
        for warning in &report.warnings {
            ui::warning(warning);
        }
        eprintln!();
    }
    let pages = report.pages.len();
    ui::success(&format!(
        "built {pages} page{} in {}  {}",
        if pages == 1 { "" } else { "s" },
        ui::ms(report.duration_ms),
        ui::dim(&format!("→ {}", out.display()))
    ));
    eprintln!();
}

fn json_error(err: &anyhow::Error, root: &Path) -> serde_json::Value {
    serde_json::json!({ "ok": false, "schema": 1, "error": diagnostic::Diagnostic::from_error(err, root) })
}
