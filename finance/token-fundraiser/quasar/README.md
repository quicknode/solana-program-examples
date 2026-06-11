# Token Fundraiser (Quasar)

Onchain crowdfunding toward a target amount in a chosen token, written with [Quasar](https://quasar-lang.com/docs). A **maker** opens a fundraiser with a target amount and a deadline; **contributors** deposit tokens into a program-controlled vault. If the target is met the maker withdraws everything; if the deadline passes without the target being met, each contributor reclaims exactly what they put in.

See also: the [repository catalog](../../../README.md) and the [Anchor variant](../anchor/) of the same program.

## Major concepts

- The **Fundraiser** account is a PDA at `["fundraiser", maker]`. It stores the maker, the token's mint, the vault address, the target (`amount_to_raise`), the running total (`current_amount`), the Clock timestamp captured at creation (`time_started`), the window length in days (`duration`), and the PDA bump. Storing the vault address lets every later instruction bind the passed vault to this fundraiser with a `has_one(vault)` constraint.
- A **Contributor** account is a PDA at `["contributor", fundraiser, contributor]`. It records how much that signer has given to that fundraiser, plus its bump. The seeds bind the record to one (fundraiser, contributor) pair, so one contributor's record can never be spent by another signer or against another fundraiser.
- The **vault** is a token account whose authority is the Fundraiser PDA. All deposits, the maker payout, and refunds flow through it, with the PDA signing outbound transfers via its seeds.
- The **fundraising window** runs from `time_started` for `duration` days. Contributions are allowed while `now < time_started + duration`; refunds are allowed once `now >= time_started + duration` and only if the target was not met. `now` is the Clock sysvar's unix timestamp.

## Lifecycle

- `initialize` (maker signs): rejects a zero target (`InvalidAmount`) or zero duration (`InvalidDuration`), creates the Fundraiser PDA and the vault, and records the current Clock time as `time_started`.
- `contribute` (contributor signs): rejects a zero amount and, after the deadline, fails with `FundraiserEnded`. Creates the contributor's Contributor PDA on first use (idempotent init, contributor pays the rent), adds the amount to both `current_amount` and the contributor's record with checked arithmetic, transfers tokens from the contributor's token account into the vault, then verifies the vault gained exactly the contributed amount (`BalanceMismatch` otherwise).
- `check_contributions` (maker signs): fails with `TargetNotMet` unless `current_amount >= amount_to_raise`. Transfers the whole vault balance to the maker's token account with the Fundraiser PDA signing, then closes the vault and the Fundraiser account, returning their rent to the maker.
- `refund` (contributor signs): fails with `FundraiserNotEnded` before the deadline and with `TargetMet` if the fundraiser succeeded. Pays the contributor's recorded amount back from the vault with the PDA signing, subtracts it from `current_amount`, verifies the vault lost exactly that amount, and closes the Contributor account back to the contributor.

Errors are defined in `src/error.rs` as a `#[error_code]` enum starting at code 6000.

## Setup

From `finance/token-fundraiser/quasar/`:

```bash
quasar build
```

Prerequisites: [Quasar](https://quasar-lang.com/docs) CLI and [Agave](https://docs.anza.xyz/) toolchain (see `Quasar.toml`).

`quasar build` also regenerates the Rust client crate under `target/client/rust/`, which the tests use for typed instruction builders.

## Testing

In-process tests via **Quasar SVM** (`quasar-svm` in `Quasar.toml`):

```bash
quasar test
```

The tests in `src/tests.rs` drive the real instruction handlers end to end (initialize, contribute, check_contributions, refund), assert vault and contributor token balances plus account state after every step, and use `QuasarSvm::warp_to_timestamp` to test both sides of the deadline. They also cover the rejection paths: contributing after the deadline, refunding early or after a successful raise, paying out below target, passing a vault not bound to the fundraiser, and refunding against another contributor's record. No local validator is needed.
