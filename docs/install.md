# Install Conduit

## Homebrew

Homebrew is the preferred installation path for a team pilot because it avoids
local Rust toolchain setup.

Once the tap is published:

```bash
brew tap Riki1312/conduit
brew install conduit
conduit about
```

Update with:

```bash
brew update
brew upgrade conduit
```

The tap should copy the generated `conduit.rb` formula from each GitHub
release. The source template lives at
`packaging/homebrew/conduit.rb.template`; release automation fills in the
version and archive checksums.

Until the tap is published, install from source or from a release archive.

## Release Archives

Tagged releases publish checksummed binary archives for:

- `aarch64-apple-darwin`
- `x86_64-apple-darwin`
- `x86_64-unknown-linux-gnu`

Install manually by downloading the matching archive from the GitHub release,
extracting `conduit`, and placing it on `PATH`.

## From Source

Use the pinned Rust toolchain from `rust-toolchain.toml`:

```bash
cargo install --path crates/conduit-cli --locked
conduit about
```

Run without installing:

```bash
cargo run -p conduit-cli -- about
```

## Agent Skill

Agents can use the bundled Conduit skill at `skills/conduit/SKILL.md`. Install
or copy that skill into the agent-specific skill directory used by your tooling.

## Maintainer Release Flow

1. Update the workspace package version in `Cargo.toml`.
2. Open and merge the version bump PR.
3. Create and push a matching tag, for example `v0.1.0`.
4. The release workflow publishes archives, checksum files, and a generated
   `conduit.rb` Homebrew formula.
5. Copy the generated formula into the Homebrew tap and commit it there.
