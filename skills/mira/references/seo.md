# SEO and AI search

Get pages found by search engines and cited by AI answer engines, and decide what each kind of crawler may do.

Mira writes the metadata search engines and AI answer engines read, checks it on every build, and lets you choose separately what search, AI answers, and AI training may do with your pages.

## On every page

From frontmatter and config, each page gets a title, description, canonical URL, Open Graph and Twitter card tags, and structured data:

| Page | Structured data |
| --- | --- |
| Home | `WebSite` and `Organization`, with `site.same_as` profiles |
| Collection entries | `BlogPosting` with dates, image, and author |
| Nested pages | `BreadcrumbList` |
| Pages with `faq` | `FAQPage` |

## Frontmatter

```markdown
---
title: Shipping less
description: What this page ships, and why every byte has to justify itself.
updated: 2026-10-01
author: Om Rajguru
image: /covers/shipping-less.png
image_alt: A page loading with no JavaScript
canonical: /posts/shipping-less/
robots: noindex
faq:
  - q: Does Mira ship JavaScript?
    a: Only a runtime of about 1 KB for transitions and prefetching.
---
```

| Key | Effect |
| --- | --- |
| `updated` | The sitemap's `lastmod` and the article's `dateModified` |
| `author` | The article's author, otherwise the site |
| `image`, `image_alt` | The social card for this page, overriding `site.image` |
| `canonical` | A path or full URL to declare as the canonical page |
| `robots` | Written as a robots meta tag; `noindex` also removes the page from the sitemap, `llms.txt`, and search |
| `faq` | A list of `q` and `a`, written as `FAQPage` |

## Site settings

```json
{
  "site": {
    "url": "https://example.com",
    "image": "/og.png",
    "image_alt": "Example, a site built with Mira",
    "twitter": "@example",
    "theme_color": "#0b0b0d",
    "same_as": ["https://github.com/example", "https://x.com/example"]
  }
}
```

`site.url` is needed for canonical URLs, the sitemap, feeds, and absolute social image URLs.

## Crawler policy

Three switches decide who may use your pages:

```json
{
  "agents": {
    "search": "allow",
    "answers": "allow",
    "training": "disallow"
  }
}
```

| Switch | Covers |
| --- | --- |
| `search` | Search engines indexing pages |
| `answers` | AI answer engines that fetch a page to answer a question and cite it: OAI-SearchBot, ChatGPT-User, Claude-SearchBot, Claude-User, PerplexityBot, Perplexity-User, DuckAssistBot, MistralAI-User |
| `training` | Crawlers that collect pages to train models: GPTBot, ClaudeBot, Google-Extended, Applebot-Extended, CCBot, Bytespider, meta-externalagent, cohere-training-data-crawler |

The settings above write:

```text
User-agent: GPTBot
Disallow: /
…

User-agent: *
Content-Signal: search=yes, ai-input=yes, ai-train=no
Allow: /
```

`agents.robots` still sets rules for any single crawler and wins over the switches. Crawler names change over time; check robots.txt after upgrading Mira.

## Build checks

Every build warns when a page has no description of its own, a title over 60 characters, a description outside 50 to 160 characters, or a title or description shared with another page. Pages marked `noindex` and the 404 page are skipped.

## For AI answer engines

What helps a page get cited is the same as what helps people: a clear title, a description that answers the question, headings that name what each section covers, and stable URLs. Mira adds the machine readable parts: a Markdown version of every page, `llms.txt` linking each one, a JSON search index, and on Vercel, the Markdown version served to requests that ask for `text/markdown`. See [Agent surface](agents.md).
