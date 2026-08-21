# Changelog

All notable changes to this repository are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [2026-08-21] - Anchor v1 kept alongside Anchor v2

Anchor v1 is expected to stay on long-term support, and many deployed programs
will stay with it. Both versions of every Anchor example now ship, each built and
tested by its own CI job.

### Added

- Every one of the 55 Anchor examples gains a sibling `anchor-v1/` directory
  holding the example exactly as it stood before the v2 port, restored from
  `94abbea`, the last commit on `main` before that merge. 864 files. The 17
  third-party `.so` test fixtures are byte-identical to the ones already tracked
  under `anchor/`, so git stores one copy and the tree grows by ~2.3 MB of text.
- `.github/workflows/anchor-v1.yml` builds and tests them on Anchor 1.1.2,
  installed through avm. It is the workflow as it stood at `94abbea`, with
  project discovery changed to `find -type d -name "anchor-v1"`. The v2-only IDL
  workaround (`anchor#4947`, enum variants) is not carried over: that bug does not
  exist in 1.1.2, so v1 builds generate IDLs normally.

### Changed

- The `Anchor` workflow is now `Anchor v2`, so the two checks read as a pair. Its
  filename, triggers and `find -type d -name "anchor"` discovery are unchanged.
  Both workflows match a directory name exactly, so neither can ever see the
  other's projects.
- Every Anchor example README now names the CLI its commands need, on both sides:
  `anchor/` pages say Anchor v2 and `anchor-v1/` pages say Anchor v1. A bare
  `anchor build` was unambiguous while the repository had one Anchor and is not
  any more. `README.md` and `CONTRIBUTING.md`, which sit above both, show both.

### Note

- The `anchor-v1/` crates are not members of the root Cargo workspace and cannot
  be: they carry the same package names as their `anchor/` siblings. `cargo fmt`
  and `cargo clippy` therefore do not see them, and the Anchor v1 workflow is what
  keeps them honest. They also have no committed `Cargo.lock` (`.gitignore` ignores
  `**/*/Cargo.lock`), so their transitive dependencies resolve fresh on each run and
  can break without anyone touching the directory.

## [2026-08-16] - Every Anchor example on Anchor v2.0.0-rc.1

All 55 Anchor examples build and pass their tests on 2.0.0-rc.1 (304 tests),
and `cargo fmt --check` and `cargo clippy -- -D warnings` are clean.

### Changed

- The remaining 39 Anchor examples, all of `tokens/`, `finance/` and
  `compression/`, now build against `anchor-lang` 2.0.0-rc.1, joining the
  `basics/` examples ported below. `.github/workflows/anchor.yml` installs
  2.0.0-rc.1, since `anchor build` under a v2 CLI will not build v1 programs.
- `docs/anchor-v2-migration.md` collects every difference the port ran into,
  ordered by how often it bites. The rules the compiler will not catch for you
  are called out: borrows held across CPIs, `Box`'s missing `cpi_handle_mut`
  forwarding, and hand-built read-only handles over a live data account.
- `has_one` is deprecated in v2 and this repository's `rust.yml` runs
  `cargo clippy -- -D warnings`, so every one of the 161 uses across 66 files
  in the Anchor programs moves to the `address` constraint on the sibling field
  it named. The Quasar crates keep `has_one`, which is still current there.
- The seven `transfer-hook` examples supply their own entrypoint. v2's
  `#[program(interface, ...)]` generates a CPI client and no dispatch, so the
  program declared that way builds to a ~900-byte object with no `entrypoint`
  crate has no entrypoint symbol, while an executable `#[program]` limits
  byte, which the transfer-hook interface's eight-byte values cannot use. Each
  crate now builds with `no-entrypoint`, so anchor exports its dispatch as
  `__anchor_dispatch`, and `src/entrypoint.rs` maps the interface
  discriminators onto handlers before delegating.
- `tokens/pda-mint-authority` and `tokens/token-extensions/cpi-guard` build
  their PDA by hand (`create_account` plus `initialize_mint2` /
  `initialize_account3`). Both examples exist to show an account that is its own
  authority, and a v2 SPL `init` constraint cannot name the account being
  initialized.
- `finance/order-book` keeps its ~180 KB zero-copy critbit book zero-copy: v2's
  `Account<T>` derefs straight to `T`, so `load_init` / `load_mut` simply go
  away rather than the state converting to borsh.
- Tests that asserted on an Anchor error *name* now assert on the numeric custom
  code (the `#[error_code]` discriminant plus the default 6000 offset). v2 does
  not log variant names, so the old assertions could never match.

### Removed

- `tokens/token-extensions/nft-meta-data-pointer` no longer depends on
  `session-keys`. That crate is Anchor v1 only: its `Session` derive requires
  `Option<Account<'info, SessionToken>>`, and `SessionToken` is not `Pod`, so
  v2's zero-copy `Account<T>` cannot hold it either. The program reads the
  session-token account layout itself (`src/session.rs`), checking owner,
  discriminator and PDA, and spells out the `#[session_auth_or]` fallback in the
  handler, so the gasless-session lesson and its security warning both survive.

## [2026-08-13] - Anchor examples in `basics/` move to Anchor v2.0.0-rc.1

### Changed

- Every Anchor example under `basics/` now builds against `anchor-lang`
  2.0.0-rc.1. v2 is a ground-up rewrite rather than a version bump: the crate is
  `no_std` and built on pinocchio, so handlers take `&mut Context<T>`, the
  `<'info>` lifetime disappears from `#[derive(Accounts)]` structs and account
  wrappers, `Pubkey` becomes `Address`, `.to_account_info()` becomes
  `.cpi_handle_mut()` / `.cpi_handle()`, and `.key()` becomes `.address()`.
- `#[account]` is now zero-copy and requires a `Pod` layout. State holding
  `String` or `Vec` moves to `#[account(borsh)]` plus `BorshAccount<T>`
  (`account-data`, `close-account`, `favorites`, `realloc`, `pyth`); state that
  is already fixed-layout keeps the zero-copy default but must carry explicit
  padding (`program-derived-addresses`) and cannot use `bool`
  (`cross-program-invocation` uses `PodBool`).
- Instruction data is wincode-encoded rather than borsh. The `#[program]` macro
  expands to `wincode` paths, so every program crate takes a direct `wincode`
  dependency. `BorshConfig` keeps the wire format byte-identical to borsh, so
  the checked-in account layouts and the tests that decode them with borsh are
  unaffected.
- The only edit most LiteSVM tests needed: v2's `solana_program` compat shim has
  no `system_program` submodule (the real module is at the crate root and
  exposes `ID`, not `id()`) and no `pubkey::Pubkey` unless the `compat` feature
  is on (`anchor_lang::Address` is the same 32-byte type).

### Fixed

- Anchor programs that put an `Address` in serialized state pin
  `solana-address = ">=2.6, <2.7"`. anchor-lang 2.0.0-rc.1 is built against
  wincode 0.5, but solana-address 2.7 moved to wincode 0.6; with both in the
  graph, `Address`'s wincode impls belong to the version the `#[account(borsh)]`
  derive is not using, and every `SchemaRead` / `SchemaWrite` bound fails. This
  is the same class of split that the zeropod/quasar-lang pin below addresses.

## [2026-08-04] - Oracle readers reject prices from before a cluster restart

### Added

- The three oracle-priced finance examples (`finance/lending`, `finance/prop-amm`, `finance/perpetual-futures`, Anchor and Quasar variants) now reject an oracle price stamped at or before the `LastRestartSlot` sysvar's slot, with a dedicated error (`PricePredatesRestart` / `PRICE_PREDATES_RESTART`) and a test per variant. A cluster halt stops the slot count but not the wall clock, so after a restart a feed can pass a slot-measured staleness bound while its price is hours old; the market pauses valuation until the publisher posts again. quasar-lang ships no LastRestartSlot sysvar, so each Quasar variant declares the 8-byte layout in `src/last_restart.rs` and reads it via `sol_get_sysvar`.

### Fixed

- The three Quasar variants pin `zeropod = "=0.3.3"`: zeropod 0.3.4 moved to wincode 0.5 while quasar-lang's pinned rev stays on wincode 0.4, so any fresh resolve (these projects commit no lockfile) split the graph across two wincode versions and failed every `Pod*` trait bound.

## [2026-07-23] - Metadata examples on Quasar 0.1.0 (vendored quasar-metadata)

### Added

- `tokens/quasar-metadata`: a vendored copy of the `quasar-metadata` crate from blueshift-gg/quasar rev `623bb70f` (the last revision that shipped it), adapted to compile against the 0.1.0 `quasar-lang` API (`RentAccess` type parameter on `AccountInit::init`, `try_find_program_address` rename, `Seed` import from `quasar_lang::cpi`, `unsafe` `set_data_len`). Upstream removed the crate before 0.1.0 with no replacement; vendoring it lets the Metaplex-metadata examples ride the same release pin as everything else. Provenance and local changes are documented in the crate's README and CHANGELOG.

### Changed

- `tokens/token-minter`, `tokens/nft-minter`, and `tokens/nft-operations` now migrate to the 0.1.0-release pin (`be60fca`) like every other example, depending on the vendored crate via `quasar-metadata = { path = "../quasar-metadata" }`. This supersedes the previous day's "not migrated" limitation: all 53 Quasar examples are now on 0.1.0.
- `quasar.yml` drops the `legacy-metadata-examples` job and `.github/.ghaignore` is empty again — the whole matrix builds with the one 0.1.0 CLI.

## [2026-07-22] - Quasar 0.1.0

### Changed

- Migrated 50 of the 53 Quasar examples to the Quasar `0.1.0-release` line, pinned by rev (`be60fca`) because crates.io still hosts `0.0.0` placeholders for `quasar-lang`/`quasar-cli`. Per project: `quasar-lang`/`quasar-spl` repinned (the four previously floating examples — `basics/pyth` and the three `compression` examples — are now pinned too); `Quasar.toml` rewritten to the 0.1.0 schema (`[testing] command`, `[clients] targets`; the old `[toolchain]`/`testing.language`/`testing.rust`/`clients.languages` keys are hard errors in 0.1.0); the `idl-build` feature and `"lib"` crate-type added for the new IDL build; and tests fully rewritten from the direct QuasarSVM harness (`QuasarSvm::new().with_program(...)`, `include_bytes!`, `assert_success`) to the new `quasar-test` fixture harness (`#[quasar_test]`, `Wallet`/`Mint`/`TokenAccount` fixtures, `crate::cpi` instruction builders, `Outcome` assertions). The standalone `quasar-svm` git dev-dependency is gone — `quasar-test` pulls the published `quasar-svm 0.1.0` from crates.io — and generated-client path dev-dependencies were dropped in favor of `crate::cpi` (a path dev-dependency to a not-yet-generated crate now breaks the required `cargo generate-lockfile`).
- `quasar.yml` CI installs the 0.1.0 CLI (`--rev be60fca`), runs `cargo generate-lockfile` before `quasar build` (the 0.1.0 IDL step runs `cargo metadata --locked`), and builds the three unmigrated examples in a separate `legacy-metadata-examples` job with the pre-0.1.0 CLI.
- Compute-unit assertions were dropped from the migrated tests pending recalibration under 0.1.0 (correct values are unknowable until the suite first runs on the new line).

### Known limitations

- `tokens/token-minter`, `tokens/nft-minter`, and `tokens/nft-operations` stay on the pre-0.1.0 pins (quasar `623bb70` / quasar-svm `cb7565d`): they depend on `quasar-metadata`, which was removed upstream before 0.1.0 with no replacement. They are listed in `.github/.ghaignore` and built by the legacy CI job.

## [2026-07-11] - Discoverability and FAQ pass

### Fixed

- Quasar CI broke repo-wide when `quasar-svm`'s HEAD (`c63afd2`, "sbpf v3") moved to `solana-program-runtime` 4.1 / `solana-address` 2.6, which cannot co-resolve with the pinned `quasar-lang` rev `623bb70` (needs `solana-address` <2.6). Pinned `quasar-svm` to `cb7565d` (the last rev before the bump) in every Quasar example that pins `quasar-lang`, matching the pin `prop-amm` already carried. `basics/pyth` and the three `compression` Quasar examples float both dependencies and are left as-is.

### Added

- FAQ sections, written as the questions people actually ask, in the root README and every finance example's `anchor/` README.
- `llms.txt` at the repository root: a summary and link manifest for LLM crawlers and answer engines.
- `docs/example-readme-template.md`, the example-README template that `CONTRIBUTING.md` referenced but which did not exist. It documents the H1 convention and the definition-first opener.

### Changed

- Every finance example README now titles itself `# Solana <Example> (<Framework>)` (e.g. `# Solana Escrow (Anchor)`) and opens with a self-contained definition that names Solana, so each example page stands alone in search results.
- The root README states its toolchain currency explicitly (Anchor 1.1, LiteSVM, July 2026) with a pointer to this changelog.
- `CONTRIBUTING.md` style rules now include the README H1 naming convention and the no-em-dash rule.

## [2026-07-10] - Failed fundraisers can be retired

### Added

- `token-fundraiser` (Anchor): a `close_fundraiser` instruction handler. The Fundraiser PDA is derived from the maker's key alone, so a failed raise used to lock its maker out of ever raising again. The maker can now retire a failed fundraiser (after the deadline, target missed, all contributions refunded), sweeping any direct vault donations to themselves and recovering both rent deposits, then initialize a fresh fundraiser. New error variant `RefundsOutstanding`.
- `token-fundraiser` (Anchor): tests for both contribution caps (`test_contribute_above_cap_fails`, `test_cumulative_contributions_above_cap_fail`) and for every branch of the close path (before deadline, target met, refunds outstanding, donation sweep, and close-then-raise-again).

## [2026-06-30] - Anchor 1.1.2

### Changed

- Upgraded every Anchor program from `anchor-lang`/`anchor-spl` `1.0.0` to the latest stable `1.1.2`, and bumped the Anchor CLI used by `anchor.yml` CI to match (`anchor-version: 1.1.2`).

### Fixed

- `anchor.yml` built no projects when `.ghaignore` was empty: `find … | grep -vE "$ignore_pattern"` treated the empty pattern as "match everything" and dropped the whole list, so the workflow passed without building anything. Guarded the filter (as `native.yml`, `pinocchio.yml` and `solana-asm.yml` already do).
- `vault-strategy` and `perpetual-futures` LiteSVM tests loaded their sibling mock program's `.so` with `include_bytes!`, which is evaluated at compile time. Anchor's IDL build compiles the tests before that sibling `.so` is built, so the build failed. They now read the sibling `.so` at runtime with `std::fs::read`, matching the existing `cross-program-invocation/hand` test.

## [2026-06-12] - Rust + LiteSVM tests everywhere

### Changed

- All native, Pinocchio, and ASM examples are now tested exclusively with Rust + LiteSVM. The web3.js v1 / solana-bankrun / ts-mocha TypeScript test suites (which duplicated existing Rust tests) were removed, along with their `package.json`, `pnpm-lock.yaml`, and `tsconfig.json` files and the `ts/` client directories.
- Rust tests now load the program binary from the workspace `target/deploy/` (built with `cargo build-sbf --manifest-path=./program/Cargo.toml`) instead of per-project `tests/fixtures` directories. Committed foreign-program fixtures (e.g. `mpl_token_metadata.so`) stay where they were.
- ASM examples standardized on `sbpf build`'s default `deploy/` output directory; their inline LiteSVM tests load from there.
- `tools/shank-and-codama` now generates a Rust client (`@codama/renderers-rust`) instead of a TypeScript one, wrapped in the `car-rental-service-client` crate, and its tests are Rust + LiteSVM under `program/tests/`.
- `transfer-hook/block-list` gained a Rust + LiteSVM lifecycle test (`program/tests/`) driving the program through its Codama-generated Rust SDK; the mocha/web3.js test was removed. Its `package.json` now only covers SDK generation.
- CI (`native.yml`, `pinocchio.yml`, `solana-asm.yml`) no longer installs Node/pnpm; it builds with `cargo build-sbf` (or `sbpf build`) and tests with `cargo test`.

### Added

- `basics/hello-solana/pinocchio` Rust + LiteSVM test (it previously had only a TypeScript test).

## [2026-04-08] - Quicknode fork modernization (Mike MacCana)

Mike MacCana led the Quicknode fork of the [Solana Foundation program examples](https://github.com/solana-developers/program-examples) from late 2025. The first commits on this repository lineage are dated **8 April 2026**; the summary below covers that work through the initial merge.

### What changed (high level)

**Toolchain and frameworks.** The tree had accumulated examples from several years of Solana development (including Anchor releases going back to the ~0.26 era in 2022 and many intermediate versions). The fork brought the Anchor examples up to **Anchor 1.0.0** stable (from 1.0.0-rc.5), refreshed Agave/Solana CLI pins, standardized on **pnpm**, and added parallel implementations in **[Quasar](https://quasar-lang.com/docs)**, **Pinocchio**, **Native Rust**, and **ASM** where applicable. Token-2022 examples were renamed to **`token-extensions`**.

**Testing.** Replaced the old pattern of local validators, Bankrun, and scattered TypeScript `anchor test` flows with **LiteSVM in-process tests** for most Anchor programs - matching current Anchor defaults (`cargo test` wired through `Anchor.toml` / `pnpm test`). Fixed broken or flaky tests across Native, Pinocchio, and Anchor; added missing harnesses (e.g. block-list Pinocchio). CI was reworked for a repo this size: path filtering, caching, matrix sharding, and reliable detection of framework roots.

**Programs and layout.** Broke large monolithic `lib.rs` files into **instruction handler modules**; adopted **`InitSpace`** and explicit PDA bumps instead of magic account sizes; corrected several logic bugs (escrow, token swap invariant, counter authority checks, compression Bubblegum program id, and more). Expanded finance and token-extension coverage; reorganized transfer-hook examples (including block-list under Pinocchio).

**Documentation.** Rewrote the root README (framework badges, clearer example blurbs, ASM links), ran a style and **truth audit** on READMEs, and linked canonical [Solana terminology](https://solana.com/docs/references/terminology) on first mention. Added this changelog, `CONTRIBUTING.md` (aligned with LiteSVM testing), README templates, per-example Anchor and Quasar READMEs, fixed Husky for GUI git clients, removed unused maintainer scripts (`sync-package-json`, `cicd.sh`, local-validator helpers for the allow/block-list UI), dropped the orphan `tokens/spl-token-minter/` tree, and removed legacy root `package.json` dependencies (web3.js, Bankrun, chai).

**Removed / deferred.** Dropped duplicate or WIP trees (duplicate block-list Pinocchio copy, Quasar metadata example blocked on `sol_realloc`, root `yarn.lock`). Some examples remain excluded from CI via `.ghaignore` until they build cleanly again (compression, escrow, pyth, and others - see that file for the live list).

## Before June 2026

There was **no changelog** before June 2026. Older history lives in git only.