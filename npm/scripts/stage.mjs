// Assembles the npm packages for one release.
//
//   node npm/scripts/stage.mjs <binaries> <out> [--tag v0.1.0] [--partial]
//
// <binaries> holds mira-<os>-<cpu> (plus .exe on Windows) for every entry in
// platforms.json. Writes one folder per package to <out> and prints them in
// publish order: platform packages first, then @miraframework/mira, then
// create-mira. The version comes from Cargo.toml; --tag must match it.
// --partial skips missing binaries, for local testing only.

import { chmodSync, copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const npmDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repo = resolve(npmDir, "..");
const [binaries, out] = process.argv.slice(2).filter((arg, i, all) => !arg.startsWith("--") && all[i - 1] !== "--tag");
const tag = process.argv.includes("--tag") ? process.argv[process.argv.indexOf("--tag") + 1] : null;
const partial = process.argv.includes("--partial");
if (!binaries || !out) {
  console.error("usage: node npm/scripts/stage.mjs <binaries> <out> [--tag vX.Y.Z] [--partial]");
  process.exit(2);
}

const cargo = readFileSync(join(repo, "Cargo.toml"), "utf8");
const version = /\[workspace\.package\][^[]*?\nversion = "([^"]+)"/.exec(cargo)?.[1];
if (!version) throw new Error("no version in [workspace.package] of Cargo.toml");
if (tag && tag !== `v${version}`) {
  throw new Error(`tag ${tag} does not match Cargo.toml version ${version}`);
}

const platforms = JSON.parse(readFileSync(join(npmDir, "platforms.json"), "utf8"));
const main = JSON.parse(readFileSync(join(npmDir, "mira", "package.json"), "utf8"));
const licenses = ["LICENSE-MIT", "LICENSE-APACHE"];

rmSync(out, { recursive: true, force: true });
const order = [];

function writePackage(dir, pkg, files) {
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "package.json"), JSON.stringify(pkg, null, 2) + "\n");
  for (const [from, to] of files) {
    mkdirSync(dirname(join(dir, to)), { recursive: true });
    copyFileSync(from, join(dir, to));
  }
  for (const license of licenses) copyFileSync(join(repo, license), join(dir, license));
  order.push(dir);
}

const optional = {};
for (const { os, cpu, target } of platforms) {
  const exe = os === "win32" ? "mira.exe" : "mira";
  const binary = join(binaries, `mira-${os}-${cpu}${os === "win32" ? ".exe" : ""}`);
  if (!existsSync(binary)) {
    if (partial) continue;
    throw new Error(`missing ${binary}`);
  }
  const name = `@miraframework/mira-${os}-${cpu}`;
  const dir = join(out, `mira-${os}-${cpu}`);
  writePackage(
    dir,
    {
      name,
      version,
      description: `The mira binary for ${os} ${cpu} (${target}).`,
      license: main.license,
      author: main.author,
      repository: { ...main.repository, directory: "npm" },
      homepage: main.homepage,
      os: [os],
      cpu: [cpu],
      files: ["bin", ...licenses],
      preferUnplugged: true,
    },
    [[binary, join("bin", exe)]],
  );
  if (os !== "win32") chmodSync(join(dir, "bin", exe), 0o755);
  writeFileSync(join(dir, "README.md"), `# ${name}\n\nThe \`mira\` binary for ${os} ${cpu}. Install [@miraframework/mira](https://www.npmjs.com/package/@miraframework/mira) instead; it picks the right binary for your platform.\n`);
  optional[name] = version;
}

writePackage(join(out, "mira"), { ...main, version, files: [...main.files, ...licenses], optionalDependencies: optional }, [
  [join(npmDir, "mira", "bin", "mira.js"), join("bin", "mira.js")],
  [join(npmDir, "mira", "README.md"), "README.md"],
]);

const create = JSON.parse(readFileSync(join(npmDir, "create-mira", "package.json"), "utf8"));
writePackage(join(out, "create-mira"), { ...create, version, files: [...create.files, ...licenses], dependencies: { "@miraframework/mira": version } }, [
  [join(npmDir, "create-mira", "index.js"), "index.js"],
  [join(npmDir, "create-mira", "README.md"), "README.md"],
]);

console.log(order.join("\n"));
