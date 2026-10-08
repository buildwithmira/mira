# Introduction

What Mira is, what kind of sites it builds, and what you get from a build.

Mira builds content sites: documentation, blogs, portfolios, product and editorial pages. You write Markdown and `.mira` templates; `mira build` writes plain HTML files you can host anywhere. This site is built with it.

## What a build gives you

- **Pages that work without JavaScript.** Each page is an `index.html` with its CSS inlined. The optional runtime that handles transitions and prefetching is about 1 KB gzipped.
- **Page transitions.** Navigation animates with the browser's View Transitions. Add `mira-morph` to an element and it travels to its match on the next page.
- **Typed content.** Frontmatter is checked against a schema on every build, and errors name the file and line.
- **Images that never shift the layout.** `<mira-frame>` reserves each image's space, encodes AVIF, and shows a small placeholder until it loads.
- **Search.** An index written at build time, loaded only on pages with a search box.
- **Output agents can read.** A Markdown copy of every page, `llms.txt`, and `mira mcp` to serve the site to MCP clients.
- **Security headers and a strict content security policy**, built from hashes of each page's own code.
- **Config for your host.** Vercel, Netlify, Cloudflare Pages, GitHub Pages, and six more, written on every build.
- **Checks before deploy.** Broken links, missing assets, and pages over their size budget fail the build.

## How a build works

| Source | Folder | Becomes |
| --- | --- | --- |
| Pages and layouts | `routes/`, `layouts/` | HTML pages with inlined CSS |
| Content | `content/`, `data/` | Pages, feeds, search, and Markdown copies |
| Config | `mira.config.json` | Theme, transitions, security policy, host config, and checks |

The compiler is written in Rust. A small site builds in tens of milliseconds; `mira build --timings` shows where the time goes.
