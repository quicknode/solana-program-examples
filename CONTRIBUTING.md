# Contribution Guidelines

Thank you for considering a contribution to this repository. We welcome new examples, fixes, and improvements from the community.

## How to Contribute

- **Code:** Add new examples or improve existing ones (bug fixes, optimizations, additional features).
- **Bug reports, ideas, feedback:** Open an issue describing what you found or what you'd like to see.

## Project structure

- Each example lives at `category/example-name/<framework>/`, e.g. `basics/counter/anchor/`.
- Supported frameworks: `anchor`, `quasar`, `pinocchio`, `native`. Use the existing layout as a reference.
- Tests live alongside the program in a `tests/` directory.

## Tooling

- **Package manager:** `pnpm`. Commit `pnpm-lock.yaml`. Do not use yarn or npm here.
- **Formatter / linter:** [Biome](https://biomejs.dev/). Run `pnpm fix` from the repo root before submitting a PR.

## Testing

This repo uses an in-process test runtime - no local validator boot, no `solana-test-validator`, no `anchor test --validator legacy`.

**Anchor examples** are tested in Rust with [LiteSVM](https://www.anchor-lang.com/docs/testing/litesvm). Tests live in `programs/<name>/tests/`, load the compiled program with `include_bytes!("../../../target/deploy/<name>.so")`, and run with `cargo test` (build the `.so` first with `cargo build-sbf` or `anchor build`). The conventional `Anchor.toml` `[scripts]` entry is:

```toml
[scripts]
test = "cargo test"
```

Optional helpers come from the [`solana-kite`](https://crates.io/crates/solana-kite) crate (wallet creation, token mint helpers, `send_transaction_from_instructions`).

**Quasar examples** are tested in Rust with QuasarSVM. Run `quasar build` (which also generates the Rust client crate under `target/client/rust/` that the tests import), then `quasar test` or `cargo test`.

**Native and Pinocchio examples** use `litesvm` directly from Rust, except for a few that keep TypeScript tests (`tsx --test` with [`solana-kite`](https://solanakite.org) and [`@solana/kit`](https://solanakit.com)) where the example is specifically about client-side tooling.

Do not write TypeScript tests for Anchor or Quasar programs, and do not use `anchor.workspace` or `program.methods.X().rpc()`.

Tests must exercise the program for real: initialize accounts, send transactions through the program's instruction handlers, and assert resulting state and balances. Placeholder tests (`assert!(true)`, build-only checks) don't count.

## Style

Write American English in prose (e.g. "behavior", "initialize", "favor"). Code identifiers stay as-is.

Other conventions:

- One H1 per markdown file.
- Fenced code blocks include a language tag (` ```rust `, ` ```typescript `, ` ```bash `, ` ```toml `).
- Use full words rather than abbreviations (`transaction`, not `tx` or `txn`; `account`, not `acc`).
- Prefer `async`/`await` over `.then()`/`.catch()`.
- Use `Array<T>` rather than `T[]` in TypeScript.
- Avoid magic numbers - name or explain them.
- Write "onchain" / "offchain" as single words (no hyphen).

## Excluding an example from CI

Add the project path to `.ghaignore` to skip it during CI builds. If you remove or replace an example, update `.ghaignore` accordingly.

## Code of conduct

Be respectful and inclusive. Constructive feedback only. Report any conduct issues to the maintainers.
