# Quick start

Create a site, run it locally with live reload, add a post, and build it for production in five commands.

## Create a site

```bash
npm create mira@latest my-site
cd my-site
npm install
```

The starter has a home page, an about page, a blog with two posts, a layout, `mira.config.json`, and an `AGENTS.md` that tells coding agents how the project works.

## Run it locally

```bash
npm run dev
```

The site is served at `http://localhost:4321` and rebuilds when you save. If a build fails, an overlay shows the file, the line, and a fix; save a correction and it clears.

Open **Writing**, then a post. The title morphs from the list into the post, and the back button plays it in reverse.

## Add a post

Create `content/posts/hello.md`:

```markdown
---
title: Hello
description: My first post on my new Mira site, written in Markdown.
date: 2026-10-08
---

Written in Markdown, rendered at build time.
```

It appears at `/posts/hello/` and at the top of `/posts/`. Every post is checked against the schema in `mira.config.json`, so a missing `title` or a malformed `date` fails with the exact line. See [Content collections](collections.md).

## Build

```bash
npm run build
```

The site lands in `dist/` as static files. The summary lists each page's gzipped size against its budget, and the files written for agents and hosts.

## Deploy

Name your host in `mira.config.json` and the build writes its config file:

```json
{ "hosts": { "vercel": {} } }
```

Then deploy `dist/`. See [Deploying](deploying.md) for all ten supported hosts.
