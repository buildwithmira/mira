# MDX

Write Markdown with components. Mira renders MDX at build time with your own .mira components, and no JavaScript runs.

An `.mdx` file works anywhere a Markdown file does: as a page in `routes/` or as an entry in `content/<name>/`. Inside it, a capitalized tag renders a component.

```mdx
---
title: Opening hours
---

import Callout from "../components/Callout.mira"

# Opening hours

<Callout tone="warn" title="Closed Mondays">
  We open again on **Tuesday** at 9.
</Callout>

Coffee is served all day. <Badge label="New" /> Pastries from 8.
```

## Components

A component is a `.mira` file in `components/`, named after the tag: `<Callout>` renders `components/Callout.mira`.

```html
<template>
<aside class="callout" data-tone="{{ props.tone }}">
  <strong>{{ props.title }}</strong>
  <slot />
</aside>
</template>

<style>
.callout { padding: var(--space-4); border-radius: var(--radius-md); background: var(--surface-1); }
</style>
```

- Props are read as `{{ props.name }}`, and work with `{#if props.name}` and `{#each props.items as item}`. See [Templates](templates.md).
- `<slot />` is where the tag's children go. Children are Markdown, and can hold other components.
- A component's `<style>` is added to every page that uses it, once.

Imports are not needed, since components are found by name, and `import` lines are dropped.

## Props

| Written as | Value |
| --- | --- |
| `title="Closed"` or `title='Closed'` | Text |
| `count={3}` | A number |
| `open={true}`, or just `open` | `true` |
| `items={["a", "b"]}` | A list, written as JSON |
| `link={{"href": "/x", "label": "More"}}` | An object, written as JSON |

## What does not run

MDX normally runs JavaScript. Mira renders everything at build time without it, so code that would need to run is a build error with its line:

| In the file | What happens |
| --- | --- |
| `export const meta = {...}` | Error: put values in the frontmatter |
| `{price * 2}` or `{/* note */}` | Expressions are errors; comments are dropped |
| `<Badge label={user.name} />` | Error: pass text, numbers, or JSON |
| `<Chart />` with no `components/Chart.mira` | Error, listing the components that exist |

Write `\{` for a literal brace. Code in backticks and fenced code blocks is left exactly as written.

## For agents

The Markdown copy of an MDX page has each component replaced with Markdown of what it rendered, so agents read the content, not the tags.
