# Agent surface

Markdown copies of every page, llms.txt, a search index, a content index for MCP, a crawler policy, and structured data, written on every build.

People read a site through the browser. Agents do better with text, structure, and stable URLs. Mira writes both from the same source on every build, so they never drift apart.

## Markdown twins

Every page has a Markdown version at the same path with `.md`:

| Page | Twin |
| --- | --- |
| `/` | `/index.md` |
| `/docs/routing/` | `/docs/routing.md` |

A twin starts with frontmatter naming the page and its canonical URL:

```markdown
---
title: "Routing"
url: "https://mira.example/docs/routing/"
description: "How files in routes/ become URLs."
---

# Routing

Every .md or .mira file in routes/ is a page…
```

Pages written in Markdown reuse their source. Component pages are converted from their rendered `<main>` element, keeping headings, lists, links, code, and images. Navigation, footers, buttons, forms, scripts, anything marked `aria-hidden="true"` or `hidden`, and empty decorative elements are left out, so a twin holds content only. Each page links its twin with `<link rel="alternate" type="text/markdown">`. Turn twins off with `"agents": { "twins": false }`.

## llms.txt

`/llms.txt` follows the llms.txt format and works as a router: the site's title and description, then every page grouped by collection, each linking to its twin with a one line description and its approximate size in tokens. An agent fetches only the pages it needs:

```markdown
- [Deploying](https://example.com/docs/deploying.md): Publish the dist folder to any host. (~1244 tokens)
```

`/llms-full.txt` is every twin in one file, for agents that want the whole site in one request; `llms.txt` gives its size too. A Data section links the JSON for each collection and data file, with entry counts and field names.

## Search API

`/_mira/search.json` lists every page with its title, description, headings, and text. See [Search](search.md#the-index).

## Content index

`/_mira/content.json` describes the site and lists every collection, with its entry count and field types, and every data file. Each collection's published entries, with all their fields, are in `/_mira/collections/<name>.json`, and each data file in `/_mira/data/<name>.json`. Set `agents.content` to `false` to publish pages and search only.

## MCP

Any MCP client can read the whole site through these files, from a project or from the deployed URL. `llms.txt` ends with the command to connect. See [MCP server](mcp.md).

## Crawler policy

For separate switches covering search, AI answers, and AI training, see [SEO and AI search](seo.md#crawler-policy).

```json
{
  "agents": {
    "robots": { "*": "allow", "ExampleBot": "disallow" }
  }
}
```

Each agent gets `allow` or `disallow`, written to `/robots.txt` with specific agents first and the catch all last. When `site.url` is set, the file also points to the sitemap.

## Sitemap and feeds

With `site.url` set, the build writes `/sitemap.xml`, with `lastmod` from each page's `date`, and an RSS feed per collection with a dynamic route. Without it, Mira skips them and says so, since both need absolute URLs.

## Structured data

Every page has Open Graph tags for its title, description, type, and URL. Collection entries also get JSON-LD as a `BlogPosting` with the headline, description, publish date, and URL.

## Stable markup

Mira's own markup is semantic: one `<main>`, labeled navigation, headings in order, and `aria-current` on the current link. Transitions are skipped for automated browsers so the DOM stays still while an agent reads it.
