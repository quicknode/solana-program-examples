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

This repo uses an in-process test runtime — no local validator boot, no `solana-test-validator`, no `anchor test --validator legacy`.

For Anchor and Quasar examples, tests are written in TypeScript and run with `node:test` via `tsx`:

```bash
npx tsx --test --test-reporter=spec tests/*.ts
```

The conventional `Anchor.toml` `[scripts]` entry is:

```toml
[scripts]
test = "npx create-codama-clients; npx tsx --test --test-reporter=spec tests/*.ts"
```

The TypeScript tests use:

- [`solana-kite`](https://solanakite.org) for the connection, wallet creation, token mint helpers, PDA derivation, and `sendTransactionFromInstructions`.
- [`@solana/kit`](https://solanakit.com) for the core types (`KeyPairSigner`, `Address`, `lamports`).
- A [Codama](https://github.com/codama-idl/codama)-generated client (via `npx create-codama-clients`) for invoking the program instructions. Do **not** use `anchor.workspace` or `program.methods.X().rpc()`.

Native and Pinocchio examples may use `litesvm` directly from Rust where appropriate.

## Style

Write American English in prose (e.g. "behavior", "initialize", "favor"). Code identifiers stay as-is.

Other conventions:

- One H1 per markdown file.
- Fenced code blocks include a language tag (` ```rust `, ` ```typescript `, ` ```bash `, ` ```toml `).
- Use full words rather than abbreviations (`transaction`, not `tx` or `txn`; `account`, not `acc`).
- Prefer `async`/`await` over `.then()`/`.catch()`.
- Use `Array<T>` rather than `T[]` in TypeScript.
- Avoid magic numbers — name or explain them.
- Write "onchain" / "offchain" as single words (no hyphen).

## Excluding an example from CI

Add the project path to `.ghaignore` to skip it during CI builds. If you remove or replace an example, update `.ghaignore` accordingly.

## Code of conduct

Be respectful and inclusive. Constructive feedback only. Report any conduct issues to the maintainers.
