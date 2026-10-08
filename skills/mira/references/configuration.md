# Configuration

Every key in mira.config.json, with types and defaults.

Mira reads `mira.config.json` from the project root. Every key is optional. Unknown keys fail the build, so a typo never passes silently.

```json
{
  "site": {
    "title": "My site",
    "description": "Notes on design and code.",
    "url": "https://example.com",
    "lang": "en"
  },
  "scheme": "dark",
  "transitions": {
    "default": "fade",
    "pairs": { "/posts/ -> /posts/*": "slide-up" }
  },
  "prefetch": { "eagerness": "moderate", "prerender": true },
  "theme": { "ink": "#f4f1ea" },
  "fonts": [
    { "family": "Inter", "src": "/fonts/Inter.woff2", "preload": true }
  ],
  "collections": {
    "posts": { "fields": { "title": "string", "date": "date" } }
  },
  "agents": { "twins": true, "robots": { "*": "allow" } },
  "headers": { "emit": true, "hsts": true },
  "hosts": { "vercel": {} },
  "redirects": { "/old/": "/new/" },
  "budgets": { "page_kb": 14 }
}
```

## site

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `title` | string | `Mira site` | Site name, used in titles, feeds, and llms.txt |
| `description` | string | none | Default page description |
| `url` | string | none | Absolute origin such as `https://example.com`; enables canonical URLs, the sitemap, and feeds |
| `lang` | string | `en` | The `lang` attribute of every page |
| `image` | string | none | Social card image under `public/`, such as `/og.png` |
| `image_alt` | string | none | Alt text for the social card |
| `twitter` | string | none | The site's X handle, such as `@example` |
| `theme_color` | string | none | Browser UI color, such as `#0b0b0d` |
| `same_as` | list | `[]` | Profile URLs for the site's structured data |

## scheme

`dark`, `light`, or `system`. Default `dark`. See [Theming](theming.md#light-or-dark).

## transitions

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `default` | string | `fade` | Transition when nothing more specific applies |
| `pairs` | object | `{}` | `"<from> -> <to>"` keys mapped to transition names |

See [Page transitions](page-transitions.md).

## prefetch

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `eagerness` | string | `moderate` | `conservative`, `moderate`, or `eager` |
| `prerender` | boolean | `true` | Prerender on press |

## theme

An object of token overrides. Nested keys join with dashes into custom property names: `{ "radius": { "md": "10px" } }` sets `--radius-md`. Values are strings or numbers. See [Theming](theming.md) for the tokens you can set.

## fonts

A list of faces with `family`, `src`, `weight`, `style`, and `preload`. See [Fonts](fonts.md).

## collections

Schemas keyed by collection name, each with `fields` and an optional `strict` (default `true`). See [Content collections](collections.md#schemas).

## actions

Typed requests agents can make, keyed by name, each with a `description`, `input` fields typed as in collection schemas, an `endpoint` that receives the input as a JSON `POST`, and `confirm` (default `true`). See [Actions](actions.md).

## agents

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `twins` | boolean | `true` | Write Markdown twins, llms.txt, and llms-full.txt |
| `robots` | object | `{ "*": "allow" }` | User agent to `allow` or `disallow`; wins over the switches below |
| `search` | string | `allow` | Search engine indexing |
| `answers` | string | `allow` | AI answer engines that fetch and cite pages |
| `training` | string | `allow` | Crawlers that collect pages for AI training |
| `content` | boolean | `true` | Publish collections and data files under `/_mira/` for agents and [MCP](mcp.md) |

## headers

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `emit` | boolean | `true` | Write `_headers` and `vercel.json` |
| `hsts` | boolean | `true` | Include `Strict-Transport-Security` |

## hosts

The hosts to write config for, each mapped to its options. Mira writes the host's own config file on every build. See [Deploying](deploying.md#hosts) for what each one writes.

| Key | Options |
| --- | --- |
| `vercel`, `netlify`, `github`, `firebase`, `azure`, `docker`, `deno`, `s3` | none: `{}` |
| `cloudflare` | `project`: the Pages project name |
| `render` | `service`: the service name |

An unknown host fails the build and lists the known ones.

## redirects

Old paths mapped to new paths or URLs, as permanent redirects. Each one is written in every host's format, plus a redirect page at the old path. Sources start with `/`; targets are a path starting with `/` or an `https://` URL. See [Deploying](deploying.md#redirects).

## budgets

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `page_kb` | number | none | Largest gzipped page size in KB; larger pages fail the build |
