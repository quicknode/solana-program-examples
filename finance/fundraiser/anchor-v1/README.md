# Solana Fundraiser (Anchor)

> [!NOTE]
> This is the **Anchor v1** copy of this example, on Anchor 1.2.0, the current
> stable Anchor release. Every `anchor` command on this page needs the v1 CLI:
> `avm install 1.2.0 && avm use 1.2.0`. The Anchor v2 version of this example is in
> [`../anchor`](../anchor/).

Onchain crowdfunding on Solana: a program that collects tokens toward a target amount, like Kickstarter without a payment processor. A **maker** creates a fundraiser [account](https://solana.com/docs/terminology#account), specifies the [mint](https://solana.com/docs/terminology#token-mint) they want to receive, the target amount, and a duration in days. **Contributors** contribute while the window is open. If the target is reached, the maker claims the funds, anyone closes each contributor's record to return its rent to that contributor, and the maker then closes the fundraiser; if it is not reached by the deadline, anyone can refund each contributor, and once refunds are complete the maker closes the fundraiser. Either way, the maker can then open a new one.

This example was called **Token Fundraiser** (`finance/token-fundraiser`) until it was renamed: contributors receive no token, only a refund if the target is missed.

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
    pub claimed: bool,
    pub open_contributor_accounts: u32,
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
- `claimed` - set by `check_contributions`. A claimed fundraiser accepts no more contributions and no second claim.
- `open_contributor_accounts` - how many Contributor accounts written for this fundraiser are still open. `contribute` adds one when it creates a Contributor account; `refund` and `close_contributor` each subtract one when they close one. `close_fundraiser` requires zero.
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

The Contributor PDA uses `init_if_needed`, which only runs the init branch on first call. On first init (when `bump == 0`) the handler stores `bumps.contributor_account` into `bump` and adds one to `open_contributor_accounts`; see [`instructions/contribute.rs`](programs/fundraiser/src/instructions/contribute.rs).

### Constants

From [`constants.rs`](programs/fundraiser/src/constants.rs):

```rust
pub const MIN_AMOUNT_TO_RAISE: u64 = 3;
pub const SECONDS_TO_DAYS: i64 = 86400;
```

`MIN_AMOUNT_TO_RAISE` is the minimum target in major units.

### Code layout

Each [instruction handler](https://solana.com/docs/terminology#instruction-handler) is a free function (`pub fn handle_<name>(accounts: &mut <Constraints>, ...)`) called from the `#[program]` module in `lib.rs`. The matching `#[derive(Accounts)]` struct (named `<Name>AccountConstraints`) sits in the same file as the handler.

### Token program compatibility

All token accounts use `anchor_spl::token_interface` types (`InterfaceAccount<Mint>`, `InterfaceAccount<TokenAccount>`, `Interface<TokenInterface>`), and every token movement uses `transfer_checked`, which carries the mint and decimals through the [CPI](https://solana.com/docs/terminology#cross-program-invocation-cpi). The same code works against the Classic Token Program and the Token Extensions Program.

### Onchain math

All balance and counter arithmetic uses `checked_*` operations and returns `FundraiserError::MathOverflow` on overflow. Both handlers that move tokens out of the vault update program state before issuing the transfer CPI (checks-effects-interactions).

## Lifecycle

### `initialize_fundraiser`

[`programs/fundraiser/src/instructions/initialize_fundraiser.rs`](programs/fundraiser/src/instructions/initialize_fundraiser.rs), account constraints `InitializeFundraiserAccountConstraints`.

The maker signs and pays for two new accounts:

- `fundraiser` - the state account, derived from `b"fundraiser"` and the maker's public key. Anchor calculates the canonical bump and the handler stores it.
- `vault` - the [ATA](https://solana.com/docs/terminology#associated-token-account-ata) that receives contributions, owned by the Fundraiser PDA.

The handler requires `amount >= MIN_AMOUNT_TO_RAISE * 10^decimals` (the target must be at least 3 major units of the mint, expressed in minor units), then initializes the Fundraiser state with `current_amount = 0`, `claimed = false`, `open_contributor_accounts = 0`, and `time_started` from the `Clock` sysvar. A target below the minimum fails with `InvalidAmount`.

### `contribute`

[`programs/fundraiser/src/instructions/contribute.rs`](programs/fundraiser/src/instructions/contribute.rs), account constraints `ContributeAccountConstraints`.

A contributor signs and the handler performs three checks in order:

1. Minimum contribution: `amount >= 10^decimals` (one major unit of the mint), else `ContributionTooSmall`.
2. Not yet claimed: `!claimed`, else `FundraiserClaimed`. The deadline may still be days away after a claim, and the vault has already been paid out.
3. Time window: contributions are allowed while `elapsed_days < duration`, where `elapsed_days = (now - time_started) / SECONDS_TO_DAYS`. Once `elapsed_days` reaches `duration` the handler fails with `FundraiserEnded`.

If all checks pass, `Fundraiser.current_amount` and `Contributor.amount` are updated (a contributor's later contributions add to the same Contributor account), then `amount` is transferred from `contributor_ata` to `vault` with `transfer_checked`.

### `check_contributions`

[`programs/fundraiser/src/instructions/checker.rs`](programs/fundraiser/src/instructions/checker.rs), account constraints `CheckContributionsAccountConstraints`.

Lets the maker claim the funds once the target is met. Requires `!claimed`, else `FundraiserClaimed`, and `fundraiser.current_amount >= amount_to_raise` (the state-tracked total, so direct donations to the vault cannot unlock the claim early), else `TargetNotMet`. The handler sets `claimed` and transfers the entire vault balance (including any direct donations) to `maker_ata` with `transfer_checked`, signed with the Fundraiser PDA's seeds.

The Fundraiser account and the empty vault stay open. Contributor accounts are derived from the Fundraiser's address, so the Fundraiser must outlive every one of them: if it closed here, the maker could initialize a new fundraiser at the same address, and the Contributor accounts left over from this raise would count as contributions to the new one, so `refund` would pay their old amounts out of the new contributors' tokens. `close_contributor` closes the Contributor accounts, then `close_fundraiser` closes the Fundraiser and the vault.

### `refund`

[`programs/fundraiser/src/instructions/refund.rs`](programs/fundraiser/src/instructions/refund.rs), account constraints `RefundAccountConstraints`.

Returns a contribution after a failed fundraiser. The contributor does not have to sign: the tokens go to their token account and the rent to them, whoever sends the transaction, so the maker can refund every contributor and close a failed fundraiser without waiting on any of them. Two checks:

1. Refunds are allowed only after the fundraiser has ended: `elapsed_days >= duration`, else `FundraiserNotEnded`.
2. The target was not met: `fundraiser.current_amount < amount_to_raise` (again the state-tracked total, so donated tokens cannot block refunds), else `TargetMet`.

The handler subtracts the contributor's recorded amount from `current_amount` zeroes the Contributor record, and subtracts one from `open_contributor_accounts` before the transfer CPI, then sends the tokens from the vault back to `contributor_ata` with `transfer_checked` (PDA signer). The Contributor account is closed via `close = contributor`, refunding its rent to the contributor.

### `close_fundraiser`

[`programs/fundraiser/src/instructions/close.rs`](programs/fundraiser/src/instructions/close.rs), account constraints `CloseFundraiserAccountConstraints`.

Closes a finished fundraiser and its vault so the maker can raise again. The Fundraiser PDA is derived from `b"fundraiser"` and the maker's public key alone, so while a Fundraiser account exists the maker cannot initialize another one.

For an unclaimed fundraiser, three checks:

1. The fundraiser has ended: `elapsed_days >= duration`, else `FundraiserNotEnded`.
2. The target was not met: `fundraiser.current_amount < amount_to_raise`, else `TargetMet` (a raise that met its target closes after the maker claims it).
3. Every contribution has been refunded: `fundraiser.current_amount == 0`, else `RefundsOutstanding` (closing the vault earlier would strand the remaining refunds).

For every fundraiser, claimed or not: `open_contributor_accounts == 0`, else `ContributorAccountsOpen`. A Contributor account left open would be read as a contribution to the next fundraiser at this address.

Anything still in the vault at this point is a direct donation outside the program's accounting; the handler pays it to `maker_ata` with `transfer_checked` rather than burning it, then closes the vault with `close_account` (both CPIs signed with the Fundraiser PDA's seeds). The Fundraiser state account is closed via `close = maker`.

### `close_contributor`

[`programs/fundraiser/src/instructions/close_contributor.rs`](programs/fundraiser/src/instructions/close_contributor.rs), account constraints `CloseContributorAccountConstraints`.

Closes a Contributor account once its fundraiser has been claimed, returning the rent to the contributor. On a successful raise the contribution has been paid out to the maker, so the account holds only rent, and `close_fundraiser` cannot run until every one of them is closed.

One check: `fundraiser.claimed`, else `FundraiserNotClaimed`. While the fundraiser is unclaimed the contribution can still be refunded, so `refund` is the way to close it. The contributor does not have to sign: the rent goes to them whoever sends the transaction, so the maker can close every Contributor account and then the fundraiser. The Contributor account's seeds bind it to the fundraiser's address, and the handler subtracts one from `open_contributor_accounts`. The account is closed via `close = contributor`.

## Testing

The tests are Rust integration tests using [LiteSVM](https://www.anchor-lang.com/docs/testing/litesvm) and [solana-kite](https://crates.io/crates/solana-kite), in [`programs/fundraiser/tests/test_fundraiser.rs`](programs/fundraiser/tests/test_fundraiser.rs). They load the compiled program with `include_bytes!`, so build the program first and rebuild after every program change:

```sh
cargo build-sbf
cargo test
```

The suite uses a nonzero duration and warps the LiteSVM `Clock` sysvar to exercise both sides of every deadline: contributing inside the window succeeds, contributing after the deadline fails, refunding before the deadline fails, and refunding after the deadline succeeds when the target was not met. Every failing case asserts the specific program error.

It checks that the claim pays the maker and marks the fundraiser claimed, that a second claim and a contribution after the claim are refused, that direct vault donations do not unlock the claim, and that anyone can refund a contributor or close their Contributor account after a claim, with the tokens and rent going to the contributor.

`test_stale_contributor_account_cannot_refund_from_next_raise` runs the attack the open-account count exists to stop: a raise succeeds, its Contributor accounts and the fundraiser are closed, the maker starts a second raise at the same address, and a first-raise contributor's refund from the second raise fails while every second-raise contributor gets back exactly what they put in. `test_reinitialize_with_open_contributor_accounts_fails` and `test_close_fundraiser_with_open_contributor_accounts_fails` check that the second raise cannot start while any first-raise Contributor account is open.

`close_fundraiser` is tested on both paths (after a failed raise, only after the deadline, only when the target was missed and refunds are complete; after a claim, only once every Contributor account is closed), including that it pays direct donations to the maker and that the same maker can then initialize a fresh fundraiser. Assertions check token balances and decoded account state rather than just transaction success.

## FAQ

### How do I build crowdfunding on Solana?

A maker opens a fundraiser with `initialize_fundraiser`, naming the token, target amount, and duration. Contributors deposit with `contribute` while the window is open, and the funds sit in a program-controlled vault that neither side can raid. When the target is reached, the maker claims the raise with `check_contributions`, which pays out the vault and marks the fundraiser claimed.

### What happens to the contributor accounts after a successful raise?

Each stays open with its rent inside until someone calls `close_contributor`, which checks that the fundraiser has been claimed and returns the rent to the contributor. Anyone can send it, so the maker can close them all, then call `close_fundraiser` to close the Fundraiser account and the vault.

### What happens if the fundraiser misses its target?

After the deadline, `refund` returns each contributor exactly what they put in; anyone can send it. Once refunds are complete, the maker calls `close_fundraiser` to retire the failed raise and can then open a new one.

### How is this fundraiser tested and verified?

`anchor build` then `cargo test` runs LiteSVM tests that warp the clock across the deadline to exercise contribution windows, claims, refunds, and closing on both paths. The arithmetic has [Kani](https://github.com/model-checking/kani) model checks in [`../kani-proofs/`](../kani-proofs/).
