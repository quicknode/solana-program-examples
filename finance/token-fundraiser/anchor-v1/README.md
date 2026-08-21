# Solana Token Fundraiser (Anchor)

> [!NOTE]
> This is the **Anchor v1** copy of this example, kept for programs staying on the
> Anchor v1 LTS line. Every `anchor` command on this page needs the v1 CLI:
> `avm install 1.1.2 && avm use 1.1.2`. The Anchor v2 version of this example is in
> [`../anchor`](../anchor/).

Onchain crowdfunding on Solana: a program that collects tokens toward a target amount, like Kickstarter without a payment processor. A **maker** creates a fundraiser [account](https://solana.com/docs/terminology#account), specifies the [mint](https://solana.com/docs/terminology#token-mint) they want to receive, the target amount, and a duration in days. **Contributors** contribute while the window is open. If the target is reached, the maker claims the funds; if it is not reached by the deadline, contributors can refund, and once refunds are complete the maker can retire the fundraiser and open a new one.

## Architecture

The fundraiser state account:

```rust
#[account]
#[derive(InitSpace)]
pub struct Fundraiser {
    pub maker: Pubkey,
    pub mint_to_raise: Pubkey,
    pub amount_to_raise: u64,
    pub current_amount: u64,
    pub time_started: i64,
    pub duration: u16,
    pub bump: u8,
}
```

Fields:

- `maker` - the person starting the fundraiser.
- `mint_to_raise` - the mint the maker wants to receive.
- `amount_to_raise` - the target amount, in minor units.
- `current_amount` - total amount contributed through the `contribute` handler. This tracked total, not the vault balance, is what `check_contributions` and `refund` compare against the target, so tokens sent directly to the vault cannot trigger an early release or block refunds.
- `time_started` - when the fundraiser was created.
- `duration` - fundraising window in days.
- `bump` - canonical bump for the Fundraiser [PDA](https://solana.com/docs/terminology#program-derived-address-pda).

The `InitSpace` derive macro implements the `Space` trait, which calculates the size of the account (not counting the [Anchor](https://solana.com/docs/terminology#anchor) discriminator).

A per-contributor record:

```rust
#[account]
#[derive(InitSpace)]
pub struct Contributor {
    pub amount: u64,
    pub bump: u8,
}
```

- `amount` - total amount contributed by this contributor.
- `bump` - canonical bump for the Contributor PDA.

The Contributor PDA uses `init_if_needed`, which only runs the init branch on first call. The handler stores `bumps.contributor_account` into `bump` on first init (when `bump == 0`); see [`instructions/contribute.rs`](programs/fundraiser/src/instructions/contribute.rs).

### Constants

From [`constants.rs`](programs/fundraiser/src/constants.rs):

```rust
pub const MIN_AMOUNT_TO_RAISE: u64 = 3;
pub const SECONDS_TO_DAYS: i64 = 86400;
pub const MAX_CONTRIBUTION_PERCENTAGE: u64 = 10;
pub const PERCENTAGE_SCALER: u64 = 100;
```

`MAX_CONTRIBUTION_PERCENTAGE / PERCENTAGE_SCALER` = 10%, the per-contributor cap. `MIN_AMOUNT_TO_RAISE` is the minimum target in major units.

### Code layout

Each [instruction handler](https://solana.com/docs/terminology#instruction-handler) is a free function (`pub fn handle_<name>(accounts: &mut <Constraints>, ...)`) called from the `#[program]` module in `lib.rs`. The matching `#[derive(Accounts)]` struct (named `<Name>AccountConstraints`) sits in the same file as the handler.

### Token program compatibility

All token accounts use `anchor_spl::token_interface` types (`InterfaceAccount<Mint>`, `InterfaceAccount<TokenAccount>`, `Interface<TokenInterface>`), and every token movement uses `transfer_checked`, which carries the mint and decimals through the [CPI](https://solana.com/docs/terminology#cross-program-invocation-cpi). The same code works against the Classic Token Program and the Token Extensions Program.

### Onchain math

All balance arithmetic uses `checked_*` operations and returns `FundraiserError::MathOverflow` on overflow. The per-contributor cap is computed in `u128` so the percentage product cannot overflow `u64`. Both handlers that move tokens out of the vault update program state before issuing the transfer CPI (checks-effects-interactions).

## Lifecycle

### `initialize`

[`programs/fundraiser/src/instructions/initialize.rs`](programs/fundraiser/src/instructions/initialize.rs), account constraints `InitializeFundraiserAccountConstraints`.

The maker signs and pays for two new accounts:

- `fundraiser` - the state account, derived from `b"fundraiser"` and the maker's public key. Anchor calculates the canonical bump and the handler stores it.
- `vault` - the [ATA](https://solana.com/docs/terminology#associated-token-account-ata) that receives contributions, owned by the Fundraiser PDA.

The handler requires `amount >= MIN_AMOUNT_TO_RAISE * 10^decimals` (the target must be at least 3 major units of the mint, expressed in minor units), then initializes the Fundraiser state with `current_amount = 0` and `time_started` from the `Clock` sysvar. A target below the minimum fails with `InvalidAmount`.

### `contribute`

[`programs/fundraiser/src/instructions/contribute.rs`](programs/fundraiser/src/instructions/contribute.rs), account constraints `ContributeAccountConstraints`.

A contributor signs and the handler performs four checks in order:

1. Minimum contribution: `amount >= 10^decimals` (one major unit of the mint), else `ContributionTooSmall`.
2. Per-call cap: `amount <= amount_to_raise * MAX_CONTRIBUTION_PERCENTAGE / PERCENTAGE_SCALER` (10% of the target), else `ContributionTooBig`.
3. Time window: contributions are allowed while `elapsed_days < duration`, where `elapsed_days = (now - time_started) / SECONDS_TO_DAYS`. Once `elapsed_days` reaches `duration` the handler fails with `FundraiserEnded`.
4. Cumulative cap: the contributor's running total (existing + new) must not exceed the same 10% cap, else `MaximumContributionsReached`.

If all checks pass, `Fundraiser.current_amount` and `Contributor.amount` are updated, then `amount` is transferred from `contributor_ata` to `vault` with `transfer_checked`.

### `check_contributions`

[`programs/fundraiser/src/instructions/checker.rs`](programs/fundraiser/src/instructions/checker.rs), account constraints `CheckContributionsAccountConstraints`.

Lets the maker claim the funds once the target is met. Requires `fundraiser.current_amount >= amount_to_raise` (the state-tracked total, so direct donations to the vault cannot unlock the claim early), else `TargetNotMet`. The handler then, signing both CPIs with the Fundraiser PDA's seeds:

1. Transfers the entire vault balance (including any direct donations) to `maker_ata` with `transfer_checked`.
2. Closes the empty vault token account with `close_account`, returning its rent to the maker.

The Fundraiser state account is closed via the `close = maker` constraint, so the maker also recovers that [rent](https://solana.com/docs/terminology#rent).

### `refund`

[`programs/fundraiser/src/instructions/refund.rs`](programs/fundraiser/src/instructions/refund.rs), account constraints `RefundAccountConstraints`.

Lets a contributor reclaim their contribution after a failed fundraiser. Two checks:

1. Refunds are allowed only after the fundraiser has ended: `elapsed_days >= duration`, else `FundraiserNotEnded`.
2. The target was not met: `fundraiser.current_amount < amount_to_raise` (again the state-tracked total, so donated tokens cannot block refunds), else `TargetMet`.

The handler subtracts the contributor's recorded amount from `current_amount` and zeroes the Contributor record before the transfer CPI, then sends the tokens from the vault back to `contributor_ata` with `transfer_checked` (PDA signer). The Contributor account is closed via `close = contributor`, refunding its rent to the contributor.

### `close_fundraiser`

[`programs/fundraiser/src/instructions/close.rs`](programs/fundraiser/src/instructions/close.rs), account constraints `CloseFundraiserAccountConstraints`.

Retires a failed fundraiser so the maker can raise again. The Fundraiser PDA is derived from `b"fundraiser"` and the maker's public key alone, so while a failed fundraiser's account exists the maker can never initialize another one. Three checks:

1. The fundraiser has ended: `elapsed_days >= duration`, else `FundraiserNotEnded`.
2. The target was not met: `fundraiser.current_amount < amount_to_raise`, else `TargetMet` (a successful raise exits through `check_contributions`, which already closes these accounts).
3. Every contribution has been refunded: `fundraiser.current_amount == 0`, else `RefundsOutstanding` (closing the vault earlier would strand the remaining refunds).

Anything still in the vault at this point is a direct donation outside the program's accounting; the handler sweeps it to `maker_ata` with `transfer_checked` rather than burning it, then closes the vault with `close_account` (both CPIs signed with the Fundraiser PDA's seeds). The Fundraiser state account is closed via `close = maker`.

## Testing

The tests are Rust integration tests using [LiteSVM](https://www.anchor-lang.com/docs/testing/litesvm) and [solana-kite](https://crates.io/crates/solana-kite), in [`programs/fundraiser/tests/test_fundraiser.rs`](programs/fundraiser/tests/test_fundraiser.rs). They load the compiled program with `include_bytes!`, so build the program first and rebuild after every program change:

```sh
cargo build-sbf
cargo test
```

The suite uses a nonzero duration and warps the LiteSVM `Clock` sysvar to exercise both sides of every deadline: contributing inside the window succeeds, contributing after the deadline fails, refunding before the deadline fails, and refunding after the deadline succeeds when the target was not met. It exercises both contribution caps (a single contribution over the 10% cap, and contributions that cumulatively exceed it), and verifies that the claim pays the maker and closes the vault, that direct vault donations do not unlock the claim, and that `close_fundraiser` retires a failed raise (only after the deadline, only when the target was missed, only once refunds are complete, sweeping direct donations to the maker) and lets the same maker initialize a fresh fundraiser. Assertions check token balances and decoded account state rather than just transaction success.

## FAQ

### How do I build crowdfunding on Solana?

A maker opens a fundraiser with `initialize`, naming the token, target amount, and duration. Contributors deposit with `contribute` while the window is open, and the funds sit in a program-controlled vault that neither side can raid. When the target is reached, the maker claims the raise with `check_contributions`, which pays out the vault and closes the fundraiser.

### What happens if the fundraiser misses its target?

Contributors call `refund` after the deadline to reclaim exactly what they put in. Once refunds are complete, the maker calls `close_fundraiser` to retire the failed raise and can then open a new one.

### How is this fundraiser tested and verified?

`anchor build` then `cargo test` runs LiteSVM tests that warp the clock across the deadline to exercise contribution windows, per-contributor caps, claims, refunds, and closing. The money math has [Kani](https://github.com/model-checking/kani) proofs in [`../kani-proofs/`](../kani-proofs/).
