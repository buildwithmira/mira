---
name: mira
description: Build, edit, check, and deploy websites made with Mira, the static site framework, and read any Mira site over MCP. Use when a project has a mira.config.json or routes/*.mira files, when asked to create or change pages, layouts, blog posts, collections, styles, or images on a Mira site, to fix a Mira build error, to deploy a Mira site, or to connect an agent to a Mira site with `mira mcp`.
---

# Mira

Mira compiles Markdown and `.mira` templates into static HTML. A project has `mira.config.json` at its root. Detailed references for every topic are in `references/`; this file is the working guide.

## Workflow

1. Read `mira.config.json` and the project's `AGENTS.md` if it has one.
2. Make the change in `routes/`, `layouts/`, `content/`, `data/`, `public/`, or `mira.config.json`.
3. Build with JSON output and read the result:
   ```bash
   npx mira build --json
   ```
   `{"ok": true, ...}` means it built. On failure, `error` has `message`, `file`, `line`, `hint`, and an `excerpt`. Fix that file and line, then build again. Repeat until `ok` is true.
4. Treat every warning in `report.warnings` as something to fix, such as a description outside 50 to 160 characters.
5. To look at the site, run `npm run dev` (or `npx mira dev`) and open `http://localhost:4321`.

Never edit `dist/` or `.mira/`; they are build output. Never hand-edit `vercel.json`, `netlify.toml`, `wrangler.toml`, or other host files written from the `hosts` setting; change `mira.config.json` instead.

## Project layout

```text
mira.config.json   site, theme, transitions, collections, hosts, redirects
routes/            pages: index.mira -> /, about.md -> /about/, posts/[slug].mira
layouts/           default.mira wraps every page
content/<name>/    Markdown entries of a collection
data/              JSON or YAML, read in templates as data.<name>
public/            copied to the output as is
```

## Pages

- `routes/about.md` is a Markdown page at `/about/`; `routes/docs/index.mira` is `/docs/`.
- `routes/posts/[slug].mira` renders once per entry of the collection named by its folder (`content/posts/`), or by `collection:` in its frontmatter. Dynamic routes must be `.mira` files.
- `routes/404.md` or `routes/404.mira` is the not found page.
- Frontmatter Mira understands: `title`, `description`, `slug`, `draft`, `layout` (a name or `false`), `transition`, `toc`, `order`, `robots`, `canonical`, `updated`, `image`, `image_alt`, `author`, `faq`.
- Every page should have a `title` and a `description` of 50 to 160 characters; the build warns otherwise.

## Templates (`.mira`)

A `.mira` file has optional frontmatter, a `<template>` block, and an optional `<style>` block scoped to the page.

```mira
---
title: Writing
description: Every post on this site, newest first, with a one-line summary of each.
---
<template>
<h1>{{ page.title }}</h1>
{#if collections.posts}
<ol>
  {#each collections.posts as post}
  <li><a href="{{ post.url }}" mira-morph="title-{{ post.slug }}">{{ post.title }}</a></li>
  {/each}
</ol>
{:else}
<p>No posts yet.</p>
{/if}
</template>
```

- `{{ path }}` prints an escaped value. `{{ unsafe path }}` prints raw HTML; avoid it, and never use it on user or content input.
- Values: `page.*`, `site.*`, `entry.*` (in dynamic routes), `collections.<name>`, `data.<name>`. Index lists with `.0`, and count them with `.length`.
- `{#if path}` tests truthiness, `{#if !path}` negates it. There are no comparisons or filters; prepare values in frontmatter or data files instead.
- `<slot />` renders the wrapped page in a layout, and an entry's Markdown in a dynamic route.

## Collections

Declare a schema in `mira.config.json`, then add Markdown files to `content/<name>/`:

```json
{ "collections": { "posts": { "fields": { "title": "string", "date": "date", "tags": "string[]?" } } } }
```

A collection can come from Sanity, Contentful, Supabase, any GraphQL API, or any JSON endpoint: add `"source"` to its schema (see `references/content-sources.md`). Tokens go in environment variables named by `token_env`, `key_env`, or `headers`, never in the config. Entries and pages can be `.mdx`, with components in `components/<Name>.mira` reading `{{ props.x }}` and `<slot />`; no JavaScript runs (see `references/mdx.md`).

Types are `string`, `number`, `boolean`, `date` (`YYYY-MM-DD`), `time` (`HH:MM`), `datetime`, `url`, `object`, `string[]`, `number[]`, and `object[]`; end a type with `?` to make it optional. Declare prices, dates, and times with their real types: agents query collections by them over MCP. Unknown fields fail the build unless the schema sets `"strict": false`. Entries sort by `order`, then newest `date`. `draft: true` hides an entry from production builds.

## Markdown

GitHub flavored Markdown with highlighted code fences, tables, task lists, footnotes, and callouts (`> [!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]`, `[!CAUTION]`). Headings get ids, and `entry.toc` lists an entry's headings with their ids and levels.

## Images and video

Use `<mira-frame>`, or a plain Markdown image, which becomes one:

```html
<mira-frame src="./diagram.png" alt="What the image shows" caption="Optional caption"></mira-frame>
```

- `alt` is required, unless the image is purely decorative (`alt=""`).
- Video needs `width` and `height`.
- `./x.png` is relative to the file, `/x.png` is under `public/`, `@/x.png` is under the project root. Remote URLs are not allowed; download the file into the project.
- In Markdown, keep the whole `<mira-frame>` tag on one line. A tag split across lines renders as text.

## Motion

- Every navigation crossfades by default. Set `transitions.default` and `transitions.pairs` in config, or `transition:` per page.
- Add `mira-morph="name"` to an element on two pages to animate it from one to the other. A name used twice on one page fails the build.

## Styling

- Theme values are CSS custom properties: `--ink`, `--surface-0`, `--line`, `--font-display`, `--space-4`, `--radius-md`, and more. Change them under `theme` in config.
- Built in components: `mira-btn`, `mira-tabs`, `mira-card`, `<mira-code>`, and `<mira-search>`.
- Page and layout `<style>` blocks win over Mira's styles without `!important`.

## Deploying

Name hosts in config, and each build writes their config files:

```json
{ "site": { "url": "https://example.com" }, "hosts": { "vercel": {} }, "redirects": { "/old/": "/new/" } }
```

Hosts: `vercel`, `netlify`, `cloudflare`, `github`, `firebase`, `render`, `azure`, `docker`, `deno`, `s3`. Any other static host works by uploading `dist/`.

## MCP

Every Mira site can be read over MCP: pages as Markdown, search, collection entries with their fields, data files, and media.

```json
{ "mcpServers": { "site": { "command": "npx", "args": ["-y", "@miraframework/mira", "mcp", "--url", "https://example.com"] } } }
```

Use `--root <dir>` instead of `--url` for a project on disk. Tools: `site`, `pages`, `search`, `read` (a page, or one section with `path#section`), `items` (query a collection by field), `data`, and `media`. Reading a deployed site with `--url` needs Mira 0.2.0 or later.

Actions let agents act on a site, such as booking a table. Declare them in `mira.config.json` under `actions`, each with a `description`, typed `input` fields (collection types), and an HTTPS `endpoint` that receives the input as a JSON `POST`. `mira mcp` offers each as a tool and sends only after the person agrees. See `references/actions.md`.

Run `mira audit --agent` after changing content to see what each page costs an agent to read; split pages over 1,000 tokens with `##` headings.

## References

| Topic | File |
| --- | --- |
| Install and first site | `references/installation.md`, `references/quick-start.md` |
| Folders and URLs | `references/project-structure.md`, `references/routing.md` |
| Template syntax | `references/templates.md`, `references/layouts.md` |
| Collections and data | `references/collections.md`, `references/data-files.md`, `references/content-sources.md` |
| MDX | `references/mdx.md` |
| Markdown | `references/markdown.md` |
| Transitions | `references/page-transitions.md`, `references/shared-elements.md` |
| Styling | `references/theming.md`, `references/components.md`, `references/fonts.md` |
| Images and video | `references/media.md` |
| Search and performance | `references/search.md`, `references/performance.md` |
| SEO and agent files | `references/seo.md`, `references/agents.md` |
| MCP and actions | `references/mcp.md`, `references/actions.md` |
| Coding agents and JSON output | `references/coding-agents.md` |
| Security and build checks | `references/security.md`, `references/errors.md` |
| Every config key and command | `references/configuration.md`, `references/cli.md` |
| Hosts and redirects | `references/deploying.md` |
| Overview | `references/introduction.md` |
