# Search

A build time index for every page and a search box that reads it.

Every build writes a search index to `/_mira/search.json`. Add a search box with one element:

```html
<mira-search placeholder="Search docs"></mira-search>
```

The element's script is a separate file that loads only on pages that use it, and the index is fetched the first time the box is focused. Pages without search pay nothing.

## Using it

- Press `/` anywhere on the page to focus the box.
- Results update as you type, best match first, with the matching words highlighted.
- Use the arrow keys to move through results and `Escape` to close them.
- When a term matches a section heading, the result links straight to that section.

## Ranking

A page scores for each search term found in its title, then its section headings, then its description, then its text. Every term must match somewhere for a page to appear.

## The index

The index is a JSON array with one object per page:

```json
{
  "url": "/docs/routing/",
  "title": "Routing",
  "description": "How files in routes/ become URLs.",
  "headings": [{ "id": "dynamic-routes", "text": "Dynamic routes" }],
  "text": "Every .md or .mira file in routes/ is a page…"
}
```

Text comes from the page's `<main>` element, without navigation, scripts, or SVG, and is capped at 1,600 characters per page; titles and headings carry most of the weight. The 404 page is left out.

The same file is a search API for agents: fetch it, filter it, and follow the canonical URLs. `mira mcp` exposes it as a `search` tool. See [MCP server](mcp.md).

## Styling

The box and its results use the design tokens. Restyle them with `mira-search input` and `mira-search ol`.
