# Security policy

## Reporting a vulnerability

Report vulnerabilities privately through [GitHub security advisories](https://github.com/buildwithmira/mira/security/advisories/new), or by email to hi@omrajguru.in. Do not open a public issue.

Include the affected version, steps to reproduce, and the impact you observed. You will get a reply within 3 business days. Once a fix is ready, we will agree on a disclosure date with you and credit you in the advisory unless you prefer otherwise.

## Supported versions

Mira is pre-release. Security fixes go into the latest release only.

## Scope

In scope:

- The `mira` binary and compiler, including `mira dev`, `mira mcp`, and `mira migrate`
- HTML, headers, and host configuration that Mira generates
- The client runtime and the npm packages

Out of scope:

- `mira dev` is a local development server and is not meant to be exposed to a network
- Vulnerabilities in a site's own content or custom templates
