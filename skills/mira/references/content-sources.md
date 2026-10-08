# Content from a CMS or API

Build collections from Sanity, Contentful, Supabase, any GraphQL API, or any JSON endpoint, including functions on AWS, Google Cloud, and Azure.

A collection can come from where your content already lives. Name a source in the collection's schema, and `mira build` fetches it, turns each item into an entry, and checks it against the schema like a Markdown file in `content/`. Routes give entries URLs, templates render them, and agents read them over [MCP](mcp.md), the same as local content.

```json
{
  "collections": {
    "posts": {
      "fields": { "title": "string", "date": "date", "excerpt": "string?" },
      "source": {
        "sanity": {
          "project": "abc123",
          "dataset": "production",
          "query": "*[_type == \"post\" && defined(slug.current)]"
        },
        "map": { "date": "publishedAt" }
      }
    }
  }
}
```

A collection can have a source and files in `content/<name>/` at once; their entries are sorted together. Without a schema, every top level field of each item is kept.

## Fields, slugs, and bodies

Each schema field is read from the item field with the same name. `map` reads it from somewhere else, with a dotted path such as `author.name` or `tags.0`:

| Key in `map` | What it sets |
| --- | --- |
| `slug` | The entry's URL segment. Defaults to `slug`; a Sanity slug object works as is |
| `body` | The entry's content. Defaults to `body`. Markdown text, Sanity Portable Text, and Contentful Rich Text are all converted to Markdown |
| any field | That field, read from the given path |

Images in rich text are downloaded at build time and processed like local [media](media.md): sized, encoded to AVIF, and served from your site rather than the CMS's servers.

## Sanity

| Key | What it is |
| --- | --- |
| `project` | The project ID |
| `dataset` | The dataset, such as `production` |
| `query` | A GROQ query that returns a list of documents |
| `token_env` | Optional. An environment variable holding a read token, for private datasets |

Only published documents are read. Without a token, Mira reads from Sanity's CDN.

## Contentful

```json
"source": {
  "contentful": { "space": "abc123", "content_type": "blogPost", "token_env": "CONTENTFUL_TOKEN" },
  "map": { "date": "publishDate" }
}
```

| Key | What it is |
| --- | --- |
| `space` | The space ID |
| `content_type` | The content type ID |
| `token_env` | The environment variable holding a Content Delivery API token |
| `environment` | Optional. Defaults to `master` |
| `locale` | Optional. Defaults to the space's default locale |

Every entry of the type is read, a thousand at a time. Linked assets become objects with their `url`, `title`, and `description`, and linked entries become their fields, two levels deep. Each item also has `id`, `created`, and `updated` from Contentful, so `"map": { "slug": "id" }` works for types without a slug field.

## Supabase

```json
"source": {
  "supabase": {
    "url": "https://abcd.supabase.co",
    "table": "events",
    "select": "title,slug,starts,venue(name)",
    "filter": { "published": "eq.true" },
    "key_env": "SUPABASE_ANON_KEY"
  }
}
```

| Key | What it is |
| --- | --- |
| `url` | The project URL |
| `table` | The table or view |
| `select` | Optional. Columns, in PostgREST syntax. Defaults to `*` |
| `filter` | Optional. Column conditions, in PostgREST syntax, such as `"eq.true"` or `"gte.2026-01-01"` |
| `key_env` | The environment variable holding the key. The anon key reads what row level security allows |

Rows are read a thousand at a time, so tables of any size load.

## GraphQL

Any GraphQL API: Hygraph, Shopify's Storefront API, WordPress with WPGraphQL, Strapi, Payload, and others.

```json
"source": {
  "graphql": {
    "url": "https://shop.example.com/api/2026-07/graphql.json",
    "query": "{ products(first: 100) { nodes { handle title description } } }",
    "items": "data.products.nodes",
    "headers": { "X-Shopify-Storefront-Access-Token": "SHOPIFY_TOKEN" }
  },
  "map": { "slug": "handle" }
}
```

`items` is the path to the list in the response. `token_env` sends a bearer token, and `headers` sends other headers, each naming the environment variable that holds its value. A GraphQL error in the response fails the build with its message.

## Any JSON endpoint

Any HTTPS endpoint that returns a list: a REST API such as Strapi, Directus, WordPress, or Ghost, or a function you write on AWS Lambda, Google Cloud Functions, Azure Functions, Supabase Edge Functions, or Cloudflare Workers that reads from DynamoDB, Firestore, Cosmos DB, or anything else.

```json
"source": {
  "json": {
    "url": "https://api.example.com/v1/menu",
    "items": "data",
    "headers": { "x-api-key": "MENU_API_KEY" }
  }
}
```

| Key | What it is |
| --- | --- |
| `url` | An `https://` URL. Query strings are allowed; credentials are not |
| `items` | Optional. The path to the list in the response, when it is not the response itself |
| `token_env` | Optional. An environment variable holding a bearer token |
| `headers` | Optional. Headers, each naming the environment variable that holds its value |

## Tokens and secrets

Tokens are never written in `mira.config.json`. Each source names an environment variable, and the build stops with the variable's name if it is not set. Set them in your shell for local builds and in your host's settings for builds there. Tokens never reach the cache, the build output, or error messages.

## Builds, caching, and failures

- `mira build` fetches every source on every build, so a deploy publishes current content. Deploy again when content changes, for example with your CMS's webhook and your host's deploy hook.
- `mira dev` reuses what it fetched for 10 minutes, so saving a file does not refetch.
- Responses are cached in `.mira/cache/`. If a source cannot be reached and a cached copy exists, the build uses it and warns, with the copy's age. Without a cached copy, the build fails with the reason.
- Requests go over HTTPS only, time out after 30 seconds, and are limited to 32 MB.
- An item that does not match the schema fails the build, naming the collection and the item, such as `collections.posts.source item 4: missing required field date`.
