# CLI

Every mira command and option, from creating a site to serving it to agents.

```text
mira <command> [options]
```

`mira --help` lists commands, `mira <command> --help` lists a command's options, and `mira --version` prints the version.

## mira new

```bash
mira new <dir>
```

Creates a site from the starter in `<dir>`, which must be new or empty. The starter has a welcome page, an about page, a blog with two posts and a schema, a layout, a favicon, `mira.config.json`, `AGENTS.md`, and a `.gitignore`.

## mira dev

```bash
mira dev [--root <dir>] [--port <n>]
```

| Option | Default | Meaning |
| --- | --- | --- |
| `--root` | `.` | Project folder |
| `--port` | `4321` | Port to listen on |

Builds into `.mira/dev/`, serves it at `http://localhost:<port>`, and rebuilds when a file in the project changes. Changes inside `.mira/`, `dist/`, `.git/`, `node_modules/`, and `target/` are ignored. Pages reload after each successful build, and failed builds show the [error overlay](errors.md#the-dev-overlay). Drafts are included. The server only listens on `127.0.0.1`.

## mira build

```bash
mira build [--root <dir>] [--out <dir>] [--json] [--timings]
```

| Option | Default | Meaning |
| --- | --- | --- |
| `--root` | `.` | Project folder |
| `--out` | `dist` | Output folder, relative to the project |
| `--json` | off | Print a JSON report or error to standard output instead of the summary |
| `--timings` | off | Show how long each build step took |

Exits with a non zero code if the build fails. See [Working with coding agents](coding-agents.md#json-output) for the JSON shape.

## mira migrate

```bash
mira migrate <source> <dest> [--from <framework>] [--dry-run] [--json]
```

| Option | Default | Meaning |
| --- | --- | --- |
| `--from` | detected | `nextjs`, `astro`, `hugo`, `jekyll`, `docusaurus`, `gatsby`, `eleventy`, `vitepress`, or `markdown` |
| `--dry-run` | off | Report what would move without writing anything |
| `--json` | off | Print the report and build result as JSON |

Moves a site into a new Mira project at `dest`, which must be new or empty. The old project is only read, and nothing in it runs. See [Migrating to Mira](https://mira.omrajguru.site/docs/migrating/).

## mira mcp

```bash
mira mcp [--root <dir>]
```

Runs an MCP server over standard input and output. See [MCP server](mcp.md).

## Terminal output

Mira's terminal output is monochrome, with stippled bars for sizes and timings. Emphasis is plain bold and dim, so it reads on any terminal theme. Styling turns off when output is not a terminal or `NO_COLOR` is set.
