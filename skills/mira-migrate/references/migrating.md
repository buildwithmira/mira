# Migrating to Mira

Move a Next.js, Astro, Hugo, Jekyll, Docusaurus, Gatsby, Eleventy, or VitePress site to Mira with one command, keeping every URL.

`mira migrate` moves a content site from another framework into a new Mira project. It brings over pages, posts, frontmatter, images, and static files. It writes a redirect for every URL that changes, then builds the new project to prove it works.

```bash
mira migrate ./old-site ./new-site
```

The framework is detected from `package.json` or the project's files. To name it yourself, pass `--from`:

```bash
mira migrate ./old-site ./new-site --from hugo
```

## Safe by design

- **Nothing in the old project runs.** Config files, components, and templates are read as text. No JavaScript, Go templates, or Liquid execute.
- **The old project is never changed.** It is only read. The destination must be a new or empty folder outside the old project.
- **No network.** Remote images are not downloaded. Each one becomes a link and is listed for review.
- **Files never leave the project.** A relative image is followed only to a regular file inside the old project, and symlinks are skipped.
- **Nothing is lost silently.** Anything that cannot be converted is kept in a `<!-- mira migrate: … -->` comment and listed, with its file and line, in `MIGRATION.md`.

## What it moves

| From the old site | In the Mira project |
| --- | --- |
| Markdown and MDX pages | `routes/<path>/index.md`, at the same URL |
| Posts in `posts/`, `_posts/`, `blog/`, `articles/`, `news/`, `notes/`, `writing/`, `changelog/`, `journal/` | A [collection](https://mira.omrajguru.site/docs/collections/) in `content/<name>/`, with `routes/<name>/[slug].mira` and an index page |
| YAML (`---`) or TOML (`+++`) frontmatter | YAML frontmatter, with common names mapped (below) |
| Images referenced by relative path, wherever they live | Copied next to the new file, as [media frames](https://mira.omrajguru.site/docs/media/) |
| `public/`, `static/`, `assets/` | `public/` |
| Links to `.md` files | Links to the new page URLs |
| A 404 page | `routes/404.md` |
| Site title, description, and URL | `site` in `mira.config.json` |

Collection schemas are inferred from the entries, with `strict: false` so fields Mira does not know are kept. Fields with mixed types across entries are left out of the schema rather than guessed.

### Frontmatter names

| Mira | Also read from |
| --- | --- |
| `description` | `summary`, `excerpt`, `subtitle`, `abstract` |
| `date` | `pubDate`, `publishDate`, `published_at`, `publishedAt`, `created`, or a `2024-05-01-` file name prefix |
| `updated` | `lastmod`, `updatedDate`, `updated_at`, `modified`, `last_update` |
| `image` | `cover`, `coverImage`, `heroImage`, `hero`, `thumbnail`, `featured_image`, `ogImage` |
| `tags` | `keywords`, `categories` |
| `author` | `authors` (the first), or the `name` of an author object |
| `draft` | `published: false` |

Dates in any common form (`2024-05-01T10:00Z`, `2024/05/01`, `May 1, 2024`, `Jul 08 2022`) become `2024-05-01`. A date that cannot be read is kept as written, never cut short, and the build points at it.

## Per framework

**Next.js.** Reads `content/`, `posts/`, `_posts/`, `blog/`, and `data/blog/`, plus Markdown and MDX under `pages/` and `app/`. `<Image>` becomes a media frame. MDX `import` and `export` lines are dropped. Other components are kept as comments and listed. Pages written as `page.tsx` or in `pages/` are listed to rebuild as `.mira` routes.

**Astro.** Reads `src/content/` and Markdown under `src/pages/`. A `heroImage` in `src/assets/` is copied to `public/images/` so it can be the social image. `.astro` pages are listed to rebuild.

**Hugo.** Reads `content/`, with TOML or YAML frontmatter. `_index.md` becomes the section's index page. `{{< figure >}}` becomes a media frame. Other shortcodes are kept as comments and listed. `static/` becomes `public/`.

**Jekyll.** Reads `_posts/` into a `posts` collection and Markdown pages from the project. Old post URLs follow the site's `permalink` setting (`date`, `pretty`, `ordinal`, `none`, or a pattern), and each one redirects to `/posts/<slug>/`. `.html` pages redirect to clean URLs. `{% highlight %}` becomes a fenced code block. Other Liquid is kept as a comment and listed.

**Docusaurus.** Reads `docs/` (at `/docs/…`) and `blog/`. Dated blog URLs like `/blog/2021/08/26/welcome` redirect to `/blog/welcome/`. `:::note`, `:::tip`, `:::info`, `:::warning`, and `:::danger` become callouts, titles included.

**Gatsby.** Reads `content/` and Markdown under `src/pages/`. A `post/index.md` takes its slug from the folder.

**Eleventy.** Reads the `input` folder named in the Eleventy config, or the project root. `.njk`, `.liquid`, and `.html` templates are listed to rebuild. Generated files like `sitemap.xml.njk` and feeds are skipped, because Mira writes its own.

**VitePress.** Reads `docs/`, or the project root. `docs/public/` becomes `public/`. `::: tip` containers become callouts. `.html` URLs redirect to clean URLs.

**A Markdown folder.** Any folder of `.md` files, with `--from markdown`.

## After migrating

```bash
cd new-site
mira dev
```

1. Read `MIGRATION.md`. It lists every item to review, with its file and line, and every redirect.
2. Rebuild the listed pages as `.mira` routes. The home page and post layout are generated as a starting point.
3. Compare the old site's `sitemap.xml` with `dist/sitemap.xml`. Next.js, Gatsby, and Astro can set URLs in code, which `mira migrate` does not run. Add any missing URLs under [`redirects`](https://mira.omrajguru.site/docs/configuration/#redirects).
4. Add [`hosts`](https://mira.omrajguru.site/docs/deploying/#hosts) for where the site will live, and deploy.

## For agents

`--json` prints the report and the build result to standard output. `--dry-run` reports what would move without writing anything.

```bash
mira migrate ./old-site ./new-site --json
```

```json
{
  "ok": true,
  "schema": 1,
  "dry_run": false,
  "report": {
    "framework": "jekyll",
    "pages": 4,
    "entries": { "posts": 12 },
    "assets": 30,
    "redirects": { "/2024/05/01/hello.html": "/posts/hello/" },
    "todo": [
      "_posts/2024-05-01-hello.md:9: Liquid `{% include note.html %}` is not rendered by Mira"
    ]
  },
  "build": { "ok": true, "pages": 19, "warnings": [] }
}
```

When the build fails, `build.error` has the same shape as [`mira build --json`](https://mira.omrajguru.site/docs/coding-agents/#json-output) errors, so an agent can fix the file and run `mira build` again.
