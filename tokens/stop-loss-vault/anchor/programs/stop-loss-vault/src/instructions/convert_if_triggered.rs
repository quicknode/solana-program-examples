use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::Discriminator;
use anchor_spl::token::{Mint, Token, TokenAccount};

use crate::constants::{ANCHOR_DISCRIMINATOR_LENGTH, MOCK_FEED_PAYLOAD_LENGTH};
use crate::error::StopLossError;
use crate::state::Vault;

/// Permissionless: anyone can crank this — typically a TukTuk worker. Reads
/// the Switchboard On-Demand feed, compares to `vault.threshold_price`, and
/// if (and only if) the latest price has fallen at or below the threshold,
/// CPIs Jupiter (mock or real) with the vault's entire volatile balance.
///
/// # Known limitation: flash-crash gap between cranks
///
/// This is a discrete-time stop-loss. The crank runs every
/// `crank_interval_seconds` (default 600s, set by TukTuk). If the price
/// crashes through the threshold AND recovers above it between two
/// consecutive cranks, this instruction will never see the crash and will
/// not convert. That is the cost of doing stop-loss permissionlessly without
/// continuous orderbook monitoring; the README's "Limitations" section walks
/// through the tradeoff and `test_flash_crash_between_cranks_misses_trigger`
/// demonstrates it explicitly. Pick `crank_interval_seconds` accordingly.
///
/// Real Switchboard On-Demand price updates are passed in the
/// `switchboard_price_update_data` argument and verified onchain via Ed25519.
/// In this teaching example the mock feed is read directly from the account
/// data, so the argument is accepted but not yet wired to verification.
pub fn handler(
    ctx: Context<ConvertIfTriggeredAccountConstraints>,
    _switchboard_price_update_data: Vec<u8>,
) -> Result<()> {
    require!(
        !ctx.accounts.vault.triggered,
        StopLossError::VaultAlreadyTriggered
    );

    // Read the oracle price out of the feed account.
    //
    // TODO: replace this direct-read with Switchboard On-Demand's verified-
    // update path. The production handler should call
    // `switchboard_on_demand::PullFeedAccountData::parse_and_verify(...)` over
    // `_switchboard_price_update_data` so the on-chain logic only trusts
    // signed price updates. For tests we read the mock layout directly:
    let feed_account = &ctx.accounts.oracle_feed;
    require_keys_eq!(
        feed_account.key(),
        ctx.accounts.vault.oracle_feed,
        StopLossError::Unauthorized
    );
    let feed_data = feed_account.try_borrow_data()?;
    require!(
        feed_data.len() >= ANCHOR_DISCRIMINATOR_LENGTH + MOCK_FEED_PAYLOAD_LENGTH,
        StopLossError::FeedDataTooShort
    );
    let payload = &feed_data[ANCHOR_DISCRIMINATOR_LENGTH
        ..ANCHOR_DISCRIMINATOR_LENGTH + MOCK_FEED_PAYLOAD_LENGTH];
    // 32 (authority) + price (16) + scale (4) + last_update_slot (8).
    let price_bytes: [u8; 16] = payload[32..48]
        .try_into()
        .map_err(|_| StopLossError::FeedDataTooShort)?;
    let price = i128::from_le_bytes(price_bytes);
    drop(feed_data);

    require!(price > 0, StopLossError::NonPositivePrice);

    // The fire condition: price strictly below threshold. Equality keeps the
    // vault armed — slightly lazy, but it means the test `price_above_threshold`
    // case includes the boundary cleanly.
    require!(
        price < ctx.accounts.vault.threshold_price,
        StopLossError::PriceAboveThreshold
    );

    // CPI into the swap aggregator with the vault's full volatile balance.
    let in_amount = ctx.accounts.vault_volatile_account.amount;
    require!(in_amount > 0, StopLossError::EmptyVault);

    // Build the Jupiter v6 `shared_accounts_route` instruction. We construct
    // it by hand (rather than via the `mock_jupiter` cpi module) because the
    // production version of this handler will target the *real* Jupiter
    // program ID and the real `shared_accounts_route` discriminator — the
    // only thing that has to change is the program ID in the client and the
    // discriminator below. For now both come from `mock_jupiter`.
    let mock_jupiter_program_id = ctx.accounts.swap_program.key();

    // Anchor-style 8-byte discriminator for `shared_accounts_route` — pulled
    // from the mock-jupiter crate so it stays in sync with the program.
    // Real Jupiter v6 uses the same sighash scheme; swap this for the real
    // Jupiter discriminator when targeting their mainnet program.
    let discriminator: &[u8] =
        mock_jupiter::instruction::SharedAccountsRoute::DISCRIMINATOR;

    // Argument layout: id (u8), route_plan_len (u8), in_amount (u64),
    // quoted_out_amount (u64), slippage_bps (u16), platform_fee_bps (u8).
    // The mock ignores everything except `in_amount`; real Jupiter requires
    // accurate values for the others.
    let mut instruction_data = Vec::with_capacity(8 + 1 + 1 + 8 + 8 + 2 + 1);
    instruction_data.extend_from_slice(discriminator);
    instruction_data.push(0u8); // id
    instruction_data.push(1u8); // route_plan_len — one hop in the mock
    instruction_data.extend_from_slice(&in_amount.to_le_bytes());
    // quoted_out_amount: 0 means "no quote expectation" for the mock; real
    // Jupiter would reject this.
    instruction_data.extend_from_slice(&0u64.to_le_bytes());
    instruction_data.extend_from_slice(&0u16.to_le_bytes()); // slippage_bps
    instruction_data.push(0u8); // platform_fee_bps

    let metas = vec![
        AccountMeta::new_readonly(ctx.accounts.token_program.key(), false),
        // user_transfer_authority — the vault PDA signs for itself.
        AccountMeta::new_readonly(ctx.accounts.vault.key(), true),
        AccountMeta::new(ctx.accounts.vault_volatile_account.key(), false),
        AccountMeta::new(ctx.accounts.pool_volatile_account.key(), false),
        AccountMeta::new(ctx.accounts.pool_stable_account.key(), false),
        AccountMeta::new(ctx.accounts.vault_stable_account.key(), false),
        AccountMeta::new_readonly(ctx.accounts.oracle_feed.key(), false),
        AccountMeta::new_readonly(ctx.accounts.volatile_mint.key(), false),
        AccountMeta::new_readonly(ctx.accounts.stable_mint.key(), false),
        AccountMeta::new_readonly(ctx.accounts.pool_authority.key(), false),
    ];

    let instruction = Instruction {
        program_id: mock_jupiter_program_id,
        accounts: metas,
        data: instruction_data,
    };

    let owner_key = ctx.accounts.vault.owner;
    let bump = ctx.accounts.vault.bump;
    let signer_seeds: &[&[&[u8]]] =
        &[&[Vault::SEED_PREFIX, owner_key.as_ref(), &[bump]]];

    invoke_signed(
        &instruction,
        &[
            ctx.accounts.token_program.to_account_info(),
            ctx.accounts.vault.to_account_info(),
            ctx.accounts.vault_volatile_account.to_account_info(),
            ctx.accounts.pool_volatile_account.to_account_info(),
            ctx.accounts.pool_stable_account.to_account_info(),
            ctx.accounts.vault_stable_account.to_account_info(),
            ctx.accounts.oracle_feed.to_account_info(),
            ctx.accounts.volatile_mint.to_account_info(),
            ctx.accounts.stable_mint.to_account_info(),
            ctx.accounts.pool_authority.to_account_info(),
            ctx.accounts.swap_program.to_account_info(),
        ],
        signer_seeds,
    )?;

    ctx.accounts.vault.triggered = true;
    Ok(())
}

#[derive(Accounts)]
pub struct ConvertIfTriggeredAccountConstraints<'info> {
    /// PDA holding the volatile stash. Note: NO `has_one = owner` here — the
    /// crank is permissionless. The owner field is read for the signer seeds
    /// only.
    #[account(
        mut,
        seeds = [Vault::SEED_PREFIX, vault.owner.as_ref()],
        bump = vault.bump,
        has_one = volatile_mint,
        has_one = stable_mint,
        has_one = oracle_feed,
    )]
    pub vault: Box<Account<'info, Vault>>,

    // Heavy `Account<...>` wrappers are boxed to keep this constraints
    // struct off the BPF stack. Without these boxes the generated
    // `try_accounts` function exceeds the 4096-byte SBF stack frame.
    pub volatile_mint: Box<Account<'info, Mint>>,

    pub stable_mint: Box<Account<'info, Mint>>,

    /// CHECK: matched against `vault.oracle_feed` via `has_one`; layout is
    /// validated when the data is read.
    pub oracle_feed: UncheckedAccount<'info>,

    #[account(
        mut,
        associated_token::mint = volatile_mint,
        associated_token::authority = vault,
    )]
    pub vault_volatile_account: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = stable_mint,
        associated_token::authority = vault,
    )]
    pub vault_stable_account: Box<Account<'info, TokenAccount>>,

    /// Mock pool's input-mint ATA (volatile token). Owned by the swap
    /// program's pool authority.
    #[account(
        mut,
        token::mint = volatile_mint,
    )]
    pub pool_volatile_account: Box<Account<'info, TokenAccount>>,

    /// Mock pool's output-mint ATA (stable token).
    #[account(
        mut,
        token::mint = stable_mint,
    )]
    pub pool_stable_account: Box<Account<'info, TokenAccount>>,

    /// CHECK: pool authority PDA; validated by the swap program.
    pub pool_authority: UncheckedAccount<'info>,

    /// CHECK: swap program. In tests this is `mock_jupiter::ID`; in
    /// production replace it with Jupiter v6's program ID.
    pub swap_program: UncheckedAccount<'info>,

    /// Anyone signs and pays — typically a TukTuk worker. This is the
    /// permissionless entry point.
    #[account(mut)]
    pub cranker: Signer<'info>,

    pub token_program: Program<'info, Token>,
}
