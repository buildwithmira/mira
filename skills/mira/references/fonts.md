# Fonts

Serve font files from your own site with preloading and swap, so pages make no requests to font services.

Put font files in `public/` and list them in config. Mira writes the `@font-face` rules into each page's CSS and preloads the faces you mark.

```json
{
  "fonts": [
    {
      "family": "Inter",
      "src": "/fonts/Inter.woff2",
      "weight": "100 900",
      "preload": true
    },
    {
      "family": "JetBrains Mono",
      "src": "/fonts/JetBrainsMono.woff2",
      "weight": "100 800"
    }
  ],
  "theme": {
    "font": {
      "text": "Inter, system-ui, sans-serif",
      "display": "Inter, system-ui, sans-serif",
      "mono": "\"JetBrains Mono\", ui-monospace, monospace"
    }
  }
}
```

The `theme.font` keys set the `--font-text`, `--font-display`, and `--font-mono` tokens, so the new families apply everywhere. See [Theming](theming.md).

| Key | Default | Meaning |
| --- | --- | --- |
| `family` | required | The family name used in CSS |
| `src` | required | Path under `public/`, starting with `/` |
| `weight` | `400` | One weight, or a range such as `100 900` for variable fonts |
| `style` | `normal` | `normal` or `italic` |
| `preload` | `false` | Preload the file. Use it for one or two faces above the fold. |

Every face uses `font-display: swap`, so text shows at once in a fallback font and swaps when the file arrives. A missing file fails the build.

## Why self host

A font from another origin is an extra connection before text can render, and a third party that sees every visit. Fonts from your own site stay inside the page's content security policy, cache with the site, and send no data elsewhere.
