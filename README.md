<p align="center"><img src="assets/mira-mark.svg" width="56" alt="Mira"></p>

<h1 align="center">Mira</h1>

<p align="center">A static site framework for content sites, written in Rust.</p>

<p align="center">
  <a href="https://github.com/buildwithmira/mira/actions/workflows/ci.yml"><img src="https://github.com/buildwithmira/mira/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://www.npmjs.com/package/@miraframework/mira"><img src="https://img.shields.io/npm/v/@miraframework/mira" alt="npm"></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue" alt="License: MIT OR Apache-2.0"></a>
</p>

Mira turns Markdown and `.mira` templates into plain HTML files. Pages work without JavaScript. When the browser supports it, navigation between pages uses native View Transitions, so a site can move like an app while staying a set of static files.

```bash
npm create mira@latest my-site
cd my-site
npm install
npm run dev
```

## Features

- **Static output.** Every page is an `index.html` with its CSS inlined. The optional runtime is about 1 KB gzipped, and the build fails if it ever exceeds 2 KB.
- **Page transitions.** Cross-document View Transitions with shared elements (`mira-morph`), per-route transition pairs, direction awareness, and reduced-motion support. Links are prefetched or prerendered with Speculation Rules.
- **Content collections.** Markdown entries in `content/` with typed frontmatter schemas, validated on every build.
- **Templates.** `.mira` files with `{{ }}` expressions, `{#if}`, `{#each}`, layouts, and slots. Output is escaped by default.
- **Media.** `<mira-frame>` reserves space for each image, encodes AVIF, and draws an 8×8 placeholder inline until the image loads.
- **Search.** A search index is written at build time and loaded only by pages that use search.
- **Security.** A strict Content Security Policy with a hash for every inline script and style, plus security headers for every supported host.
- **Machine-readable output.** Each page has a Markdown version, plus `llms.txt`, a sitemap, RSS feeds, and JSON-LD. `mira build --json` reports results and errors as JSON.
- **Hosting.** Writes native config for Vercel, Netlify, Cloudflare Pages, GitHub Pages, Firebase, Render, Azure Static Web Apps, Docker (nginx), Deno Deploy, and S3 with CloudFront.
- **Migration.** `mira migrate` imports Next.js, Astro, Hugo, Jekyll, Docusaurus, Gatsby, Eleventy, and VitePress sites, and writes redirects for every URL that changes.

The starter site builds in about 20 ms, with pages between 5 and 7 KB gzipped.

## Install

Mira is pre-release. It ships as a single native binary for Linux (x64, arm64), macOS (Apple silicon, Intel), and Windows (x64, arm64).

**npm.** Installs the binary for your platform. Node.js 18 or later.

```bash
npm install --save-dev @miraframework/mira
```

**Prebuilt binaries.** Attached to each [release](https://github.com/buildwithmira/mira/releases), with SHA-256 checksums.

**From source.** Requires [Rust](https://rustup.rs).

```bash
cargo install --git https://github.com/buildwithmira/mira mira
```

## Commands

| Command | What it does |
| --- | --- |
| `mira new <dir>` | Create a site from the starter template |
| `mira dev` | Serve the site on `localhost:4321`, rebuilding and reloading on change |
| `mira build` | Build the site to `dist/` |
| `mira migrate <from> <to>` | Move an existing site into a new Mira project |
| `mira mcp` | Serve the site's content over MCP on standard input and output |

Run `mira <command> --help` for options.

## Project layout

```text
my-site/
├── mira.config.json     site settings, collections, hosts, redirects
├── routes/              pages: index.mira, about.md, posts/[slug].mira
├── layouts/             default.mira wraps every page
├── content/             collections of Markdown entries
└── public/              copied to the output as is
```

## Repository

| Path | Contents |
| --- | --- |
| `crates/mira_compiler` | The compiler library: config, routing, templates, Markdown, media, outputs, and host adapters |
| `crates/mira_cli` | The `mira` binary: commands, dev server, MCP server, terminal output |
| `crates/mira_compiler/runtime` | The client runtime and base CSS embedded in every build |
| `crates/mira_compiler/starter` | The template used by `mira new` |
| `npm/` | The npm packages: `@miraframework/mira`, its per-platform binaries, and `create-mira` |
| `tools/check_host.py` | Checks a deployed site's status codes, headers, and content types |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). To report a security issue, follow [SECURITY.md](SECURITY.md) and do not open a public issue.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT), at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in Mira by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
