# AGENTS

## Release policy

- Do not create release tags manually when the Cargo package version has not
  been updated.
- The git tag and the Rust crate version must match.
- Releases derive their SemVer version from conventional commits with Cocogitto.
- Use the `mise` release flow; it runs `cog bump --auto` instead of
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

   Cocogitto derives the version, runs the pre-bump hooks in `cog.toml`
   (Cargo version, tests, binary version check), writes the changelog, commits
   `chore(version): v{{version}}`, creates the matching `v{{version}}` tag, and
   pushes.

3. Let GitHub Actions build release artifacts and update `Formula/barrs.rb`.

## Configuration

- `cog.toml` defines the release behavior for this repository.
- `.cog/changelog.tera` is the changelog template.
- Tags must be created as `v{{version}}`.
- Nothing is published to crates.io; releases are distributed through GitHub
  artifacts and Homebrew.

## Version checks

- A pre-bump hook verifies before committing/tagging:

  ```bash
  cargo run -- --version | grep -Fx 'barrs {{version}}'
  ```

- The printed version must match the derived release version and
  `v{{version}}` tag.
