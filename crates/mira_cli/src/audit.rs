//! `mira audit --agent`: what each page costs an agent to read.
//!
//! Builds the site into `.mira/audit` and reports, per page, the tokens in
//! its Markdown twin, in its search index entry, and in its largest
//! section, flagging pages over a budget. Tokens are approximate, at four
//! characters each.

use std::path::Path;

use anyhow::{Result, bail};
use mira_compiler::{BuildOptions, build};
use serde_json::{Value, json};

use crate::ui;

/// Per page figures, in approximate tokens.
struct Row {
    url: String,
    markdown: usize,
    /// The page has no Markdown copy, so agents cannot read it.
    missing: bool,
    index: usize,
    sections: usize,
    largest: usize,
}

fn tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

pub fn run(root: &Path, budget: usize, json_out: bool) -> Result<()> {
    if !json_out {
        ui::header("audit", &root.display().to_string());
    }
    if !mira_compiler::Config::load(root)?.agents.twins {
        bail!(
            "mira.config.json: agents.twins is false, so pages have no Markdown copies and agents cannot read them over MCP\nhint: remove \"twins\": false from agents to audit what agents read"
        );
    }
    let out = std::path::absolute(root)?.join(".mira").join("audit");
    build(&BuildOptions { root: root.to_path_buf(), out: out.clone(), dev: false, host_config: false })?;
    let read = |path: &str| std::fs::read_to_string(out.join(path)).ok();

    let index: Vec<Value> = read("_mira/search.json").and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    if index.is_empty() {
        bail!("the build wrote no search index, so there is nothing to audit");
    }
    let mut rows: Vec<Row> = index
        .iter()
        .map(|doc| {
            let url = doc["url"].as_str().unwrap_or("/").to_string();
            let twin = match url.trim_end_matches('/') {
                "" => "index.md".to_string(),
                path => format!("{}.md", path.trim_start_matches('/')),
            };
            let markdown = read(&twin);
            let (sections, largest) = sections(markdown.as_deref().unwrap_or(""));
            Row {
                missing: markdown.is_none(),
                markdown: tokens(markdown.as_deref().unwrap_or("")),
                index: tokens(&doc.to_string()),
                sections,
                largest,
                url,
            }
        })
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.markdown));

    let over: Vec<&Row> = rows.iter().filter(|r| r.markdown > budget).collect();
    let schemas = tokens(&crate::mcp::tools().to_string());
    let llms = tokens(&read("llms.txt").unwrap_or_default());
    let full = tokens(&read("llms-full.txt").unwrap_or_default());
    let missing: Vec<&Row> = rows.iter().filter(|r| r.missing).collect();

    if json_out {
        let pages: Vec<Value> = rows
            .iter()
            .map(|r| {
                json!({
                    "url": r.url, "markdown_tokens": (!r.missing).then_some(r.markdown), "index_tokens": r.index,
                    "sections": r.sections, "largest_section_tokens": r.largest, "over_budget": r.markdown > budget,
                })
            })
            .collect();
        let report = json!({
            "budget": budget, "pages": pages, "over_budget": over.len(), "without_markdown": missing.len(),
            "llms_txt_tokens": llms, "llms_full_tokens": full, "mcp_tool_tokens": schemas,
        });
        println!("{}", json!({ "ok": true, "schema": 1, "report": report }));
        return Ok(());
    }

    let width = rows.iter().map(|r| r.url.chars().count()).max().unwrap_or(0).max(16) + 4;
    ui::section("pages");
    eprintln!("    {}  {:>8}  {:>7}  {:>8}", ui::dim(&format!("{:width$}", "")), ui::dim("markdown"), ui::dim("index"), ui::dim("largest"));
    for r in &rows {
        let note = if r.markdown > budget { ui::dim("  over budget") } else { String::new() };
        eprintln!(
            "    {}  {}  {:>6}  {:>7}  {:>8}{note}",
            ui::leader(&r.url, width),
            ui::bar(r.markdown as f64 / budget as f64, 10),
            r.markdown,
            r.index,
            r.largest
        );
    }
    eprintln!("    {}", ui::dim(&format!("approximate tokens, bars against the {budget} token budget")));
    eprintln!();
    eprintln!("  {}  {llms} tokens  {}", ui::bold("llms.txt"), ui::dim(&format!("llms-full.txt {full}")));
    eprintln!("  {}  {schemas} tokens  {}", ui::bold("mcp tools"), ui::dim("sent once per session"));
    eprintln!();
    for r in &missing {
        ui::warning(&format!("{} has no Markdown copy, so agents cannot read it over MCP", r.url));
    }
    for r in &over {
        let advice = if r.sections > 1 && r.largest <= budget {
            format!("agents can read it a section at a time; its largest section is {} tokens", r.largest)
        } else if r.sections > 1 {
            format!("its largest section is {} tokens; split that section with more headings", r.largest)
        } else {
            "it has no sections; add ## headings so agents can read part of it".to_string()
        };
        ui::warning(&format!("{} is {} tokens, over the {budget} token budget: {advice}", r.url, r.markdown));
    }
    if over.is_empty() && missing.is_empty() {
        ui::success(&format!("all {} pages are within {budget} tokens", rows.len()));
    }
    eprintln!();
    Ok(())
}

/// The number of `##` and deeper sections in a Markdown page, and the
/// tokens in the largest one.
fn sections(markdown: &str) -> (usize, usize) {
    let mut sizes = Vec::new();
    let mut current = 0;
    let mut fenced = false;
    let mut count = 0;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        if !fenced && line.starts_with("##") {
            sizes.push(current);
            current = 0;
            count += 1;
        }
        current += line.chars().count() + 1;
    }
    sizes.push(current);
    (count, sizes.into_iter().max().unwrap_or(0).div_ceil(4))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_sections() {
        let (count, largest) = sections("# A\n\nshort\n\n## B\n\n```\n## code\n```\n\n## C\n\nxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\n");
        assert_eq!(count, 2);
        assert_eq!(largest, 12);
    }
}
