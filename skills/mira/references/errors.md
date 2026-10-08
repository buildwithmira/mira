# Checks and errors

What the build checks, how errors read, and the dev overlay.

Mira checks a site while it builds and stops on anything that would break for a reader. Errors name the file and line and say how to fix the problem.

## What fails the build

- Invalid frontmatter, JSON, or YAML
- Frontmatter that does not match its collection's schema
- Template syntax errors, such as an unclosed `{#each}`
- A missing layout, or a dynamic route without a collection
- Two files producing the same URL
- An internal link or asset that does not exist
- A shared element name used twice on one page
- A page over its size budget
- A missing font file, or an invalid config value

## What warns

Warnings print after the summary and do not stop the build:

- A link to an anchor that does not exist on its target page
- An `<img>` without `alt` text; use `alt=""` for decorative images
- More than one `<h1>` on a page
- Each `{{ unsafe … }}` in a template
- A generated file skipped because `public/` has one with the same name
- Feeds, sitemap, and canonical URLs skipped because `site.url` is not set

## Reading an error

```text
✕ /about/ links to /guide/, which does not exist

  routes/about.md:7
    5 # About
    6
  › 7 See [the guide](/guide/).

  hint  fix the path, add the page under routes/, or add the file under public/
```

The first line is the problem. Below it are the file and line, the surrounding source with the failing line marked `›`, any underlying causes, and a hint.

## The dev overlay

In `mira dev`, a failed rebuild opens an overlay on the page you are viewing with the same message, location, source, and hint. It also catches errors thrown by scripts in the page.

- **Copy** puts a plain text report on the clipboard, ready for an issue or a chat with an agent.
- **Esc** collapses it to a small badge; click the badge to reopen it.
- With more than one error, step through them with the arrows.

Fix the file and save. The overlay closes and the page reloads once the build succeeds. If the very first build fails, the dev server serves a holding page that shows the overlay and reloads on its own.

## In scripts

`mira build` exits with a non zero code on failure. Add `--json` to get the error as structured data. See [Working with coding agents](coding-agents.md#errors-an-agent-can-act-on).
