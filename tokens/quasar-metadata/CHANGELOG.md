# Changelog

## [2026-07-23]

### Added

- Vendored from [blueshift-gg/quasar](https://github.com/blueshift-gg/quasar)
  rev `623bb70f` (the last revision shipping the `metadata/` crate), so the
  three Metaplex-metadata examples can build against the Quasar
  `0.1.0-release` pin. See README.md for the full rationale.

### Changed (adaptations to the 0.1.0 `quasar-lang` API)

- `Cargo.toml` rewritten as a standalone crate: `quasar-lang` moves from the
  upstream workspace path dep to the git pin `rev = "be60fca"`; workspace
  lints/package values inlined.
- `AccountInit::init` impls gained the trait's new `R: RentAccess` type
  parameter (`InitCtx<'a, R>`).
- `quasar_lang::pda::based_try_find_program_address` was renamed upstream;
  both PDA helpers now call `try_find_program_address`.
- `Seed` left the prelude; `init.rs` imports `quasar_lang::cpi::Seed`.
- `CpiDynamic::set_data_len` became `unsafe`; both call sites (create/update
  metadata) wrap it with a SAFETY comment — the preceding raw-pointer writes
  initialize exactly `data[0..offset]`.
