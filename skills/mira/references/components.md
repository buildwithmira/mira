# Components

The built in buttons, tabs, cards, code blocks, and search box, with their markup. Each page ships only the CSS it uses.

Components are plain HTML with a class. The compiler checks each page's HTML and inlines CSS only for the components that page uses.

## Button

```html
<a class="mira-btn" href="/start/">Start building</a>
<a class="mira-btn" data-variant="outline" href="/docs/">Read the docs</a>
<button class="mira-btn" data-size="sm">Copy</button>
```

- **Solid**, the default, is the main action on a surface.
- **Outline** is for secondary actions.
- `data-size="sm"` makes a compact 32px button.

## Tabs

Pill navigation. Add `mira-nav` to each link and the current page is marked with `aria-current`.

```html
<nav class="mira-tabs" aria-label="Primary">
  <a href="/" mira-nav>Home</a>
  <a href="/docs/" mira-nav>Docs</a>
</nav>
```

## Card

```html
<a class="mira-card" href="/docs/routing/">
  <h2>Routing</h2>
  <p>How files become URLs.</p>
</a>
```

A link card lifts 2px on hover.

## Code

Fenced code blocks in Markdown render with syntax highlighting. In a `.mira` template, use `<mira-code lang="rust">`. See [Markdown](markdown.md).

## Search

`<mira-search>` renders a search box over the site's index. See [Search](search.md).
