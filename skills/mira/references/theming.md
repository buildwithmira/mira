# Theming

Pick light or dark, change colors, type, spacing, and motion from mira.config.json, and style pages without fighting the defaults.

Every Mira page starts with a theme written as CSS custom properties. Change any of them from config, or use them in your own CSS.

## Light or dark

```json
{ "scheme": "dark" }
```

| Value | Result |
| --- | --- |
| `dark` | Dark theme, the default |
| `light` | Light theme |
| `system` | Follows the reader's setting |

Color tokens use `light-dark()`, so the same names work in both themes.

## Change tokens

Set values under `theme`. Nested keys join with dashes into the property name:

```json
{
  "theme": {
    "ink": "#f4f1ea",
    "radius": { "md": "10px" },
    "font": { "display": "Inter, system-ui, sans-serif" },
    "motion": { "morph": "640ms" }
  }
}
```

That sets `--ink`, `--radius-md`, `--font-display`, and `--motion-morph` above the defaults. Values cannot contain `{`, `}`, `;`, or `<`.

## Tokens

| Token | Use |
| --- | --- |
| `--surface-0`, `--surface-1`, `--surface-2` | Page ground, raised panels, and wells such as code blocks |
| `--ink`, `--ink-muted`, `--ink-faint` | Text, from primary to captions |
| `--line`, `--line-strong` | Dividers and control borders |
| `--focus` | Focus rings |
| `--font-text`, `--font-display`, `--font-mono` | Body, headings, and code |
| `--text-sm`, `--text-base`, `--text-lg` | Text sizes |
| `--display-sm` to `--display-xl` | Fluid heading sizes |
| `--space-1`, `-2`, `-3`, `-4`, `-6`, `-8`, `-12`, `-16`, `-24` | Spacing: the step times 4px, so `--space-6` is 24px |
| `--radius-sm`, `--radius-md`, `--radius-lg`, `--radius-xl` | Corner radii |
| `--motion-fast`, `--motion-control`, `--motion-morph` | Durations for crossfades, controls, and shared elements |

## Your CSS wins

Mira's CSS sits in ordered cascade layers, so styles in your layouts and pages override it without `!important`:

```css
@layer mira.reset, mira.tokens, mira.theme, mira.base, mira.prose,
       mira.components, mira.transitions, layout, page;
```

A layout's `<style>` lands in `layout`, and a page's in `page`.
