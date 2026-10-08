# Routing

How files in routes/ become URLs, including dynamic routes and the 404 page.

Every `.md` or `.mira` file in `routes/` is a page. Its path in the folder is its URL.

## File to URL

| File | URL | Output file |
| --- | --- | --- |
| `routes/index.mira` | `/` | `index.html` |
| `routes/about.md` | `/about/` | `about/index.html` |
| `routes/docs/index.mira` | `/docs/` | `docs/index.html` |
| `routes/docs/setup.md` | `/docs/setup/` | `docs/setup/index.html` |
| `routes/posts/[slug].mira` | `/posts/<slug>/` | one file per entry |
| `routes/404.md` | `/404.html` | `404.html` |

URLs end with a slash, and each page is written as `index.html` in its own folder, which every static host serves without configuration. Two files that produce the same URL fail the build and name both files.

## Markdown pages

A `.md` route is rendered as Markdown and wrapped in `<article class="prose">`. Frontmatter sets its title, description, layout, and transition:

```markdown
---
title: About
description: Who we are.
---

# About

Text in Markdown.
```

## Component pages

A `.mira` route is a component with a template and optional styles. Use one when a page needs layout, loops, or data. See [Templates](templates.md).

## Dynamic routes

A file named in brackets, such as `routes/posts/[slug].mira`, renders once for every entry of a collection. The collection is the folder's name, so `routes/posts/[slug].mira` reads `content/posts/`. Set `collection:` in the route's frontmatter to use another:

```mira
---
collection: articles
---
<template>
<article>
  <h1>{{ entry.title }}</h1>
  <slot />
</article>
</template>
```

Inside a dynamic route, `entry` holds the entry's frontmatter, `slug`, and `url`, and `<slot />` renders its Markdown. Dynamic routes must be `.mira` files.

## The 404 page

Add `routes/404.md` or `routes/404.mira` to design your own. Without one, Mira writes a built in 404 page. Hosts serve `404.html` for missing paths; the dev server does too.

## Page titles

The document title is the page's `title` followed by the site title, as in `About · My site`. The home page uses the site title alone.

## Drafts

An entry with `draft: true` is left out of `mira build` and included in `mira dev`, so you can preview it locally.
