# MCP server

Read every part of a Mira site over MCP, from a project on disk or from any deployed site, with a client config you can paste.

Every Mira site can be read over the Model Context Protocol: its pages as Markdown, search, each collection's entries with typed fields, data files, and media. `mira mcp` runs the server over standard input and output, which every MCP client supports.

## Connect to a deployed site

Give `--url` the address of any site built with Mira, on any host:

```json
{
  "mcpServers": {
    "my-site": {
      "command": "npx",
      "args": ["-y", "@miraframework/mira", "mcp",
               "--url", "https://example.com"]
    }
  }
}
```

The site needs no server of its own. Every build publishes the files the server reads: a Markdown copy of each page, the search index, and the content index under `/_mira/`. Static hosts such as GitHub Pages and S3 work the same as any other.

## Connect to a project

Point `--root` at a project on disk. The server builds it into `.mira/mcp/` on the first call, and again only when a file in the project changes, so answers match your source as you edit:

```json
{
  "mcpServers": {
    "my-site": {
      "command": "npx",
      "args": ["-y", "@miraframework/mira", "mcp",
               "--root", "/path/to/my-site"]
    }
  }
}
```

If `mira` is already on your `PATH`, use `"command": "mira"` and drop the first two arguments.

## Tools

| Tool | Arguments | Returns |
| --- | --- | --- |
| `site` | none | The site's title and URL, and every collection (with entry counts and field types) and data file |
| `pages` | optional `prefix`, such as `/docs/` | One line per page: path, title, and description |
| `search` | `query`, optional `limit` (default 5, at most 20) | The best matches, each with a score, a `path#section` link, the title, and a short snippet |
| `read` | `path`, optional `section` and `max_chars` | The page as Markdown, or one section of it |
| `items` | `collection`, optional `where`, `fields`, `sort`, `limit` (default 20), and `offset` | The matching entries and their total |
| `data` | `name`, optional `path` such as `hours.monday` | A data file from `data/`, or one value in it, as JSON |
| `media` | optional `page` | Images and video with alt text, captions, sizes, and file URLs |

Each [action](actions.md) the site declares, such as `book_table`, is a tool too, with an input schema built from its fields.

Tool failures, such as an unknown page or collection, come back as tool results with `isError: true` and a message the agent can act on, such as the list of sections a page has.

## Read less

Every answer is sized for an agent's context window.

- **Search points at sections.** A hit like `/docs/deploying/#caching` goes straight to `read`, which returns only that section: the heading and everything under it, up to the next heading at the same level.
- **Long pages are cut, not dumped.** `read` returns up to 24,000 characters by default. A page cut short ends with how much is left and the ids of its remaining sections. Set `max_chars` to go lower.
- **Markdown copies hold content only.** Navigation, footers, buttons, forms, and anything marked `aria-hidden="true"` or `hidden` are left out, and so is decoration such as empty icons.

## Query collections

`items` filters a collection by its fields before anything is sent, so an agent asks for the five talks after a given time instead of reading every event:

```json
{
  "collection": "events",
  "where": { "starts": { "gte": "2026-10-07T18:00" }, "tags": "talk" },
  "fields": ["title", "starts", "speaker"],
  "sort": "starts",
  "limit": 5
}
```

| Condition | Matches when the field |
| --- | --- |
| `"field": value` | Equals the value. For a list field, any item does |
| `{ "ne": value }` | Does not equal the value |
| `{ "gt": value }`, `gte`, `lt`, `lte` | Is greater than or less than the value |
| `{ "contains": value }` | Contains the text, or for a list field, includes the item |
| `{ "in": [a, b] }` | Equals any of the values |

Numbers compare as numbers and everything else as text, ignoring case, so `date`, `time`, and `datetime` fields compare in time order. Fields can be nested, as in `hours.monday`. `sort` takes a field name, with `-` in front for descending order. Without `fields`, each entry comes back with every field except its Markdown body; name `markdown` in `fields` to include it.

Declare the field types in the collection's schema, such as `"price": "number"` and `"starts": "datetime"`, and the build checks every entry, so an agent can trust what it queries. See [Content collections](collections.md).

## What a build publishes

| File | Contents |
| --- | --- |
| `/<page>.md` | Each page as Markdown |
| `/_mira/search.json` | Every page's title, description, headings, and opening text |
| `/_mira/content.json` | The site and its collections and data files |
| `/_mira/collections/<name>.json` | A collection's published entries with all their fields |
| `/_mira/data/<name>.json` | A data file |
| `/media.json` | Images and video, when the site has them |

Draft entries and pages marked `robots: noindex` are left out. To publish pages and search only, without collections and data files, set:

```json
{ "agents": { "content": false } }
```

## Check what agents read

`mira audit --agent` builds the site and reports what each page costs an agent, in approximate tokens: its Markdown copy, its search index entry, and its largest section. Pages over the budget, 1,000 tokens unless you pass `--budget`, are listed with what to change:

```bash
mira audit --agent
mira audit --agent --budget 1500 --json
```

## Security

- `--url` accepts `https://` addresses, and `http://` only for `localhost`. It follows no redirects, stops after 20 seconds, and reads at most 16 MB per file.
- Page paths, collection names, and data file names are checked before use, so a request cannot read anything outside the site or its build folder.
- The server never writes to the project. In project mode it builds into `.mira/mcp/`, never `dist/` or your host config files, and with `--url` it only reads the deployed site's files.
- Only [action](actions.md) tools send anything: a JSON `POST` to the endpoint the site declares, after the person agrees, unless the action sets `"confirm": false`.
- Logs go to standard error, so they never mix with protocol messages.

## Protocol details

The server speaks JSON-RPC 2.0, one message per line, and handles `initialize`, `ping`, `tools/list`, and `tools/call`. One server process serves the whole session and keeps what it has read in memory, so calls after the first answer in about a millisecond. A deployed site's files are fetched again after 30 seconds, so a long session sees new deploys.
