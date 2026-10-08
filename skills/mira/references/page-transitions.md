# Page transitions

Native, cross document transitions with route pairs, direction, and accessible defaults.

Moving between pages animates by default. Mira uses the browser's cross document view transitions, so each page stays plain HTML and the animation costs no framework code.

## How it works

Every page opts in to view transitions. When the next page arrives, the runtime, about 1 KB gzipped, picks a transition name and a direction and hands them to the browser, which animates from a snapshot of the old page to the new one.

Browsers without cross document view transitions navigate normally. Nothing breaks; the motion is an enhancement.

## Choosing a transition

Mira picks the first match:

1. A route pair in config that matches the navigation
2. The destination page's `transition` frontmatter
3. `transitions.default` in config, which is `fade` unless you change it

```json
{
  "transitions": {
    "default": "fade",
    "pairs": {
      "/posts/ -> /posts/*": "slide-up"
    }
  }
}
```

```markdown
---
title: Changelog
transition: slide
---
```

## Route pairs

A pair key is `"<from> -> <to>"`. Each side is an exact path such as `/posts/`, or a prefix ending in `*` such as `/posts/*`, which matches any path under it but not the prefix itself.

Navigating a pair in the opposite direction plays the transition backwards. With the pair above, opening a post slides up, and returning to the list, by link or by the back button, slides down.

## Built in transitions

| Name | Forward | Back |
| --- | --- | --- |
| `fade` | Crossfade | Crossfade |
| `slide-up` | New page rises in | Old page sinks out |
| `slide` | Pushes in from the right | Pulls in from the left |
| `none` | No animation | No animation |

## Direction

The runtime sets a second transition type, `forward` or `back`. Back is a history traversal to an earlier entry, or a pair navigated in reverse. Use both types in your own CSS.

## Custom transitions

A transition name is a view transition type, so you define your own in CSS:

```css
html:active-view-transition-type(zoom)::view-transition-new(root) {
  animation: 420ms var(--mira-spring) both zoom-in;
}
html:active-view-transition-type(zoom):active-view-transition-type(back)::view-transition-new(root) {
  animation-name: zoom-out;
}
@keyframes zoom-in { from { opacity: 0; transform: scale(0.96); } }
@keyframes zoom-out { from { opacity: 0; transform: scale(1.04); } }
```

Then use `transition: zoom` in frontmatter or a pair.

## Timing

Shared elements move with `--mira-spring` over 520ms. The page itself fades with `--mira-ease-out`. Change `--motion-morph` and `--motion-fast` in your theme to retune everything at once. See [Theming](theming.md#change-tokens).

## Accessibility

- **Reduced motion.** When the reader asks for reduced motion, shared elements jump into place and the page crossfades over 120ms.
- **Focus.** After a link navigation, focus moves to the new page's main heading without scrolling, so keyboard and screen reader users start at the new content. Back navigations and links to a `#hash` keep the browser's behavior.
- **Agents and automation.** Transitions are skipped when the browser reports automation or a crawler, so the DOM is stable for agents.

## Prefetching

Transitions only look good when the next page is ready. Mira prefetches links on hover and prerenders on press with Speculation Rules, and falls back to a prefetch on hover, touch, or focus in other browsers. See [Performance](performance.md#prefetching).
