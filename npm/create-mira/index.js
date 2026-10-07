#!/usr/bin/env node
"use strict";

// npm create mira@latest [directory]
//
// Creates a site with `mira new`, then adds a package.json so the site's
// scripts and its version of Mira are managed by npm.

const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const args = process.argv.slice(2);
if (args.includes("--help") || args.includes("-h")) {
  console.log("Usage: npm create mira@latest [directory]\n\nCreates a new Mira site. The directory defaults to my-site.");
  process.exit(0);
}
const dir = args.find((arg) => !arg.startsWith("-")) ?? "my-site";
const root = path.resolve(dir);

const launcher = require.resolve("@buildwithmira/mira/bin/mira.js");
const result = spawnSync(process.execPath, [launcher, "new", dir, "--no-hints"], { stdio: "inherit" });
if (result.status !== 0) {
  process.exit(result.status ?? 1);
}

const manifest = path.join(root, "package.json");
if (!fs.existsSync(manifest)) {
  const { version } = require("./package.json");
  const name =
    path
      .basename(root)
      .toLowerCase()
      .replace(/[^a-z0-9._-]+/g, "-")
      .replace(/^[._-]+|-+$/g, "") || "mira-site";
  const pkg = {
    name,
    private: true,
    scripts: { dev: "mira dev", build: "mira build" },
    devDependencies: { "@buildwithmira/mira": `^${version}` },
  };
  fs.writeFileSync(manifest, JSON.stringify(pkg, null, 2) + "\n");
}

const gitignore = path.join(root, ".gitignore");
const ignored = fs.existsSync(gitignore) ? fs.readFileSync(gitignore, "utf8") : "";
if (!ignored.split(/\r?\n/).includes("node_modules/")) {
  fs.appendFileSync(gitignore, (ignored && !ignored.endsWith("\n") ? "\n" : "") + "node_modules/\n");
}

const cd = path.relative(process.cwd(), root) || ".";
console.log(`Next:\n  cd ${cd.includes(" ") ? JSON.stringify(cd) : cd}\n  npm install\n  npm run dev`);
