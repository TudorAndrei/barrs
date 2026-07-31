# AGENTS

## Release policy

- Do not create release tags manually when the Cargo package version has not
  been updated.
- The git tag and the Rust crate version must match.
- Releases derive their SemVer version from conventional commits with Cocogitto.
- Use the `mise` release flow; it invokes `cargo-release` internally instead of
  hand-editing `Cargo.toml` or manually tagging.

## Standard release flow

1. From a clean `main` working tree, preview the next version:

   ```bash
   mise run release-plan
   ```

2. Execute the automated release only after reviewing the preview:

   ```bash
   mise run release-auto
   ```

   The script derives the version, updates the changelog and Cargo metadata,
   verifies the binary version, and uses `cargo-release` to commit, create the
   matching `v{{version}}` tag, and push.

3. Let GitHub Actions build release artifacts and update `Formula/barrs.rb`.

## Configuration

- `release.toml` defines the release behavior for this repository.
- Tags must be created as `v{{version}}`.
- `cargo publish` is disabled because releases are distributed through GitHub
  artifacts and Homebrew, not crates.io.

## Version checks

- The release automation verifies before committing/tagging:

  ```bash
  cargo run -- --version
  ```

- The printed version must match the derived release version and
  `v{{version}}` tag.
