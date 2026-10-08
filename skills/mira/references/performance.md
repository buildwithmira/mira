# Performance

What Mira does to make pages load first and stay small, and the budgets that keep them that way.

Mira's job is to send less. A content page is one HTML file with everything it needs to render inside it.

## What a page contains

- **HTML rendered at build time.** No component code runs in the browser and nothing hydrates.
- **Inlined CSS.** The base styles, the theme, the layout's and page's styles, and only the component CSS the page uses, in one `<style>` element. There is no stylesheet request to wait for.
- **A runtime of about 1 KB.** Gzipped, inline in the head. It sets the transition and its direction, moves focus after navigation, and prefetches in browsers without Speculation Rules.
- **Prefetch rules.** A small Speculation Rules block for the next page.

## Component CSS on demand

Each component's CSS is tagged with its class. While writing a page, the compiler checks the HTML and inlines only the blocks whose class appears, so a page without buttons carries no button CSS.

## Prefetching

```json
{ "prefetch": { "eagerness": "moderate", "prerender": true } }
```

| Setting | Effect |
| --- | --- |
| `eagerness: "conservative"` | Prefetch when a link is pressed |
| `eagerness: "moderate"` | Prefetch when a pointer rests on a link, the default |
| `eagerness: "eager"` | Prefetch as early as the browser allows |
| `prerender: true` | Also prerender on press, so the page is fully rendered before it is shown |

Only same origin links are fetched. Opt a link, or every link inside an element, out with `data-mira-no-prefetch`; use it for links that change state or need a signed in session. Links with `download` are skipped. Browsers without Speculation Rules get a prefetch on hover, touch, or focus instead, which is skipped when the reader has Save-Data on.

## Images

Every `<img>` that points at a file in `public/` gets its intrinsic `width` and `height` written in, so it reserves its space and never shifts the layout. Images also get `loading="lazy"` and `decoding="async"` unless you set them. Set `loading="eager"` on the image above the fold.

## Budgets

```json
{ "budgets": { "page_kb": 14 } }
```

`page_kb` is the largest a page may be, gzipped. A page over budget fails the build with its size. 14 KB fits in the first round trip of a new connection, which is why the starter uses it.

The build summary draws each page against the budget:

```text
pages
  / ·································  ⣿⣿⣿⣿⣿⠀⠀⠀⠀⠀   6.9 KB  1 morph
  /posts/ ···························  ⣿⣿⣿⡏⠀⠀⠀⠀⠀⠀   5.0 KB  3 morphs
```

## Build speed

The compiler is a native Rust binary that renders pages in parallel. To see where a build spends its time:

```bash
mira build --timings
```

```text
timings
  load ·······  ⣿⣿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀    2.5 ms
  render ·····  ⣿⠃⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀    1.5 ms
  check ······  ⡿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀    1.1 ms
  write ······  ⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⠁⠀⠀⠀⠀     13 ms  slowest
```

The same numbers are in `mira build --json` under `timings`.
