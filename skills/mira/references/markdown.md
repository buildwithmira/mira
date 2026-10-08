# Markdown

The Markdown Mira renders at build time, from tables to callouts and highlighted code.

Mira renders Markdown at build time, so none of it costs the reader any JavaScript. It follows CommonMark with the GitHub extensions.

## Supported syntax

- Tables, strikethrough, task lists, and footnotes
- Heading ids with `{#custom-id}` after a heading
- Smart punctuation: straight quotes become curly, `--` becomes an en dash, `---` an em dash
- Raw HTML, passed through as written

## Headings

Every heading gets an id from its text, so `## Shared elements` links as `#shared-elements`. Repeated headings get `-1`, `-2`, and so on. Second and third level headings are collected into the page's `toc`, which this site shows as **On this page**.

## Code

Fenced code blocks are highlighted at build time into classes the theme colors:

````markdown
```rust
fn main() {
    println!("Hello from Mira");
}
```
````

```rust
fn main() {
    println!("Hello from Mira");
}
```

The language label shows in the block's corner. Mira understands the common languages, including Rust, JavaScript, HTML, CSS, JSON, YAML, Markdown, Python, Go, and shell. A few names map to the closest grammar: `mira`, `svelte`, and `vue` highlight as HTML, and `ts`, `tsx`, and `jsx` as JavaScript. Unknown languages render as plain, escaped text.

## Callouts

Start a quote with a marker to make a callout:

```markdown
> [!NOTE]
> Useful information.
```

> [!NOTE]
> Useful information the reader should notice.

> [!TIP]
> A better way to do something.

> [!IMPORTANT]
> Something the reader needs to know.

> [!WARNING]
> Something that needs attention right away.

> [!CAUTION]
> The risks of an action.

## Footnotes

Mira writes footnotes[^1] with links back to the text.

[^1]: Like this one.

## Prose styles

Markdown pages are wrapped in `<article class="prose">`, which sets a readable measure, rhythm, and styles for lists, quotes, tables, code, and images. Add the class to any element in a component to get the same styles.
