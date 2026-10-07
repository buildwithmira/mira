# @buildwithmira/mira

The `mira` command line tool for [Mira](https://github.com/buildwithmira/mira), a static site framework for content sites.

```bash
npm install --save-dev @buildwithmira/mira
npx mira dev
```

To start a new site, run `npm create mira@latest my-site`.

This package contains a small launcher. The compiled binary comes from a platform package, such as `@buildwithmira/mira-linux-x64`, which npm installs automatically for Linux (x64, arm64), macOS (arm64, x64), and Windows (x64, arm64).

| Command | What it does |
| --- | --- |
| `mira new <dir>` | Create a site from the starter template |
| `mira dev` | Serve the site on `localhost:4321`, rebuilding and reloading on change |
| `mira build` | Build the site to `dist/` |
| `mira migrate <from> <to>` | Move an existing site into a new Mira project |
| `mira mcp` | Serve the site's content over MCP on standard input and output |

Licensed under MIT or Apache-2.0, at your option.
