# Actions

Let agents do things on your site, such as booking a table, with typed input, an endpoint you choose, and a confirmation from the person they act for.

Reading a site is half of what an agent does for someone. The other half is acting on it: booking a table, joining a waitlist, requesting a quote. An action declares one of these as a typed request, so an agent can make it correctly and only with the person's agreement.

## Declare an action

Add it to `mira.config.json`:

```json
{
  "actions": {
    "book_table": {
      "description": "Book a table for dinner. Bookings open 17:00 to 22:00.",
      "input": {
        "name": "string",
        "party_size": "number",
        "date": "date",
        "time": "time",
        "notes": "string?"
      },
      "endpoint": "https://api.example.com/bookings"
    }
  }
}
```

| Key | What it does |
| --- | --- |
| `description` | What the action does and any rules an agent should follow, in plain sentences |
| `input` | Each field and its type, written as in [collection schemas](collections.md). `?` marks a field optional |
| `endpoint` | Where the input goes, as a JSON `POST`. `https://` only, or `http://localhost` while developing |
| `confirm` | Ask the person before sending. On unless set to `false` |

Names use lowercase letters, digits, and `_`, and cannot be the name of a built-in MCP tool such as `search`. The build checks every action, and a mistake fails it, naming the key to fix.

## The endpoint

Your site stays static. The endpoint is whatever receives the request: a form service, your own API, or a function on AWS Lambda (with a function URL or API Gateway), Google Cloud Functions or Cloud Run, Azure Functions, Supabase Edge Functions, Cloudflare Workers, Vercel, or Netlify. It gets the input as a JSON body:

```http
POST /bookings HTTP/1.1
Content-Type: application/json

{"date":"2026-10-09","name":"Ada","party_size":2,"time":"19:30"}
```

Answer with a `2xx` status when the action succeeded. The first 4 KB of your response, such as a booking reference, is passed back to the agent. Any other status tells the agent the action may not have happened.

Validate the input on the endpoint too. An agent going through Mira sends only well typed input, but the endpoint is public, and anyone can call it directly.

## How an agent runs it

Each action becomes an MCP tool with the action's name and an input schema built from its fields. Before anything is sent:

1. **The input is checked.** A missing field, a wrong type, or a field the action does not take is refused with a message naming it, such as `party_size must be a number, got "two"`.
2. **The person agrees.** When the MCP client can ask its user directly, Mira asks it to show what will be sent, and to whom, and sends only on yes. Otherwise the tool returns that summary with a one-time confirmation, and the agent has to ask the person and call again with it. A confirmation works once, only for the exact input it was given for, and for 10 minutes.
3. **The input is sent,** once, and the endpoint's answer comes back.

Set `"confirm": false` only for actions that are harmless to repeat and send nothing personal. It applies when the agent reads your project; actions read from a deployed site always ask.

## Without MCP

Every build publishes the actions at `/_mira/actions.json`, with a JSON Schema for each input, and lists them in `llms.txt`, so any agent can find them:

```markdown
## Actions

- book_table: Book a table for dinner. `POST https://api.example.com/bookings` with date (date), name (string), notes (string?), party_size (number), time (time)
```

## Security

- Mira sends only to the endpoint the site declares, only over `https://` (or to `localhost`), and follows no redirects.
- Input is limited to the declared fields and 16 KB.
- In `--url` mode, the actions come from someone else's site, so Mira treats them as untrusted. Each endpoint is checked again before sending. An endpoint on `localhost`, a loopback, private, or link-local address is refused, and so is any name that resolves to one, checked on the address Mira actually connects to. A site cannot use your agent to reach your own machine or network. A confirmation covers the exact input and endpoint the person saw. And every action asks the person first, even when the site sets `confirm: false`.
