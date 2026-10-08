# Installation

Install Mira from npm, download a prebuilt binary, or build it from source, then check that it works.

Mira is one native binary named `mira`, built for Linux (x64 and arm64), macOS (Apple silicon and Intel), and Windows (x64 and arm64).

## Start a new site

```bash
npm create mira@latest my-site
```

This creates `my-site/` from the starter template and adds a `package.json` with `dev` and `build` scripts and Mira as a dev dependency. Then:

```bash
cd my-site
npm install
npm run dev
```

Node.js 18 or later is needed to install. The `mira` binary itself runs without Node.

## Add Mira to an existing project

```bash
npm install --save-dev @miraframework/mira
```

npm installs the binary for your platform alongside a small launcher, so `npx mira` and `mira` inside package scripts both work.

## Download a binary

Each [release on GitHub](https://github.com/buildwithmira/mira/releases) has an archive per platform and a `SHA256SUMS` file. Unpack the archive and put `mira` on your `PATH`.

## Build from source

With a [Rust toolchain](https://rustup.rs) installed:

```bash
cargo install --git https://github.com/buildwithmira/mira mira
```

## Check the install

```bash
npx mira --version
```

`mira --help` lists every command, and `mira build --help` shows the options for one.

## Verify a release

Every npm package is built and published by the repository's release workflow and carries a provenance statement linking it to that build. To check the packages in a project:

```bash
npm audit signatures
```
