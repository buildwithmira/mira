# Shared elements

Make an element travel between pages with the mira-morph attribute.

A shared element is the same thing shown on two pages, such as a post title on a list and on the post itself. Give both the same `mira-morph` name and it moves from one position to the other during the transition.

```mira
<!-- routes/posts/index.mira -->
{#each collections.posts as post}
  <a href="{{ post.url }}">
    <h2 mira-morph="title-{{ post.slug }}">{{ post.title }}</h2>
  </a>
{/each}
```

```mira
<!-- routes/posts/[slug].mira -->
<h1 mira-morph="title-{{ entry.slug }}">{{ entry.title }}</h1>
```

On this site, each page's heading carries a name like `doc-routing`, so a link with the same name morphs into it.

## What the compiler does

`mira-morph="title-ship-less"` becomes `data-mira-morph="title-ship-less"`, and the page's own stylesheet gets:

```css
[data-mira-morph="title-ship-less"] {
  view-transition-name: title-ship-less;
  view-transition-class: mira-morph;
}
```

Names live in the stylesheet rather than in inline `style` attributes, so pages keep a strict content security policy.

## Rules

- **Unique per page.** A name used twice on one page makes the browser skip the whole transition, so Mira fails the build instead.
- **Valid names.** Characters other than letters, digits, `-`, and `_` become `-`. A name starting with a digit gets an `m-` prefix. `none`, `auto`, `root`, and other CSS keywords are rejected.
- **Stable elements.** An element with the same name and position on both pages, such as a site header, holds still while the rest of the page changes. This site's docs sidebar uses that to stay put.

## Styling the motion

Every shared element carries the view transition class `mira-morph`, so you can tune them together:

```css
::view-transition-group(*.mira-morph) {
  animation-duration: 640ms;
}
```

Or one at a time by name:

```css
::view-transition-group(brand) {
  animation-duration: 0s;
}
```

## Tips

- Give shared elements `width: fit-content` when the text should not stretch across the column during the move.
- Morph the text, not its card. Large boxes changing aspect ratio distort.
- Keep names derived from stable data, such as a slug, rather than a position in a list.
