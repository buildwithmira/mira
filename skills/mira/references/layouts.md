# Layouts

Wrap pages in shared chrome, choose a layout per page, or opt out.

A layout is a `.mira` file in `layouts/` that wraps pages. Its `<slot />` is where the page goes.

```mira
<template>
<header>
  <a href="/">{{ site.title }}</a>
  <nav>
    <a href="/" mira-nav>Home</a>
    <a href="/posts/" mira-nav>Writing</a>
  </nav>
</header>
<main id="main">
  <slot />
</main>
</template>

<style>
body { max-width: 46rem; margin-inline: auto; }
</style>
```

## Choosing a layout

| Frontmatter | Result |
| --- | --- |
| none | `layouts/default.mira` if it exists, otherwise no layout |
| `layout: docs` | `layouts/docs.mira`; the build fails if it does not exist |
| `layout: false` | No layout; the page renders on its own |

A layout sees the same `site`, `page`, `entry`, `collections`, and `data` as the page it wraps.

## The document shell

Layouts render inside `<body>`. Mira writes everything around it: the doctype, `<html lang>`, the content security policy, title, description, canonical URL, Open Graph tags, the inlined CSS, the runtime, and the prefetch rules. You never write a `<head>`.

## Give the page a main landmark

Put the page in `<main id="main">`. Mira then holds the first paint until `main` is parsed, so shared elements exist when an incoming transition takes its snapshot, and moves keyboard focus to the main heading after navigation. A skip link to `#main` is good practice:

```mira
<a class="skip" href="#main">Skip to content</a>
```

## Navigation state

Add `mira-nav` to navigation links. The compiler sets `aria-current="page"` on the link whose `href` matches the page, or whose `href` is a section the page is in, so `/posts/` stays current on every post. Style it with `[aria-current="page"]`; the `.mira-tabs` component already does.
