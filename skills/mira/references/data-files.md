# Data files

Keep navigation and other structured data in JSON or YAML and read it from any template.

Files in `data/` are loaded at build time and exposed to every template as `data.<name>`, where the name is the file name without its extension. Dashes become underscores, so `docs-nav.json` is `data.docs_nav`.

| File | Template name |
| --- | --- |
| `data/docs.json` | `data.docs` |
| `data/team.yaml` | `data.team` |
| `data/docs-nav.yml` | `data.docs_nav` |

## Example: docs navigation

This site's sidebar is one JSON file:

```json
[
  {
    "title": "Getting started",
    "items": [
      { "title": "Introduction", "url": "/docs/introduction/" },
      { "title": "Installation", "url": "/docs/installation/" }
    ]
  }
]
```

And one loop in the docs route:

```mira
{#each data.docs as section}
  <p>{{ section.title }}</p>
  <ul>
    {#each section.items as item}
    <li><a href="{{ item.url }}" mira-nav>{{ item.title }}</a></li>
    {/each}
  </ul>
{/each}
```

Every URL in the file is checked at build time like any other link, so a renamed page cannot leave a dead sidebar entry.

## Errors

Invalid JSON or YAML fails the build with the file and line.
