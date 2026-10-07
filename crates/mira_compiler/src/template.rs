//! `.mira` single file components.
//!
//! A component is optional frontmatter, a `<template>` block, and an
//! optional `<style>` block. Templates support:
//!
//! - `{{ page.title }}` interpolation, HTML escaped
//! - `{{ unsafe entry.html }}` raw output, which must be marked explicitly
//! - `{#each collections.posts as post}` ... `{/each}`
//! - `{#if page.date}` ... `{:else}` ... `{/if}`, and `{#if !path}`
//! - `<slot />` for the content of the page being wrapped

use std::path::Path;

use anyhow::{Result, bail};
use serde_json::{Map, Value};

use crate::content::parse_document;
use crate::html::escape;

pub struct Component {
    pub data: Map<String, Value>,
    pub template: Template,
    pub style: Option<String>,
}

pub fn parse_component(src: &str, path: &Path) -> Result<Component> {
    let doc = parse_document(src, path)?;
    let (template_src, template_line) = match block(&doc.body, "template") {
        Some((inner, offset)) => (inner, doc.body_line + line_of(&doc.body, offset) - 1),
        None => (doc.body.as_str(), doc.body_line),
    };
    let style = block(&doc.body, "style").map(|(inner, _)| inner.trim().to_string());
    let template = Template::parse(template_src, path, template_line)?;
    Ok(Component { data: doc.data, template, style })
}

/// Contents of the outermost `<tag>...</tag>` block and the byte offset of
/// its contents.
fn block<'a>(src: &'a str, tag: &str) -> Option<(&'a str, usize)> {
    let open = format!("<{tag}>");
    let start = src.find(&open)? + open.len();
    let end = src.rfind(&format!("</{tag}>"))?;
    (end >= start).then(|| (&src[start..end], start))
}

fn line_of(src: &str, offset: usize) -> usize {
    src[..offset].matches('\n').count() + 1
}

#[derive(Debug)]
pub struct Template {
    nodes: Vec<Node>,
}

#[derive(Debug)]
enum Node {
    Text(String),
    Expr { path: Vec<String>, raw: bool },
    Each { path: Vec<String>, var: String, body: Vec<Node> },
    If { path: Vec<String>, negate: bool, then: Vec<Node>, otherwise: Vec<Node> },
    Slot,
}

/// A block being parsed: its opening tag, the nodes so far, the nodes of
/// an `{:else}` branch, and the line it opened on.
type Open = (Token, Vec<Node>, Option<Vec<Node>>, usize);

enum Token {
    Text(String),
    Expr { path: Vec<String>, raw: bool },
    Each { path: Vec<String>, var: String },
    If { path: Vec<String>, negate: bool },
    Else,
    EndEach,
    EndIf,
    Slot,
}

impl Template {
    pub fn parse(src: &str, path: &Path, first_line: usize) -> Result<Template> {
        let tokens = tokenize(src, path, first_line)?;
        let mut stack: Vec<Open> = Vec::new();
        let mut nodes = Vec::new();
        for (token, line) in tokens {
            let loc = || format!("{}:{}", path.display(), line);
            match token {
                Token::Text(t) => nodes.push(Node::Text(t)),
                Token::Expr { path, raw } => nodes.push(Node::Expr { path, raw }),
                Token::Slot => nodes.push(Node::Slot),
                open @ (Token::Each { .. } | Token::If { .. }) => {
                    stack.push((open, std::mem::take(&mut nodes), None, line));
                }
                Token::Else => match stack.last_mut() {
                    Some((Token::If { .. }, _, then @ None, _)) => {
                        *then = Some(std::mem::take(&mut nodes));
                    }
                    _ => bail!("{}: {{:else}} without a matching {{#if}}", loc()),
                },
                Token::EndEach => match stack.pop() {
                    Some((Token::Each { path, var }, parent, _, _)) => {
                        let body = std::mem::replace(&mut nodes, parent);
                        nodes.push(Node::Each { path, var, body });
                    }
                    _ => bail!("{}: {{/each}} without a matching {{#each}}", loc()),
                },
                Token::EndIf => match stack.pop() {
                    Some((Token::If { path, negate }, parent, then, _)) => {
                        let last = std::mem::replace(&mut nodes, parent);
                        let (then, otherwise) = match then {
                            Some(then) => (then, last),
                            None => (last, Vec::new()),
                        };
                        nodes.push(Node::If { path, negate, then, otherwise });
                    }
                    _ => bail!("{}: {{/if}} without a matching {{#if}}", loc()),
                },
            }
        }
        if let Some((open, _, _, line)) = stack.pop() {
            let (tag, close) = if matches!(open, Token::Each { .. }) { ("{#each}", "{/each}") } else { ("{#if}", "{/if}") };
            bail!(
                "{}:{}: {} is never closed
hint: add {} where the block should end",
                path.display(),
                line,
                tag,
                close
            );
        }
        Ok(Template { nodes })
    }

    /// Renders against `ctx`, an object of top-level names. `slot` replaces
    /// every `<slot />`.
    pub fn render(&self, ctx: &Value, slot: &str) -> String {
        let mut out = String::new();
        let mut scope = Scope { root: ctx, locals: Vec::new() };
        render_nodes(&self.nodes, &mut scope, slot, &mut out);
        out
    }
}

fn tokenize(src: &str, path: &Path, first_line: usize) -> Result<Vec<(Token, usize)>> {
    let bytes = src.as_bytes();
    let mut tokens = Vec::new();
    let (mut i, mut text_start, mut line) = (0, 0, first_line);
    let flush = |tokens: &mut Vec<(Token, usize)>, from: usize, to: usize, line: usize| {
        if to > from {
            tokens.push((Token::Text(src[from..to].to_string()), line));
        }
    };
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\n' {
            line += 1;
        }
        // `{#raw}...{/raw}` passes its contents through untouched.
        if b == b'{' && src[i..].starts_with("{#raw}") {
            let start = i + "{#raw}".len();
            let Some(len) = src[start..].find("{/raw}") else {
                bail!("{}:{}: {{#raw}} is never closed\nhint: add {{/raw}} where the literal text ends", path.display(), line);
            };
            flush(&mut tokens, text_start, i, line);
            tokens.push((Token::Text(src[start..start + len].to_string()), line));
            line += src[start..start + len].matches('\n').count();
            i = start + len + "{/raw}".len();
            text_start = i;
            continue;
        }
        let is_tag = b == b'{' && matches!(bytes.get(i + 1), Some(b'{' | b'#' | b':' | b'/'));
        if is_tag {
            let close = if bytes[i + 1] == b'{' { "}}" } else { "}" };
            let Some(len) = src[i + 2..].find(close) else {
                bail!("{}:{}: tag is never closed with {close}", path.display(), line);
            };
            let inner = src[i + 2..i + 2 + len].trim();
            let token = parse_tag(bytes[i + 1], inner).map_err(|e| anyhow::anyhow!("{}:{}: {e}", path.display(), line))?;
            flush(&mut tokens, text_start, i, line);
            tokens.push((token, line));
            line += src[i..i + 2 + len].matches('\n').count();
            i += 2 + len + close.len();
            text_start = i;
            continue;
        }
        if b == b'<' && src[i..].starts_with("<slot") {
            let Some(len) = src[i..].find('>') else {
                bail!("{}:{}: <slot is never closed", path.display(), line);
            };
            flush(&mut tokens, text_start, i, line);
            tokens.push((Token::Slot, line));
            i += len + 1;
            if src[i..].starts_with("</slot>") {
                i += "</slot>".len();
            }
            text_start = i;
            continue;
        }
        i += 1;
    }
    flush(&mut tokens, text_start, bytes.len(), line);
    Ok(tokens)
}

fn parse_tag(kind: u8, inner: &str) -> Result<Token> {
    Ok(match kind {
        b'{' => match inner.strip_prefix("unsafe ") {
            Some(rest) => Token::Expr { path: parse_path(rest)?, raw: true },
            None => Token::Expr { path: parse_path(inner)?, raw: false },
        },
        b'#' => {
            if let Some(rest) = inner.strip_prefix("each ") {
                let Some((list, var)) = rest.split_once(" as ") else {
                    bail!("expected {{#each list as item}}");
                };
                let var = var.trim();
                if !is_ident(var) {
                    bail!("{var:?} is not a valid loop variable name");
                }
                Token::Each { path: parse_path(list)?, var: var.into() }
            } else if let Some(rest) = inner.strip_prefix("if ") {
                let rest = rest.trim();
                match rest.strip_prefix('!') {
                    Some(p) => Token::If { path: parse_path(p)?, negate: true },
                    None => Token::If { path: parse_path(rest)?, negate: false },
                }
            } else {
                bail!("unknown block {{#{inner}}}, expected #each or #if");
            }
        }
        b':' if inner == "else" => Token::Else,
        b'/' if inner == "each" => Token::EndEach,
        b'/' if inner == "if" => Token::EndIf,
        _ => bail!("unknown tag {inner:?}"),
    })
}

fn parse_path(src: &str) -> Result<Vec<String>> {
    let src = src.trim();
    let path: Vec<String> = src.split('.').map(str::to_string).collect();
    if path.iter().any(|p| !is_ident(p)) {
        bail!("{src:?} is not a valid path, expected names joined by dots like page.title");
    }
    Ok(path)
}

fn is_ident(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

struct Scope<'a> {
    root: &'a Value,
    locals: Vec<(String, Value)>,
}

impl Scope<'_> {
    fn lookup(&self, path: &[String]) -> Option<Value> {
        let (head, rest) = path.split_first()?;
        let mut value = match self.locals.iter().rev().find(|(name, _)| name == head) {
            Some((_, v)) => v,
            None => self.root.get(head)?,
        };
        for (i, key) in rest.iter().enumerate() {
            value = match value {
                Value::Array(items) if key == "length" && i == rest.len() - 1 => {
                    return Some(Value::from(items.len()));
                }
                Value::Array(items) => items.get(key.parse::<usize>().ok()?)?,
                Value::Object(map) => map.get(key)?,
                _ => return None,
            };
        }
        Some(value.clone())
    }
}

fn truthy(value: &Option<Value>) -> bool {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        _ => true,
    }
}

fn render_nodes(nodes: &[Node], scope: &mut Scope, slot: &str, out: &mut String) {
    for node in nodes {
        match node {
            Node::Text(t) => out.push_str(t),
            Node::Slot => out.push_str(slot),
            Node::Expr { path, raw } => {
                let text = match scope.lookup(path) {
                    None | Some(Value::Null) => String::new(),
                    Some(Value::String(s)) => s,
                    Some(other) => other.to_string(),
                };
                if *raw { out.push_str(&text) } else { out.push_str(&escape(&text)) }
            }
            Node::If { path, negate, then, otherwise } => {
                let branch = if truthy(&scope.lookup(path)) != *negate { then } else { otherwise };
                render_nodes(branch, scope, slot, out);
            }
            Node::Each { path, var, body } => {
                let Some(Value::Array(items)) = scope.lookup(path) else { continue };
                for item in items {
                    scope.locals.push((var.clone(), item));
                    render_nodes(body, scope, slot, out);
                    scope.locals.pop();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn render(src: &str, ctx: Value) -> String {
        Template::parse(src, Path::new("t.mira"), 1).unwrap().render(&ctx, "SLOT")
    }

    #[test]
    fn escapes_by_default() {
        let ctx = json!({"page": {"title": "<b>&</b>"}});
        assert_eq!(render("{{ page.title }}", ctx.clone()), "&lt;b&gt;&amp;&lt;/b&gt;");
        assert_eq!(render("{{ unsafe page.title }}", ctx), "<b>&</b>");
    }

    #[test]
    fn loops_and_conditionals() {
        let ctx = json!({"posts": [{"t": "a"}, {"t": "b", "new": true}]});
        let src = "{#each posts as p}[{{ p.t }}{#if p.new}*{:else}-{/if}]{/each}{{ posts.length }}";
        assert_eq!(render(src, ctx), "[a-][b*]2");
    }

    #[test]
    fn slot_and_negation() {
        assert_eq!(render("<main><slot /></main>{#if !x}none{/if}", json!({})), "<main>SLOT</main>none");
    }

    #[test]
    fn raw_blocks_pass_through() {
        assert_eq!(render("{#raw}{{ page.title }} {#each x as y}{/raw}!", json!({})), "{{ page.title }} {#each x as y}!");
    }

    #[test]
    fn unclosed_block_reports_line() {
        let err = Template::parse("a\n{#each xs as x}\nb", Path::new("t.mira"), 1).unwrap_err();
        assert!(err.to_string().starts_with(
            "t.mira:2: {#each} is never closed
hint: add {/each}"
        ));
    }

    #[test]
    fn component_blocks() {
        let src = "---\nlayout: post\n---\n<template>\n<h1>{{ page.title }}</h1>\n</template>\n<style>\nh1 { color: red }\n</style>\n";
        let c = parse_component(src, Path::new("c.mira")).unwrap();
        assert_eq!(c.data["layout"], "post");
        assert_eq!(c.style.as_deref(), Some("h1 { color: red }"));
        assert_eq!(c.template.render(&json!({"page": {"title": "Hi"}}), "").trim(), "<h1>Hi</h1>");
    }
}
