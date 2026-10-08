# Changelog

All notable changes to Mira are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## Unreleased

- A `<mira-frame>` tag split across lines in Markdown fails the build at that line. Markdown would have shown it as text.
- Actions read from a deployed site with `mira mcp --url` cannot reach `localhost` or loopback, private, or link-local addresses, including names that resolve to them, and always ask the person first, even when the site sets `confirm: false`. A confirmation covers the endpoint the person saw as well as the input.
- Cached responses from CMS and API sources are kept separately for each set of credentials, and a source that refuses its token (HTTP 401 or 403) fails the build and drops its cached copy instead of falling back to it.
- `mira audit --agent` refuses to run when `agents.twins` is `false`, and flags pages without a Markdown copy instead of counting them as empty.

## 0.2.0

- Agent skills: `mira` for building and deploying Mira sites, and `mira-migrate` for moving sites to Mira. Install with `npx skills add buildwithmira/mira`.
- Every Mira site can be read over MCP, wherever it is deployed. `mira mcp --url https://example.com` serves any deployed Mira site, on any host, from the files its build publishes.
- Collections from a CMS or API: Sanity, Contentful, Supabase, any GraphQL API (Hygraph, Shopify, WordPress, Strapi), or any JSON endpoint, including functions on AWS, Google Cloud, and Azure. Rich text and Portable Text become Markdown, their images are downloaded and encoded like local media, tokens come from environment variables, and responses are cached so a failed request falls back to the last copy with a warning.
- MDX: `.mdx` pages and entries render capitalized tags with components from `components/<Name>.mira`, with props and Markdown children, at build time. Exports and expressions are build errors with their line, since no JavaScript runs.
- AWS Amplify Hosting: `hosts.amplify` writes `amplify.yml` and `customHttp.yml`.
- Actions: declare typed requests such as `book_table` in `mira.config.json`, each with input fields, a description, and an HTTPS endpoint that receives the input as JSON. Builds publish them at `/_mira/actions.json` and in `llms.txt`, and `mira mcp` offers each as a tool. Input is checked against its types, and nothing is sent until the person using the agent agrees, through the client's own prompt (MCP elicitation) or a one-time confirmation tied to the exact input.
- MCP tools, renamed and shortened: `site`, `pages`, `search`, `read`, `items`, `data`, and `media`. Clients discover tools when they connect, so existing configs keep working.
- `mira mcp` keeps one build for the whole session and rebuilds a project only when one of its files changes. Calls after the first answer in about a millisecond, down from 120 to 150 ms.
- `search` returns five matches by default, each with a score, a `path#section` link to the heading that matches, and a snippet of at most 160 characters.
- `read` takes `path#section` or `section` to return one section, and `max_chars` to cap the answer. A page cut short lists its remaining sections.
- `items` queries a collection: filter with `where` (`gt`, `gte`, `lt`, `lte`, `ne`, `contains`, `in`), pick `fields`, and `sort`. Entries come back without their Markdown body unless asked for.
- `data` takes a dotted `path`, such as `hours.monday`, to return one value from a data file. `pages` takes a `prefix`, and `media` a `page`.
- Collection schemas accept `time`, `datetime`, `object`, `number[]`, and `object[]`, so hours, schedules, and prices can be typed and queried.
- `llms.txt` is a router: each page lists its approximate size in tokens, `llms-full.txt` its total, and a Data section links each collection's JSON with its entry count and fields.
- `mira audit --agent` reports what each page costs an agent to read: the tokens in its Markdown copy, its search index entry, and its largest section, flagging pages over a budget.
- Markdown copies of component pages leave out footers, buttons, forms, anything marked `aria-hidden="true"` or `hidden`, and empty decorative elements, which used to show up as stray `*` and `#` characters.
- The search index separates words that sat in neighboring `<span>` elements and no longer adds spaces around inline code and links.
- Each build writes a content index for agents: `/_mira/content.json`, plus `/_mira/collections/<name>.json` and `/_mira/data/<name>.json`. Turn it off with `agents.content: false`. `llms.txt` links the index and shows how to connect over MCP.
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
