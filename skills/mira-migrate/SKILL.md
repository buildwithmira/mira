---
name: mira-migrate
description: Move an existing Next.js, Astro, Hugo, Jekyll, Docusaurus, Gatsby, Eleventy, or VitePress site, or a folder of Markdown, to Mira with `mira migrate`, then finish the job by resolving every item it lists for review, rebuilding framework pages as .mira routes, and checking that every old URL still works. Use when asked to migrate, port, move, or convert a site to Mira.
---

# Migrating a site to Mira

`mira migrate` moves content, frontmatter, images, and static files into a new Mira project and writes a redirect for every URL that changes. It reads the old project as text and never runs or changes it. What it cannot convert, it lists in `MIGRATION.md`. Your job is to run it, then resolve that list until the new site builds clean and keeps every URL.

For anything about Mira itself, such as templates, collections, or config, use the `mira` skill. The full migration reference is `references/migrating.md`.

## 1. Preview

```bash
npx -y @miraframework/mira migrate ./old-site ./new-site --dry-run --json
```

Check `report.framework`. If it is wrong, add `--from` with `nextjs`, `astro`, `hugo`, `jekyll`, `docusaurus`, `gatsby`, `eleventy`, `vitepress`, or `markdown`. Check that `pages` and `entries` match what the old site has; if content is missing, the content lives somewhere unusual, so point the command at the folder that holds it.

## 2. Migrate

```bash
npx -y @miraframework/mira migrate ./old-site ./new-site --json
```

The destination must be new or empty, and outside the old project. The JSON has `report` (with `redirects` and `todo`) and `build`. If `build.ok` is false, fix `build.error` first: it names the file and line in the new project.

## 3. Resolve every review item

Each item in `report.todo` and in `new-site/MIGRATION.md` names a file and line in the old project. Where text could not be converted, the new file keeps it in an HTML comment that starts with `mira migrate:`, so search the new project for that string to find each spot.

| Review item | What to do |
| --- | --- |
| `tsx page`, `jsx page`, `astro page`, `vue page`, `njk page`, and similar | Rebuild the page as a `.mira` route at the same URL. Read the old file for its markup and data, then translate it with the table below. Its text was not moved. |
| `MDX component <Name> needs a Mira equivalent` | Replace the comment with plain Markdown or HTML. Images become `<mira-frame>`, notes become callouts such as `> [!NOTE]`, and tabs or accordions become headings and sections. |
| `<Callout>` or similar `was kept as text` | Turn it into a callout: `> [!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]`, or `[!CAUTION]`. |
| `Hugo shortcode` or `Liquid` | Replace the comment with the HTML the shortcode or tag produced. Includes usually belong in a layout. |
| `a remote image became a link` | Download the image into the project, next to the page or under `public/`, and change the link back to an image with alt text. |
| `an image has no alt text` | Describe the image in the alt text. Use `alt=""` only when the image is decoration. |
| `link to x.md does not match a migrated page` | Point the link at the right page URL, or remove it. |
| `its URL collided with another page` | Choose the right URL for each page, then add a redirect for the one that moved. |
| `frontmatter could not be read` | Rewrite the frontmatter as valid YAML at the top of the new file. |

## Translating framework templates to `.mira`

| In the old site | In Mira |
| --- | --- |
| `{title}`, `{{ .Title }}`, `{{ page.title }}` | `{{ page.title }}` |
| `items.map(item => <li>{item.name}</li>)`, `{{ range .Pages }}`, `{% for p in site.posts %}` | `{#each collections.posts as item}<li>{{ item.name }}</li>{/each}` |
| `{cond && <X />}`, `{{ if .Params.x }}`, `{% if x %}` | `{#if path}...{/if}` |
| `{cond ? <A /> : <B />}` | `{#if path}A{:else}B{/if}` |
| A layout component wrapping children | `layouts/default.mira` with `<slot />`, or `layout: name` in a page |
| `getStaticPaths`, `[slug].astro`, a blog template | A collection in `content/<name>/` and `routes/<name>/[slug].mira` |
| Data from imports or props | A file in `data/`, read as `data.<name>` |
| `className="x"` | `class="x"` |
| `<Image>`, `<img>`, `{{< figure >}}` | `<mira-frame src alt caption>` |
| `<Link href>` | `<a href>` |
| Client-side React state or effects | Static HTML. Mira pages ship no app JavaScript, so rebuild the content statically, or leave the feature out and note it for a person. |

`.mira` templates have no expressions, comparisons, or filters: only paths, `{#each}`, `{#if}`, `{#if !path}`, and `{:else}`. Compute values ahead of time in frontmatter or data files.

## 4. Build until clean

```bash
cd new-site
npx mira build --json
```

Fix every error and warning, then build again. When it passes, run `npx mira dev` and compare the main pages with the old site.

## 5. Keep every URL

Next.js, Gatsby, and Astro can set URLs in code, which `mira migrate` does not run. Compare the old site's sitemap with `dist/sitemap.xml`, and add any old URL that is missing to `redirects` in `mira.config.json`:

```json
{ "redirects": { "/2024/05/01/hello.html": "/posts/hello/" } }
```

## 6. Deploy

Set `site.url` and `hosts` in `mira.config.json`, so the build writes the host's config. See the `mira` skill.

## Rules

- Never change the old project. It is the record of what the site was.
- Never delete a review item without resolving it. Each one is content or behavior a reader would otherwise lose.
- Keep every URL that existed. Either the page is at the same URL, or a redirect points to its new one.
