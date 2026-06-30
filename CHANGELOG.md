# Changelog

All notable changes to this repository are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

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