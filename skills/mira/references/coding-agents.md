# Working with coding agents

How Mira helps coding agents build sites correctly on the first attempt.

Coding agents build Mira sites for people. Mira gives them predictable conventions, machine readable output, and errors that say how to fix themselves.

## AGENTS.md

`mira new` writes an `AGENTS.md` at the project root. It covers the folder conventions, template syntax, transitions, the design tokens, and the commands, in the format coding agents look for. Keep it updated as your project adds its own conventions.

## JSON output

`mira build --json` prints one JSON object to standard output and nothing else:

```json
{
  "ok": true,
  "schema": 1,
  "report": {
    "pages": [
      {
        "url": "/",
        "source": "routes/index.mira",
        "html_bytes": 21890,
        "gzip_bytes": 7066,
        "morphs": 1
      }
    ],
    "collections": { "posts": 2 },
    "runtime_gzip_bytes": 1058,
    "outputs": ["sitemap.xml", "llms.txt", "robots.txt"],
    "warnings": [],
    "timings": [{ "step": "render", "ms": 1.5 }],
    "duration_ms": 30.2
  }
}
```

`schema` is the output version; it changes only when the shape does.

## Errors an agent can act on

With `--json`, a failure prints the error in the same structure the terminal and the dev overlay use:

```json
{
  "ok": false,
  "schema": 1,
  "error": {
    "message": "layout \"wide\" does not exist",
    "file": "routes/about.md",
    "line": null,
    "hint": "add layouts/wide.mira, or use one of: default",
    "causes": [],
    "excerpt": []
  }
}
```

The exit code is non zero, so scripts and agents can branch on it. See [Checks and errors](errors.md).

## Reading the site back

An agent that changed a page can confirm the result as text with `mira mcp`'s `read` tool, or by reading the page's twin in `dist/`. `mira audit --agent` shows what each page costs an agent to read.
