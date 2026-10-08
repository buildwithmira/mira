# Security

The policies and defaults that make a Mira site safe without configuration.

A Mira site is static HTML, which removes most of the attack surface. Mira locks down the rest by default.

## Content security policy

Every page carries a policy in a meta tag, generated from the page itself:

```text
default-src 'self';
script-src 'self' 'sha256-…' 'sha256-…';
style-src 'self' 'sha256-…';
img-src 'self' data: https:;
object-src 'none';
base-uri 'self';
form-action 'self'
```

The hashes cover the page's inline runtime, its prefetch rules, and its inlined stylesheet, and nothing else. An injected script or style has no matching hash and does not run. Shared element names are compiled into the stylesheet instead of `style` attributes, so the policy never needs `unsafe-inline`.

Scripts and styles from your own origin are allowed, so a file you put in `public/` and link works without changes.

## Security headers

Mira writes a `_headers` file, read by Netlify and Cloudflare Pages, with the headers a meta tag cannot set:

| Header | Value |
| --- | --- |
| `Content-Security-Policy` | `frame-ancestors 'none'; object-src 'none'; base-uri 'self'` |
| `X-Frame-Options` | `DENY` |
| `X-Content-Type-Options` | `nosniff` |
| `Referrer-Policy` | `strict-origin-when-cross-origin` |
| `Cross-Origin-Opener-Policy` | `same-origin` |
| `Cross-Origin-Resource-Policy` | `same-origin` |
| `Permissions-Policy` | Camera, microphone, geolocation, payment, and USB off |
| `Strict-Transport-Security` | Two years, including subdomains |

Twins are served as `text/markdown`. Turn HSTS off with `"headers": { "hsts": false }` until HTTPS is permanent on your domain, or stop writing the file with `"emit": false`. For other hosts, copy these headers into the host's own config.

## Templates escape by default

`{{ value }}` escapes `<`, `>`, `&`, and quotes. Raw HTML needs `{{ unsafe value }}`, and every use is reported as a build warning with its file and line. JSON written into pages escapes `</`, so data cannot close a script element.

## Prefetching stays safe

Prefetching only fetches same origin links with GET. Mark links that change state or depend on a session with `data-mira-no-prefetch`.

## A careful build

- An output directory is only cleared when it carries Mira's `.mira-output` marker, so `--out` pointed at the wrong folder fails instead of deleting it.
- Theme values and font settings are validated so config cannot inject CSS or HTML.
- The dev server listens on `127.0.0.1` only and rejects paths that try to leave the output folder.
