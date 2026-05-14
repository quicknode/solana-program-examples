# Token Fundraiser

Create a fundraiser that collects tokens. A user creates a fundraiser account, specifies the mint they want to receive, the target amount, and a duration. Other users contribute. If the target is reached, the maker can claim the funds; if it isn't reached within the duration, contributors can refund.

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

- `maker` — the person starting the fundraiser.
- `mint_to_raise` — the mint the maker wants to receive.
- `amount_to_raise` — the target amount.
- `current_amount` — total amount currently contributed.
- `time_started` — when the fundraiser was created.
- `duration` — fundraising window in days.
- `bump` — canonical bump for the Fundraiser PDA.

The `InitSpace` derive macro implements the `Space` trait, which calculates the size of the account (not counting the Anchor discriminator).

A per-contributor record:

```rust
#[account]
#[derive(InitSpace)]
pub struct Contributor {
    pub amount: u64,
    pub bump: u8,
}
```

- `amount` — total amount contributed by this contributor.
- `bump` — canonical bump for the Contributor PDA.

The Contributor PDA uses `init_if_needed`, which only runs the init branch on first call. The handler stores `bumps.contributor_account` into `bump` on first init (when `bump == 0`); see [`instructions/contribute.rs`](programs/fundraiser/src/instructions/contribute.rs).

### Constants

From [`constants.rs`](programs/fundraiser/src/constants.rs):

```rust
pub const MIN_AMOUNT_TO_RAISE: u64 = 3;
pub const SECONDS_TO_DAYS: i64 = 86400;
pub const MAX_CONTRIBUTION_PERCENTAGE: u64 = 10;
pub const PERCENTAGE_SCALER: u64 = 100;
```

`MAX_CONTRIBUTION_PERCENTAGE / PERCENTAGE_SCALER` = 10%, the per-contributor cap.

### Code layout

Each instruction handler is a free function (`pub fn handle_<name>(accounts: &mut <Context>, ...)`) called from the `#[program]` module in `lib.rs`. Account-validation structs sit in the same file as the handler.

## Instruction handlers

### `initialize`

[`programs/fundraiser/src/instructions/initialize.rs`](programs/fundraiser/src/instructions/initialize.rs).

```rust
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub maker: Signer<'info>,
    pub mint_to_raise: Account<'info, Mint>,
    #[account(
        init,
        payer = maker,
        seeds = [b"fundraiser", maker.key().as_ref()],
        bump,
        space = Fundraiser::DISCRIMINATOR.len() + Fundraiser::INIT_SPACE,
    )]
    pub fundraiser: Account<'info, Fundraiser>,
    #[account(
        init,
        payer = maker,
        associated_token::mint = mint_to_raise,
        associated_token::authority = fundraiser,
    )]
    pub vault: Account<'info, TokenAccount>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}
```

Account breakdown:

- `maker` — the person starting the fundraiser. Signs; mutable so we can deduct lamports.
- `mint_to_raise` — the mint the maker wants to receive.
- `fundraiser` — the state account. Derived from `b"fundraiser"` and the maker's public key; Anchor calculates the canonical bump and stores it in the struct.
- `vault` — the ATA that receives contributions, owned by the Fundraiser PDA.
- `system_program`, `token_program`, `associated_token_program` — needed to initialize the new accounts.

The handler requires `amount >= MIN_AMOUNT_TO_RAISE.pow(mint.decimals)` and initializes the Fundraiser state.

### `contribute`

[`programs/fundraiser/src/instructions/contribute.rs`](programs/fundraiser/src/instructions/contribute.rs).

Account-validation struct: see source. The handler performs four `require!` checks in order:

1. `amount >= 1_u64.pow(mint.decimals)` — minimum contribution (this is `1`, since `1.pow(n) == 1`; effectively contributions just need to be non-zero).
2. `amount <= amount_to_raise * MAX_CONTRIBUTION_PERCENTAGE / PERCENTAGE_SCALER` — per-call cap of 10% of the target.
3. `fundraiser.duration <= (current_time - time_started) / SECONDS_TO_DAYS` — see the [duration semantics note](#duration-check-semantics) below.
4. Cumulative contributor cap: this contributor's running total (existing + new) must not exceed 10% of the target.

If all four checks pass, tokens are transferred from `contributor_ata` to `vault` via a CPI to the Classic Token Program, and both `Fundraiser.current_amount` and `Contributor.amount` are updated.

### `check_contributions`

[`programs/fundraiser/src/instructions/checker.rs`](programs/fundraiser/src/instructions/checker.rs).

Lets the maker claim the funds. Requires `vault.amount >= amount_to_raise`. The CPI uses `new_with_signer` with the Fundraiser PDA's seeds because the vault is owned by the PDA. The Fundraiser account is closed (via the `close = maker` constraint) and its rent is refunded to the maker.

### `refund`

[`programs/fundraiser/src/instructions/refund.rs`](programs/fundraiser/src/instructions/refund.rs).

Lets a contributor reclaim their contribution if the target wasn't met. Two checks:

1. `fundraiser.duration >= (current_time - time_started) / SECONDS_TO_DAYS` — see the [duration semantics note](#duration-check-semantics) below.
2. `vault.amount < amount_to_raise` — target not met.

Then the vault's tokens are transferred back to the contributor's ATA (CPI with PDA signer seeds) and the Contributor account is closed (via `close = contributor`), refunding its rent to the contributor.

## Duration check semantics

The `contribute` and `refund` handlers compare `fundraiser.duration` (a `u16` in *days*) against elapsed days since `time_started`. The two checks use opposite comparison operators, which is worth reading carefully:

- `contribute`: `require!(duration <= elapsed_days, FundraiserEnded)` — fails (with `FundraiserEnded`) when `elapsed_days < duration`.
- `refund`: `require!(duration >= elapsed_days, FundraiserNotEnded)` — fails (with `FundraiserNotEnded`) when `elapsed_days > duration`.

> ⚠️ Both comparisons look inverted relative to their error names. If you adapt this code, audit the duration logic carefully before relying on it.
