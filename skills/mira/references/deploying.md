# Deploying

Publish dist to Vercel, Netlify, Cloudflare, GitHub Pages, Firebase, Render, Azure, AWS, Google Cloud, Docker, or Deno, with each host's config written for you.

`mira build` writes a complete static site to `dist/`. Deploying is publishing that folder. Name your hosts in `mira.config.json`, and Mira writes each host's own config file on every build, so headers, clean URLs, the 404 page, redirects, and Markdown twins behave the same everywhere.

```json
{
  "site": { "url": "https://example.com" },
  "hosts": { "vercel": {}, "netlify": {} }
}
```

## What is in dist

| Path | Contents |
| --- | --- |
| `index.html`, `*/index.html` | Pages |
| `404.html` | The not found page |
| `*.md` | Markdown twins |
| `llms.txt`, `llms-full.txt` | Agent indexes |
| `sitemap.xml`, `*/rss.xml` | When `site.url` is set |
| `robots.txt` | Crawler policy |
| `media/`, `media.json` | [Media frames](media.md) and their manifest |
| `_headers`, `_redirects` | For Netlify and Cloudflare Pages |
| `vercel.json` | For Vercel, when `dist/` is the project root |
| `_mira/` | The search index, and scripts a page uses |
| everything from `public/` | As is |

## Before you deploy

Set `site.url` to your production origin so canonical URLs, the sitemap, and feeds use absolute links.

## Hosts

Each key under `hosts` writes that host's config. Files go in the project root unless noted. Mira only replaces files it wrote itself: if a config file you wrote by hand is in the way, the build stops and says so, and your file is left alone.

| Host | Key | Mira writes | Set up |
| --- | --- | --- | --- |
| Vercel | `vercel` | `vercel.json` | Import the repository. No build command is needed. |
| Netlify | `netlify` | `netlify.toml`, `dist/_redirects` | Import the repository. |
| Cloudflare Pages | `cloudflare` | `wrangler.toml`, `dist/_redirects` | Connect the repository. Set `hosts.cloudflare.project` to the Pages project name. |
| GitHub Pages | `github` | `.github/workflows/mira-pages.yml` | In the repository settings, set Pages to deploy from GitHub Actions. |
| Firebase Hosting | `firebase` | `firebase.json` | `firebase deploy --only hosting` |
| Render | `render` | `render.yaml` | Create a Blueprint from the repository. `hosts.render.service` names the service. |
| Azure Static Web Apps | `azure` | `dist/staticwebapp.config.json` | Point the app at `dist`. |
| Docker | `docker` | `Dockerfile`, `nginx.conf` | `docker build -t site .` then `docker run -p 8080:8080 site` |
| Deno Deploy | `deno` | `main.ts` | Deploy `main.ts` as the entry point. |
| S3 and CloudFront | `s3` | `cloudfront-function.js` | Upload `dist/` to the bucket and attach the function to the distribution's viewer requests. |
| AWS Amplify Hosting | `amplify` | `amplify.yml`, `customHttp.yml` | Connect the repository. Amplify publishes the committed `dist/`. |

Container platforms run the `docker` image as it is: Google Cloud Run, AWS App Runner, Azure Container Apps, Fly.io, and Railway. Google Cloud's static hosting is Firebase Hosting, and Azure's is Static Web Apps, both above.

Every host gets the same behavior:

- the [security headers](security.md#security-headers), with immutable caching for `/fonts/` and `/media/`
- URLs that end in a slash, with a permanent redirect from the version without one
- the site's own `404.html`, with status 404
- `.md` twins served as `text/markdown`, and, on hosts that support it, the twin for requests with `Accept: text/markdown`
- every entry in [`redirects`](#redirects), as permanent redirects in the host's own format

Vercel is verified live, Markdown negotiation included. GitHub Pages is verified too; it cannot set headers, so pages rely on their CSP meta tag there.

## Redirects

When a page moves, map the old path to the new one. [`mira migrate`](https://miraweb.in/docs/migrating/) fills this in for you.

```json
{
  "redirects": {
    "/2024/05/01/hello.html": "/posts/hello/",
    "/old-docs/": "https://docs.example.com/"
  }
}
```

Mira writes each redirect into every host's config. It also writes a small redirect page at the old path, with a canonical link, so hosts without redirect rules still send readers and crawlers on. A redirect cannot replace a page that exists, and the build warns when one points to a page it does not produce.

## Checking a deploy

`tools/check_host.py` in the Mira source checks a live site. It covers status codes, URLs with and without the trailing slash, the site's own 404 page, security headers, content types for twins and AVIF, media caching, and Markdown negotiation.

```bash
python tools/check_host.py https://your-site.example
```

**Your own server.** Serve `dist/` as static files. Map a request for `/path/` to `/path/index.html`, return `404.html` with status 404 for missing files, and send `.md` files as `text/markdown`. The `nginx.conf` from the `docker` host is a complete example.

## Clean URLs

Pages live at paths ending in a slash, each as an `index.html` in its own folder, so they work on every host without rewrite rules.

## Caching

Pages carry their CSS and runtime inline, so a page is one request. Cache HTML briefly and let it revalidate. Fonts and media are marked immutable; their file names change when their contents do.
