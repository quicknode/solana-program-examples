# Managed-fund: Kani model checks

Kani harnesses that model-check the ERC4626-style share vault, in the spirit
of [`aeyakovenko/percolator`](https://github.com/aeyakovenko/percolator), which
uses the [Kani](https://github.com/model-checking/kani) model checker to check
the arithmetic of a DeFi engine. Kani marks a harness with `#[kani::proof]`,
which is why the crate is `kani-proofs` and the harnesses are named `proof_*`;
each one is a model check: Kani tries every value of the inputs the harness
declares and reports either that every assertion held or an input that breaks
one.

## What is checked

Depositors mint share tokens against the fund's net asset value; withdrawals
burn shares for a proportional slice of every vault balance; a manager fee mints
a small slice of shares over time. Token movement is via SPL CPIs Kani cannot
symbolically execute, but the share math is pure integer arithmetic. For every
input, the harnesses check:

- `proof_withdraw_within_balance`: **Solvency**: a withdrawal never takes more of any vault balance than it holds (`floor(balance·shares/total) <= balance`, since `shares <= total`); burning the whole supply takes exactly the whole balance.
- `proof_deposit_withdraw_cannot_extract`: A deposit→withdraw round-trip never returns more than was deposited, no rounding attack mints shares worth more than they cost.
- `proof_recorded_holdings_never_exceed_balance`: The program prices shares and pays withdrawals from its recorded holdings, not vault balances. Across a deposit, a donation, and a withdrawal, the recorded holding never exceeds the vault's real balance, so every payout is covered however much is donated.
- `proof_donation_cannot_dilute_next_deposit`: The inflation attack modelled directly: after an attacker's first deposit and a donation of any size, the victim's deposit mints exactly one share per minor unit and withdraws in full.
- `proof_fee_shares_bounded_by_supply`: The time-based manager fee, `ceil(total_shares·fee_bps·elapsed/(10000·seconds_per_year))`, can never mint more than 100%/year of dilution (`fee_shares <= total_shares` for `elapsed <= 1yr`, `fee_bps <= 10000`), and rounds up: it is never below the exact quotient and never more than one share above it.
- `proof_deposit_nav_rounds_against_the_depositor`: Deposit values each asset rounding up. The rounded-up value is never below the floored one and at most one minor unit above it, and a deposit priced against it mints no more shares than one priced against the floored NAV, so valuation rounding never hands a depositor a share the holders paid for.
- `proof_sell_floor_rounds_in_the_funds_favour`: Rebalance refuses a sale that pays less than its slippage tolerance keeps of the oracle value sold. That floor is the exact product rounded up in one division: never below the exact figure, under one minor unit above it, and never below the floor taken by flooring the value and then the tolerance's share, which could accept a sale a fraction of a minor unit short.

## Bounded model checking

The nonlinear harnesses check 128-bit arithmetic with a symbolic divisor (the share
supply / NAV), so (as percolator does) they bound their symbolic inputs to a
representative range; the share identities are scale-invariant.

- `proof_withdraw_within_balance`: balances/supply `<= 255`, runs in ~12s
- `proof_deposit_withdraw_cannot_extract`: `<= 31`, runs in ~3s
- `proof_recorded_holdings_never_exceed_balance`: balances and supply `<= 255`
- `proof_donation_cannot_dilute_next_deposit`: deposits `<= 31`, donation unbounded
- `proof_fee_shares_bounded_by_supply`: `<= 255`, runs in ~4s
- `proof_deposit_nav_rounds_against_the_depositor`: amounts and prices `<= 255`, deposits and supply `<= 31`, runs in ~3 minutes
- `proof_sell_floor_rounds_in_the_funds_favour`: amounts and prices `<= 255`, tolerance 90% to 100%, the scale fixed at `10^-3` (a symbolic power leaves the solver dividing by a symbolic denominator, and it does not finish), runs in ~2.5 minutes

Run weekly in CI (the `kani.yml` `verify` job), not on every push/PR, because
the bounded nonlinear model checks are slow. A fast unit-test job runs per push/PR.

## Running

```bash
cargo test                                                 # unit tests, no Kani
cargo install --locked kani-verifier && cargo kani setup   # one-time
cargo kani                                                  # Kani model checks
```
