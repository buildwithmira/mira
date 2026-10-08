# Templates

The .mira component format and its template syntax.

A `.mira` file is a single file component: optional frontmatter, a `<template>` block, and an optional `<style>` block. It compiles to HTML and CSS with no runtime.

```mira
---
title: Writing
---
<template>
<h1>{{ page.title }}</h1>
<ol>
  {#each collections.posts as post}
  <li><a href="{{ post.url }}">{{ post.title }}</a></li>
  {/each}
</ol>
</template>

<style>
ol { list-style: none; padding: 0; }
</style>
```

If a file has no `<template>` block, the whole body is the template.

## Output

Write a value with double braces. Output is HTML escaped, so `<`, `>`, `&`, and quotes are always safe:

```mira
<h1>{{ page.title }}</h1>
<a href="{{ post.url }}">Read</a>
```

To write raw HTML, mark it `unsafe`. Every use is reported as a warning at build time so it stays a deliberate choice:

```mira
{{ unsafe page.trusted_html }}
```

Values are read by dotted paths. Arrays take numeric indexes and a `length`:

```mira
{{ collections.posts.0.title }}
{{ collections.posts.length }} posts
```

A missing path renders nothing. Strings print as is, numbers and booleans print as text, and lists or objects print as JSON.

## Conditions

```mira
{#if post.description}
  <p>{{ post.description }}</p>
{:else}
  <p>No summary.</p>
{/if}
```

`{#if !path}` negates. A value is false when it is missing, `null`, `false`, `0`, an empty string, or an empty list.

## Loops

```mira
{#each collections.posts as post}
  <h2>{{ post.title }}</h2>
{/each}
```

Loop variables shadow outer names inside the loop.

## Slots

`<slot />` marks where wrapped content goes. In a layout it is the page; in a dynamic route it is the entry's rendered Markdown.

## Literal braces

Wrap text in `{#raw}` and `{/raw}` to output it exactly as written, which is how this site shows template syntax inside templates.

## What templates can read

| Name | Contents |
| --- | --- |
| `site` | `title`, `description`, `url`, and `lang` from config |
| `page` | The page's frontmatter, its `url`, and in dynamic routes the entry's fields |
| `entry` | In dynamic routes: the entry's frontmatter, `slug`, `url`, `toc`, `prev`, and `next` |
| `collections` | Every collection by name, as lists of entries |
| `data` | Every file in `data/` by name |

## Styles

A component's `<style>` block is placed in the `page` cascade layer, and a layout's in the `layout` layer. Both come after Mira's own layers, so your styles win without `!important`. All CSS is inlined into the page; there is no stylesheet request.

## Built in elements

The compiler expands a few elements at build time:

| Element | Becomes |
| --- | --- |
| `<mira-code lang="rust">…</mira-code>` | Highlighted code, indentation trimmed |
| `<mira-search>` | A search box backed by the build's index ([Search](search.md)) |

Two attributes are compiled too: `mira-morph` ([Shared elements](shared-elements.md)) and `mira-nav`, which sets `aria-current="page"` on the link that matches the current page or its section.

## Errors

Template mistakes fail the build with the file, the line, and a hint:

```text
✕ {#each} is never closed

  routes/broken.mira:3
  › 3 {#each collections.posts as p}

  hint  add {/each} where the block should end
```
