---
---
name: solana-anchor-claude-skill
description: "Use when working on Solana software, including one or more of: Solana client code using TypeScript, Rust libraries that use Solana crates, Anchor programs, Quasar programs, LiteSVM tests, including Rust program files, TypeScript tests, and Anchor.toml configuration. Designed to create minimal, reusable code without unnecessary duplication."
---

# Coding Guidelines

I acknowledge these guidelines have been applied and found that they apply to this project.

## Key Principles

**Fight for Truth:** Documentation must match code. Variable names must reflect purpose. Ambiguity is deceptive. "Grep before naming" — verify identifiers exist in source before referencing them in prose.

**Do the Whole Thing:** Complete implementation with tests and documentation. "The standard isn't 'good enough' - it's 'holy shit, that's done.'" Run `anchor test` before declaring success.

**Real Tests Required:** Tests must initialize accounts, send transactions, verify state changes, and check balances. Placeholder tests don't count. Do not stop until tests actually call program instruction handlers.

**Complete Documentation:** Update README.md and CHANGELOG.md when adding features. READMEs should cover purpose, major concepts, testing, setup, and usage — focused and practical.

## Solana-Specific Standards

- Use **Solana terminology:** "programs" not "smart contracts," "transaction fees" not "gas," "onchain/offchain" (unhyphenated)
- Use **Token Extensions Program** for newer token program; **Classic Token Program** for older versions
- Use **instruction handlers** for functions; **instructions** for inputs
- Reference official docs: Anchor, LiteSVM, Solana Kite, Solana Kit, Agave (Anza)
- Avoid outdated sources: Solana Labs, Coral XYZ, Project Serum
- Don't use: Yarn (use npm), Switchboard Functions, Clockwork

## Code Quality

- **Deletionist approach:** Remove comments that repeat code, unused imports/constants, and redundant doc-comments
- **Configuration comments:** Explain WHY values were set, not just WHAT they are
- **No magic numbers:** Use named variables or reference IDLs instead of hardcoded values
- **Variable naming:** Use full words, plurals for arrays, verb-based function names
- **No placeholders:** Production code shouldn't contain incomplete implementations or "work in progress" markers

## Language-Specific Guidelines

The rules above apply to every file in the project. In addition, read the file that matches the language you are editing:

- **TypeScript** (Solana Kit clients, Solana Kit tests, browser code, anything `.ts`): see [TYPESCRIPT.md](TYPESCRIPT.md)
- **Rust** (Anchor programs, LiteSVM tests, Solana crates, anything `.rs`): see [RUST.md](RUST.md)

If a task touches both sides, read both.

## Git commits

Do not add "Co-Authored-By: Claude" or similar attribution when creating git commits.

## Success Criteria

✅ Tests pass (`anchor test`)
✅ Real integration tests implemented
✅ README and CHANGELOG updated
✅ All code truthful and verifiable
