# Project structure

The folders and files of a Mira project and what each one does.

A Mira project is a folder with a config file and a few conventional directories. Only `routes/` is required.

```text
my-site/
├── mira.config.json     Site, theme, transitions, schemas, and checks
├── AGENTS.md            Conventions for coding agents
├── routes/              Pages; every file is a URL
│   ├── index.mira
│   ├── about.md
│   └── posts/
│       ├── index.mira
│       └── [slug].mira  One page per entry in content/posts/
├── layouts/
│   └── default.mira     Wraps every page unless a page opts out
├── content/
│   └── posts/           A collection: Markdown files with frontmatter
├── data/                JSON or YAML available to every template
├── public/              Copied to the output as is
└── dist/                Build output; safe to delete
```

## Folders

| Folder | Purpose | Docs |
| --- | --- | --- |
| `routes/` | `.md` and `.mira` files that become pages | [Routing](routing.md) |
| `layouts/` | `.mira` components that wrap pages | [Layouts](layouts.md) |
| `content/` | One folder per collection of Markdown entries | [Content collections](collections.md) |
| `data/` | Structured data such as navigation | [Data files](data-files.md) |
| `public/` | Static files served from the site root | below |

## Public files

Everything in `public/` is copied to the output with the same path: `public/favicon.svg` is served at `/favicon.svg`. If `public/favicon.svg` exists, every page links it as the icon.

A file in `public/` wins over a generated file with the same name. Put your own `robots.txt` there and Mira skips its generated one, with a warning so the choice stays visible.

## Generated folders

- `dist/` holds the production build. Mira writes a `.mira-output` marker into it and only ever clears a directory that carries the marker, so a mistyped `--out` cannot delete your files.
- `.mira/` holds the dev server's and the MCP server's builds.

Add both to `.gitignore`; the starter does.
