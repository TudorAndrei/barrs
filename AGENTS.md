# AGENTS

## Release policy

- Do not create release tags manually when the Cargo package version has not been updated.
- The git tag and the Rust crate version must match.
- Use `cargo-release` for releases instead of hand-editing `Cargo.toml` and manually tagging.

## Standard release flow

1. Make sure the working tree is clean.
2. Run a dry run:

   ```bash
   cargo release patch
   ```

3. Execute the release:

   ```bash
   cargo release patch --execute
   ```

4. Let GitHub Actions build release artifacts and update `Formula/barrs.rb`.

## Configuration

- `release.toml` defines the release behavior for this repository.
- Tags must be created as `v{{version}}`.
- `cargo publish` is disabled because releases are distributed through GitHub artifacts and Homebrew, not crates.io.

## Version checks

- Before creating a release, verify:

  ```bash
  cargo run -- --version
  ```

- The printed version must match the intended tag version.
