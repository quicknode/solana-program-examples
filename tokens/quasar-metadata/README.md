# quasar-metadata (vendored)

Metaplex Token Metadata integration for the Quasar Solana framework.

This crate is a vendored copy of the `metadata/` crate from
[blueshift-gg/quasar](https://github.com/blueshift-gg/quasar) at rev
`623bb70f` — the last revision that shipped it. Upstream removed
`quasar-metadata` before the 0.1.0 release with no replacement, but three
examples in this repository (`tokens/token-minter`, `tokens/nft-minter`,
`tokens/nft-operations`) demonstrate Metaplex metadata flows and need it.
Vendoring the crate lets those examples build and test against the same
Quasar `0.1.0-release` pin (`be60fca`) as every other example instead of
staying frozen on a pre-0.1.0 toolchain.

Local changes against the upstream copy are limited to what the 0.1.0
`quasar-lang` API requires (documented in CHANGELOG.md alongside this file)
plus this standalone Cargo.toml. Delete this crate and switch the three
examples back to the upstream dependency if Quasar ships a metadata story
for 0.1.x.

Licensed under MIT (LICENSE-MIT), matching the upstream repository at the
vendored revision.
