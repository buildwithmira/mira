# Working on this Mira site

This is a Mira project. Conventions:

- `routes/` holds pages. `routes/about.md` serves `/about/`, `routes/index.mira`
  serves `/`. A `.mira` file has optional `---` frontmatter, a `<template>`
  block, and an optional `<style>` block. Markdown pages are wrapped in
  `.prose` automatically.
- `routes/posts/[slug].mira` renders one page per entry in `content/posts/`.
  Inside it, `<slot />` is the entry's rendered Markdown.
- Each post's frontmatter is checked against `collections.posts.fields` in
  `mira.config.json`. Add a field there before using it in a post.
- `layouts/default.mira` wraps every page. `<slot />` is the page. Set
  `layout: false` in frontmatter to skip it, or `layout: name` to pick another.
- Template syntax: `{{ page.title }}` (escaped), `{{ unsafe path }}` (raw HTML,
  avoid), `{#each collections.posts as post}...{/each}`,
  `{#if path}...{:else}...{/if}`.
- Shared element transitions: put `mira-morph="name"` on matching elements on
  two pages. Names must be unique within a page.
- Transitions: `transitions.default` and `transitions.pairs` in
  `mira.config.json`, or `transition:` in page frontmatter. Built in names:
  `fade`, `slide-up`, `slide`, `none`.
- Navigation: add `mira-nav` to links and the compiler sets
  `aria-current="page"` on the one matching the current page.
- Images and video: use `<mira-frame src="./photo.png" alt="…" caption="…">`
  (add `zoom` to open full size). Markdown images become frames. Every
  frame needs `alt`; use `alt=""` for decoration. Videos also need `width`
  and `height`. Files ship under `/media/` as AVIF plus the original.
- SEO: every page should have a `title` and a 50 to 160 character
  `description`. Frontmatter also takes `robots: noindex`, `canonical`,
  `updated`, `image`, `image_alt`, `author`, and `faq` (a list of `q` and
  `a`). The build warns on missing, duplicate, or badly sized titles and
  descriptions. `agents.search`, `agents.answers`, and `agents.training`
  in `mira.config.json` set the crawler policy.
- Pixel module: `<mira-mark />` draws the 5 by 5 pixel M and
  `<mira-pixels text="404" />` sets pixel text, both in `currentColor`. Add
  `dither` to show unlit pixels faintly.

## Design system

Monochrome core for structure and text; the secondary palette for
atmosphere. Night is the primary theme.

- Surfaces and ink: `--surface-0` page, `--surface-1` panels, `--surface-2`
  wells and code, `--line`, `--line-strong`, `--ink`, `--ink-muted`,
  `--ink-faint`, `--fill-ink` with `--on-fill-ink`, `--focus`.
- Secondary palette: `--ember`, `--sun`, `--peach`, `--rose`, `--cobalt`,
  `--ocean`, `--navy`, `--haze`, `--lilac`, `--lagoon`, `--lime`. Text on warm
  hues uses `--on-warm`, on cobalt, ocean, and navy `--on-cool`.
- Type: `--font-display` (Bricolage Grotesque), `--font-text` (Geist),
  `--font-mono` (Geist Mono, for uppercase labels), `--font-pixel`
  (Pixelify Sans).
- Space `--space-1` to `--space-24` in 4px steps; radii `--radius-px`, `-sm`,
  `-md`, `-lg`, `-xl`, `-pill`; motion `--mira-spring`, `--mira-ease-out`.
- Components (CSS ships only on pages that use them): `.mira-btn` with
  `data-variant="solid|outline|accent"` and `data-size="sm"`;
  `.mira-eyebrow`; `.mira-hl` with four `<i></i>` handles; `.mira-atmo` with
  `data-palette="sunset|sky|cobalt|meadow|night"` and a `.mira-atmo__field`
  child; `.mira-glass` with `data-shape`; `.mira-cursor`; `.mira-tabs`;
  `.mira-card`.

Commands: `mira dev` serves with live reload, `mira build` writes `dist/`,
and `mira build --json` prints a machine readable report.
