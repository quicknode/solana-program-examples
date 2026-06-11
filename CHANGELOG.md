# Changelog

All notable changes to this repository are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

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