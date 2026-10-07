# Changelog

All notable changes to Mira are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## Unreleased

## 0.1.0

First public release.

- `mira new`, `mira dev`, and `mira build`, with `--json` output for build results and errors
- Routes, layouts, and templates in `.mira` and Markdown files
- Content collections with typed frontmatter schemas
- Page transitions with View Transitions, shared elements, and route pair overrides
- Speculation Rules prefetch and prerender
- Media frames with AVIF encoding and inline placeholders
- Build-time search index
- Strict Content Security Policy with hashes, and security headers
- Markdown version of every page, `llms.txt`, sitemap, RSS feeds, and JSON-LD
- Host config for Vercel, Netlify, Cloudflare Pages, GitHub Pages, Firebase, Render, Azure Static Web Apps, Docker, Deno Deploy, and S3 with CloudFront
- Redirects written for every host
- `mira migrate` for Next.js, Astro, Hugo, Jekyll, Docusaurus, Gatsby, Eleventy, and VitePress sites
- `mira mcp` to serve site content over MCP
- npm packages: `@buildwithmira/mira` and `create-mira`
