# MCP server

Read every part of a Mira site over MCP, from a project on disk or from any deployed site, with a client config you can paste.

Every Mira site can be read over the Model Context Protocol: its pages as Markdown, search, each collection's entries with their fields, data files, and media. `mira mcp` runs the server over standard input and output, which every MCP client supports.

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

Nothing has to be deployed for this to work. Every build publishes the files the server reads: a Markdown copy of each page, the search index, and the content index under `/_mira/`. Static hosts such as GitHub Pages and S3 work the same as any other.

## Connect to a project

Point `--root` at a project on disk. The server rebuilds it into `.mira/mcp/` before each answer, so answers match your source as you edit:

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
| `site_info` | none | The site's title, description, and URL, and every collection (with entry counts and field types) and data file |
| `list_pages` | none | Every page's URL, title, and description |
| `read_page` | `url`, such as `/docs/routing/` | The page as Markdown, with its title and canonical URL |
| `search` | `query`, optional `limit` from 1 to 25 | The best matches with URL, title, and a snippet |
| `list_entries` | `collection`, optional `offset` and `limit` up to 200 | A collection's entries with every field, and the total |
| `read_data` | `name`, such as `nav` | A data file from `data/`, as JSON |
| `list_media` | none | Every image and video with its alt text, caption, sizes, and file URLs |

Both modes answer the same way. Tool failures, such as an unknown URL or collection, come back as tool results with `isError: true` and a message the agent can act on.

## What a build publishes

| File | Contents |
| --- | --- |
| `/<page>.md` | Each page as Markdown |
| `/_mira/search.json` | Every page's title, description, headings, and text |
| `/_mira/content.json` | The site and its collections and data files |
| `/_mira/collections/<name>.json` | A collection's published entries with all their fields |
| `/_mira/data/<name>.json` | A data file |
| `/media.json` | Images and video, when the site has them |

Draft entries and pages marked `robots: noindex` are left out. To publish pages and search only, without collections and data files, set:

```json
{ "agents": { "content": false } }
```

## Security

- `--url` accepts `https://` addresses, and `http://` only for `localhost`. It follows no redirects, stops after 20 seconds, and reads at most 16 MB per file.
- Page URLs, collection names, and data file names are checked before use, so a request cannot read anything outside the site or its build folder.
- The server only reads. It never writes to the project, and in project mode it builds into `.mira/mcp/`, never `dist/` or your host config files.
- Logs go to standard error, so they never mix with protocol messages.

## Protocol details

The server speaks JSON-RPC 2.0, one message per line, and handles `initialize`, `ping`, `tools/list`, and `tools/call`. A deployed site's indexes are fetched again after 30 seconds, so a long session sees new deploys.
