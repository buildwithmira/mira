# Content collections

Group Markdown entries, type their frontmatter, and sort, link, and list them.

A collection is a folder in `content/`. Each Markdown file in it, including in subfolders, is an entry with frontmatter and a body.

```text
content/
└── posts/
    ├── motion-is-navigation.md
    └── ship-less.md
```

## Entries

```markdown
---
title: Ship less
description: Every byte sent to a browser must justify itself.
date: 2026-09-21
---

The body is Markdown.
```

Each entry gets:

| Field | Value |
| --- | --- |
| `slug` | The `slug` frontmatter, or the file name without `.md` |
| `url` | Set when a dynamic route renders the collection, otherwise `null` |
| `reading_time` | Minutes at 230 words per minute, at least 1 |
| `toc` | Second and third level headings: `level`, `id`, `text` |
| `prev`, `next` | The neighboring entries' `title` and `url`, in collection order |

Slugs cannot contain spaces, slashes, `?`, or `#`.

## Listing a collection

Every template can read `collections.<name>`:

```mira
{#each collections.posts as post}
  <a href="{{ post.url }}">{{ post.title }}</a>
{/each}
```

## Rendering entries as pages

A dynamic route renders one page per entry. See [Routing](routing.md#dynamic-routes).

## Order

Entries with a numeric `order` come first, lowest first. The rest follow newest first by `date`, then alphabetically by slug. These docs use `order`; a blog uses `date`.

## Schemas

Declare the fields a collection's frontmatter must have in `mira.config.json`:

```json
{
  "collections": {
    "posts": {
      "fields": {
        "title": "string",
        "description": "string?",
        "date": "date",
        "tags": "string[]?"
      }
    }
  }
}
```

| Type | Accepts |
| --- | --- |
| `string` | Text |
| `number` | Numbers |
| `boolean` | `true` or `false` |
| `date` | A date like `2026-10-07` |
| `url` | A path starting with `/`, or an `http` or `https` URL |
| `string[]` | A list of text values |

Add `?` to make a field optional. Fields not in the schema fail the build, which catches typos such as `auther:`; set `"strict": false` on the collection to allow them. Mira's own keys (`slug`, `draft`, `layout`, `transition`, and `order`) are always allowed.

A failing entry stops the build at the exact line:

```text
✕ `date` must be a date like 2026-10-07, got "September"

  content/posts/ship-less.md:4
  › 4 date: September

  hint  the schema in mira.config.json declares date: date
```

## Drafts

`draft: true` keeps an entry out of production builds. The dev server still shows it.

## Feeds

When `site.url` is set, every collection with a dynamic route gets an RSS feed at its section path, such as `/posts/rss.xml`, linked from every page.
