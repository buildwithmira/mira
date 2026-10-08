# Changelog

All notable changes to Mira are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## Unreleased

- Fixed: `mira dev` and `mira mcp` no longer rewrite host config files such as `netlify.toml` in the project root. They pointed those files at a private build folder, so a deploy after running `mira dev` could publish the wrong directory.
- Fixed: code blocks in Markdown keep the space between them and the text around them, and inline code no longer breaks across lines in the middle of a token such as `--help`. On screens narrower than 40rem, wide tables scroll inside themselves and long inline code wraps, so neither makes the page scroll sideways.

## 0.1.2

- The npm package page shows the full README, including how to connect `mira mcp` to an MCP client and how to deploy to each supported host.
- npm packages are released with trusted publishing and staged publishing: npm verifies each release against this repository's release workflow, no npm token is stored, and each version goes live only after a maintainer approves it with 2FA.
- Release and CI workflows use the current versions of the GitHub actions they depend on.

## 0.1.1

- npm packages are published under the `@miraframework` scope: `@miraframework/mira`, its platform packages, and `create-mira`. 0.1.0 was released on GitHub only.

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
