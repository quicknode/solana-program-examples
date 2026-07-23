# Changelog

## [2026-07-22]

### Changed

- Migrated to Quasar 0.1.0 (`0.1.0-release` branch, rev `be60fca`): Quasar.toml
  rewritten to the 0.1.0 schema, `idl-build` feature added, and tests rewritten
  from the direct QuasarSVM harness to `quasar-test` (`#[quasar_test]`
  fixtures, `crate::cpi` instruction builders, `Outcome` assertions). The
  previously-floating `quasar-lang` git dependency is now pinned by rev; the
  `quasar-svm` git dev-dependency, the generated-client path dev-dependency,
  and the unused `solana-address` dev-dep are gone. The borsh/keccak
  Bubblegum-mirroring helpers (metadata hashing, merkle-tree account layout)
  are unchanged; only the harness layer moved.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
