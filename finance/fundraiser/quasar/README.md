# Solana Fundraiser (Quasar)

Onchain crowdfunding on Solana toward a target amount in a chosen token, written with [Quasar](https://quasar-lang.com/docs). A **maker** opens a fundraiser with a target amount and a deadline; **contributors** deposit tokens into a program-controlled vault. If the target is met the maker withdraws everything; if the deadline passes without the target being met, each contributor gets back exactly what they put in. Either way, once every contributor's record is closed the maker closes the fundraiser and can open another.

This example was called **Token Fundraiser** (`finance/token-fundraiser`) until it was renamed: contributors receive no token, only a refund if the target is missed.

See also: the [repository catalog](../../../README.md) and the [Anchor variant](../anchor/) of the same program.

## Major concepts

- The **Fundraiser** account is a PDA at `["fundraiser", maker]`. It stores the maker, the token's mint, the vault address, the target (`amount_to_raise`), the running total (`current_amount`), the Clock timestamp captured at creation (`time_started`), the window length in days (`duration`), whether the maker has claimed it (`claimed`), how many Contributor accounts written for it are still open (`open_contributor_accounts`), and the PDA bump. Storing the vault address lets every later instruction bind the passed vault to this fundraiser with a `has_one(vault)` constraint.
- A **Contributor** account is a PDA at `["contributor", fundraiser, contributor]`. It records how much that contributor has given to that fundraiser, plus its bump. The seeds bind the record to one (fundraiser, contributor) pair, so one contributor's record can never be paid to anyone else or against another fundraiser.
- The **vault** is a token account whose authority is the Fundraiser PDA. All deposits, the maker payout, and refunds flow through it, with the PDA signing outbound transfers via its seeds.
- **The Fundraiser outlives every Contributor account.** A Contributor PDA is derived from the Fundraiser's address, and the Fundraiser's address from the maker alone. If the Fundraiser could close while a Contributor account was open, the maker could initialize a new fundraiser at the same address and that Contributor account would count as a contribution to it, so `refund` would pay its old amount out of the new contributors' tokens. `open_contributor_accounts` counts them, and `close_fundraiser` requires zero.
- The **fundraising window** runs from `time_started` for `duration` days. Contributions are allowed while `now < time_started + duration`; refunds are allowed once `now >= time_started + duration` and only if the target was not met. `now` is the Clock sysvar's unix timestamp.

## Lifecycle

- `initialize_fundraiser` (maker signs): rejects a zero target (`InvalidAmount`) or zero duration (`InvalidDuration`), creates the Fundraiser PDA and the vault, and records the current Clock time as `time_started`, with `claimed` false and `open_contributor_accounts` zero.
- `contribute` (contributor signs): rejects a zero amount, fails with `FundraiserClaimed` once the maker has claimed, and fails with `FundraiserEnded` after the deadline. Creates the contributor's Contributor PDA on first use (idempotent init, contributor pays the rent) and adds one to `open_contributor_accounts`, adds the amount to both `current_amount` and the contributor's record with checked arithmetic, transfers tokens from the contributor's token account into the vault, then verifies the vault gained exactly the contributed amount (`BalanceMismatch` otherwise).
- `check_contributions` (maker signs): fails with `FundraiserClaimed` if already claimed and with `TargetNotMet` unless `current_amount >= amount_to_raise`. Sets `claimed` and transfers the whole vault balance to the maker's token account with the Fundraiser PDA signing. The Fundraiser and the empty vault stay open until every Contributor account is closed.
- `refund` (anyone may send it; the contributor does not sign): fails with `FundraiserNotEnded` before the deadline and with `TargetMet` if the fundraiser succeeded. Pays the contributor's recorded amount back from the vault to the contributor's own token account (checked against the contributor's address) with the PDA signing, subtracts it from `current_amount` and one from `open_contributor_accounts`, verifies the vault lost exactly that amount, and closes the Contributor account, returning its rent to the contributor. The tokens and rent can only reach the contributor, so the maker can refund everyone without waiting on them.
- `close_contributor` (anyone may send it; the contributor does not sign): fails with `FundraiserNotClaimed` until the maker has claimed, because before then the contribution can still be refunded. Closes the Contributor account, returning its rent to the contributor, and subtracts one from `open_contributor_accounts`. The Contributor account's seeds bind it to the fundraiser address, so no other fundraiser can be substituted.
- `close_fundraiser` (maker signs): for an unclaimed fundraiser, fails with `FundraiserNotEnded` before the deadline, `TargetMet` if the target was met, and `RefundsOutstanding` while `current_amount` is above zero. For any fundraiser, fails with `ContributorAccountsOpen` while `open_contributor_accounts` is above zero. Pays any tokens left in the vault (direct transfers outside `contribute`) to the maker's token account, then closes the vault and the Fundraiser account, returning their rent to the maker.

Errors are defined in `src/error.rs` as a `#[error_code]` enum starting at code 6000.

## Setup

From `finance/fundraiser/quasar/`:

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

The tests in `src/tests.rs` drive the real instruction handlers end to end (initialize_fundraiser, contribute, check_contributions, refund, close_contributor, close_fundraiser), assert vault and contributor token balances plus account state after every step, and use `QuasarSvm::warp_to_timestamp` to test both sides of the deadline. They also cover the rejection paths: contributing after the deadline, refunding early or after a successful raise, paying out below target, passing a vault not bound to the fundraiser, refunding against another contributor's record, closing a contributor account before the claim, and closing the fundraiser early, with contributions unrefunded, or with Contributor accounts open. `stale_contributor_account_cannot_refund_from_next_raise` (see `src/tests.rs`) runs two raises at the same address and checks that a first-raise contributor cannot take a refund from the second. No local validator is needed.
