# Contributing to Mira

Thanks for helping. This guide covers how to build Mira, what a good change looks like, and how changes get merged.

## Before you start

For anything larger than a small fix, open an issue first and describe the problem and the change you have in mind. This avoids work on something that does not fit the project.

Bug reports need a minimal site that reproduces the problem, the command you ran, and the full output.

## Building

Mira is a Rust workspace. Install Rust with [rustup](https://rustup.rs), then:

```bash
cargo build
cargo test
```

Run the CLI against a test site:

```bash
cargo run -p mira -- new /tmp/site
cargo run -p mira -- dev --root /tmp/site
```

## Before you open a pull request

These must pass. CI runs the same checks on Linux, macOS, and Windows.

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

- Add a test for every bug fix and every new behavior. Compiler tests live next to the code they cover.
- Keep the client runtime under its 2 KB gzipped budget. A test enforces it.
- Error messages name the file and line, say what is wrong, and give a hint for the fix. Follow the existing ones.
- Do not add a dependency without explaining why in the pull request.
- Update `CHANGELOG.md` under **Unreleased** for any change a user would notice.

## Pull requests

Keep each pull request to one change. Describe what changed and why, and how you tested it. A maintainer will review it, and may ask for changes before merging.

## Releases

Maintainers release by updating the version in `Cargo.toml` and `CHANGELOG.md`, then pushing a `vX.Y.Z` tag. The release workflow builds the binaries, creates the GitHub release, and publishes the npm packages.

## License

By contributing, you agree that your contributions are licensed under the MIT and Apache-2.0 licenses, as described in the [README](README.md#license).
