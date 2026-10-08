# Changelog

## 2026-09-24

### Changed

- Renamed the `#[program]` module from `anchor_test` to `pythexample`, matching
  the crate. Anchor names the generated IDL after the module, so the IDL was
  written to `target/idl/anchor_test.json`; Surfpool 1.6 looks for
  `target/idl/pythexample.json` when it deploys the program at the start of
  `anchor test`, and exited with `Runbook execution failed` before any test ran.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
