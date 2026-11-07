# Changelog

## [2026-07-22]

### Changed

- Migrated to Quasar 0.1.0 (`0.1.0-release` branch, rev `be60fca`): Quasar.toml
  rewritten to the 0.1.0 schema, `idl-build` feature and `lib` crate-type added,
  and tests rewritten from the direct QuasarSVM harness to `quasar-test`
  (`#[quasar_test]` fixtures, `crate::cpi` instruction builders, `Outcome`
  assertions). The `quasar-svm` git dev-dependency is gone. The message field
  and instruction args changed from `String<1024>` to `String<1024, 2>`:
  zeropod 0.3.3 enforces at compile time that the capacity fits the length
  prefix, so a 1024-byte string needs a 2-byte (u16) prefix — the account and
  instruction wire layout now carries a u16 LE length instead of u8. The
  update test now also verifies the realloc'd account contents.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
