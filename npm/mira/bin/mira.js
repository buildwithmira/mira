#!/usr/bin/env node
"use strict";

// Runs the native mira binary from the platform package npm installed
// alongside this one, passing arguments, input, output, and exit code through.

const { spawnSync } = require("node:child_process");

const target = `${process.platform}-${process.arch}`;
const pkg = `@miraframework/mira-${target}`;
const exe = process.platform === "win32" ? "mira.exe" : "mira";

let binary;
try {
  binary = require.resolve(`${pkg}/bin/${exe}`);
} catch {
  const supported = ["linux-x64", "linux-arm64", "darwin-arm64", "darwin-x64", "win32-x64", "win32-arm64"];
  console.error(
    supported.includes(target)
      ? `mira: the package ${pkg} is missing.\n` +
          "It is an optional dependency of @miraframework/mira. Reinstall without --no-optional or --omit=optional."
      : `mira: no prebuilt binary for ${target}.\n` +
          "Build from source with: cargo install --git https://github.com/buildwithmira/mira mira",
  );
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit", windowsHide: false });
if (result.error) {
  console.error(`mira: could not run ${binary}: ${result.error.message}`);
  process.exit(1);
}
if (result.signal) {
  process.kill(process.pid, result.signal);
}
process.exit(result.status ?? 1);
