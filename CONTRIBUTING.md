# Contribution Guidelines

Thank you for considering a contribution to this repository. We welcome new examples, fixes, and improvements from the community. For coding guidelines, see the [Quicknode Solana coding skill](https://github.com/quicknode/solana-claude-skill).

See [CHANGELOG.md](./CHANGELOG.md) for release history. This file had no changelog before June 2026.

## How to Contribute

- **Code:** Add new examples or improve existing ones (bug fixes, optimizations, additional features).
- **Bug reports, ideas, feedback:** Open an issue describing what you found or what you'd like to see.

## Project structure

- Each example lives at `category/example-name/<framework>/`, e.g. `basics/counter/anchor/`.
- Supported frameworks: `anchor`, `quasar`, `pinocchio`, `native`, `asm`. Use the existing layout as a reference.
- Anchor and Quasar programs usually keep Rust tests under `programs/<name>/tests/`.
- Native and Pinocchio TypeScript tests (where present) live in a `tests/` directory next to the program.

## Tooling

- **Package manager:** `pnpm`. Commit `pnpm-lock.yaml`. Do not use yarn or npm here.
- **Formatter / linter:** [Biome](https://biomejs.dev/). Run `pnpm fix` from the repo root before submitting a PR.

## Testing

Run `pnpm test` from `category/example/anchor/` or `category/example/quasar/`. For existing test patterns follow `basics/counter/anchor/programs/counter_anchor/tests/test_counter.rs`.

### Native and Pinocchio

- Prefer LiteSVM for new tests.
- Some older Native examples still use `@solana/web3.js` v1 or `solana-bankrun`; do not copy that stack for new work. Migrate toward LiteSVM + Solana Kit when touching those files.

### ASM

ASM examples keep LiteSVM tests inline in `src/lib.rs`. Build with `sbpf build`, test with `cargo test`.

### TypeScript client tests (legacy / optional)

A few paths still use TypeScript with `node:test` and Codama-generated clients. That is not the default for new Anchor examples. Run with:

```bash
npx tsx --test --test-reporter=spec tests/*.ts
```

## Documentation

Every `anchor/` (and other framework) directory should include a `README.md`. Use [docs/example-readme-template.md](./docs/example-readme-template.md) as the starting point.

Also update [CHANGELOG.md](./CHANGELOG.md) when you ship user-visible changes.

### Style

Write American English in prose (e.g. "behavior", "initialize", "favor"). Code identifiers stay as-is.

- One H1 per markdown file.
- Fenced code blocks include a language tag (` ```rust `, ` ```typescript `, ` ```bash `, ` ```toml `).
- Link canonical Solana terms to the [terminology page](https://solana.com/docs/references/terminology) on first mention in READMEs.

## Excluding an example from CI

Add the project path to `.github/.ghaignore` with a one-line comment explaining why (build failure, needs mainnet fixtures, etc.). Remove entries when the example is fixed.

## Code of conduct

Be respectful and inclusive. Constructive feedback only. Report any conduct issues to the maintainers.
