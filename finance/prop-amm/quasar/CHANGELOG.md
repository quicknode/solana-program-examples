# Changelog

## [2026-07-22]

### Changed

- Migrated to Quasar 0.1.0 (`0.1.0-release` branch, rev `be60fca`): Quasar.toml
  rewritten to the 0.1.0 schema, `idl-build` feature and `lib` crate-type added,
  and tests rewritten from the direct QuasarSVM harness to `quasar-test`
  (`#[quasar_test]` fixtures, `crate::cpi` instruction builders, `Outcome`
  assertions). quasar-test has no `with_slot`, so the tests pin the current
  slot by writing the Clock sysvar ACCOUNT via `test.set_account` (the SVM
  fills its sysvar cache from provided accounts first), which keeps the
  stale-price scenario expressible. The `quasar-svm` git dev-dependency is
  gone. Program-source fix for 0.1.0: `Seed` is no longer in the prelude, so
  `swap.rs` and `withdraw_inventory.rs` now import it from `quasar_lang::cpi`.

## 2026-07-11 (later)

Retuned the walkthrough trade to 5 NVDAx (825.825 USDC at the ask,
824.175 back at the bid, 1.65 round-trip spread) so the numbers match the
book's convention that every character starts with 1,000 USDC. Same math,
same gates; only the amounts changed.

## 2026-07-11

Initial version: Quasar port of the prop-amm example, matching the Anchor
sibling's design and math.
