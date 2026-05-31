//! Mock Jupiter v6 swap aggregator for testing the stop-loss vault.
//!
//! Real Jupiter aggregates across many AMMs and routes through possibly
//! multiple pools. The instruction that the vault uses against real Jupiter is
//! `shared_accounts_route` — a single permissioned route through Jupiter's
//! shared program-owned accounts.
//!
//! This mock implements ONE instruction with the same external shape (an
//! 8-byte Anchor-style discriminator + a borsh argument struct + a fixed
//! account list head). Instead of actually routing through DEXes, the mock:
//!
//!   1. Reads the current price from a mock Switchboard feed passed in
//!      remaining accounts.
//!   2. Transfers `in_amount` of the input mint from the user's source ATA to
//!      the mock liquidity pool's input ATA.
//!   3. Transfers `in_amount * price / 10^scale` adjusted for decimal
//!      differences of the output mint from the mock pool's output ATA back
//!      to the user's destination ATA.
//!
//! This is enough to exercise the vault's swap path in tests. NOT FOR
//! PRODUCTION — real Jupiter swaps go through real liquidity, real price
//! impact, real slippage, and real route accounts.
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, TransferChecked};

declare_id!("DSMyed6WZ2US8nfwLQtF7en9jcd9exn7c4qQd52Nffx1");

#[program]
pub mod mock_jupiter {
    use super::*;

    /// Mock of Jupiter v6's `shared_accounts_route`. Same argument layout, same
    /// account order at the head, but executes a deterministic price multiply
    /// instead of a real route. The `_route_plan_len`, `_quoted_out_amount`,
    /// `_slippage_bps` and `_platform_fee_bps` arguments are accepted but
    /// ignored — the mock's "route" is the single Switchboard price.
    pub fn shared_accounts_route(
        ctx: Context<SharedAccountsRoute>,
        _id: u8,
        _route_plan_len: u8,
        in_amount: u64,
        _quoted_out_amount: u64,
        _slippage_bps: u16,
        _platform_fee_bps: u8,
    ) -> Result<()> {
        // Decode the mock Switchboard feed from the dedicated account slot.
        // Anchor account layout is `[8-byte discriminator | borsh struct]`.
        // The vault passes the same feed account here as it does for its own
        // pre-flight price check, so prices are consistent.
        let feed_account = &ctx.accounts.price_feed;
        let feed_data = feed_account.try_borrow_data()?;
        require!(
            feed_data.len()
                >= MOCK_FEED_DISCRIMINATOR_LENGTH + MOCK_FEED_PAYLOAD_LENGTH,
            MockJupiterError::FeedDataTooShort
        );
        // Skip the 8-byte Anchor discriminator and decode the fixed-layout
        // payload: 32 (authority) + 16 (price i128) + 4 (scale u32) +
        // 8 (last_update_slot u64).
        let payload =
            &feed_data[MOCK_FEED_DISCRIMINATOR_LENGTH..MOCK_FEED_DISCRIMINATOR_LENGTH
                + MOCK_FEED_PAYLOAD_LENGTH];
        let price_bytes: [u8; 16] = payload[32..48]
            .try_into()
            .map_err(|_| MockJupiterError::FeedDataTooShort)?;
        let price = i128::from_le_bytes(price_bytes);
        let scale_bytes: [u8; 4] = payload[48..52]
            .try_into()
            .map_err(|_| MockJupiterError::FeedDataTooShort)?;
        let scale = u32::from_le_bytes(scale_bytes);
        drop(feed_data);

        require!(price > 0, MockJupiterError::NonPositivePrice);

        // Pull the user's volatile tokens into the mock pool.
        let cpi_in = CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.source_token_account.to_account_info(),
                mint: ctx.accounts.input_mint_decimals.to_account_info(),
                to: ctx.accounts.program_source_token_account.to_account_info(),
                authority: ctx.accounts.user_transfer_authority.to_account_info(),
            },
        );
        token::transfer_checked(cpi_in, in_amount, ctx.accounts.input_mint_decimals.decimals)?;

        // Compute the stable amount the user receives.
        //
        // `price` has `scale` decimal places (e.g. scale=8, price=200_00000000
        // means $200). `in_amount` is in the input mint's smallest units
        // (e.g. lamports for SOL). The output token has its own decimals on
        // its mint; the caller passes them explicitly so this mock can scale
        // correctly without doing a CPI to the mint.
        //
        // out_amount = in_amount * price * 10^output_decimals
        //                       / (10^scale * 10^input_decimals)
        let in_decimals = ctx.accounts.input_mint_decimals.decimals as u32;
        let out_decimals = ctx.accounts.output_mint_decimals.decimals as u32;

        let in_amount_u128 = in_amount as u128;
        let price_u128 = u128::try_from(price)
            .map_err(|_| MockJupiterError::NonPositivePrice)?;
        let numerator = in_amount_u128
            .checked_mul(price_u128)
            .ok_or(MockJupiterError::MathOverflow)?
            .checked_mul(ten_pow(out_decimals)?)
            .ok_or(MockJupiterError::MathOverflow)?;
        let denominator = ten_pow(scale)?
            .checked_mul(ten_pow(in_decimals)?)
            .ok_or(MockJupiterError::MathOverflow)?;
        let out_amount_u128 = numerator
            .checked_div(denominator)
            .ok_or(MockJupiterError::MathOverflow)?;
        let out_amount: u64 = out_amount_u128
            .try_into()
            .map_err(|_| MockJupiterError::MathOverflow)?;

        // Push the stable tokens back to the user from the mock pool.
        // The pool ATA is owned by a PDA so we sign for it.
        let pool_authority_bump = ctx.bumps.pool_authority;
        let signer_seeds: &[&[&[u8]]] =
            &[&[POOL_AUTHORITY_SEED, &[pool_authority_bump]]];
        let cpi_out = CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx
                    .accounts
                    .program_destination_token_account
                    .to_account_info(),
                mint: ctx.accounts.output_mint_decimals.to_account_info(),
                to: ctx.accounts.destination_token_account.to_account_info(),
                authority: ctx.accounts.pool_authority.to_account_info(),
            },
            signer_seeds,
        );
        token::transfer_checked(cpi_out, out_amount, ctx.accounts.output_mint_decimals.decimals)?;
        Ok(())
    }

    /// Convenience instruction so tests can derive a stable PDA-owned pool
    /// authority without rolling their own keypair scheme. Not part of the
    /// Jupiter API surface.
    pub fn initialize_pool_authority(_ctx: Context<InitializePoolAuthority>) -> Result<()> {
        Ok(())
    }
}

/// 8-byte Anchor discriminator length. Anchor accounts and Anchor instructions
/// both prefix their serialised data with an 8-byte discriminator, so this
/// constant is shared.
pub const MOCK_FEED_DISCRIMINATOR_LENGTH: usize = 8;
/// Fixed payload length of `mock_switchboard::MockFeed`:
/// 32 (authority Pubkey) + 16 (price i128) + 4 (scale u32) + 8 (last_update_slot u64).
pub const MOCK_FEED_PAYLOAD_LENGTH: usize = 32 + 16 + 4 + 8;

/// PDA seed for the mock pool authority. Tests fund the mock pool ATAs owned
/// by this PDA so the pool has stables to disburse.
pub const POOL_AUTHORITY_SEED: &[u8] = b"mock-jupiter-pool";

fn ten_pow(power: u32) -> Result<u128> {
    10u128
        .checked_pow(power)
        .ok_or_else(|| error!(MockJupiterError::MathOverflow))
}

/// Stub PDA the mock pool ATAs are owned by. Holds no state; existence makes
/// it a valid signer authority for token-transfer CPIs out of pool ATAs.
#[account]
pub struct PoolAuthority {}

#[derive(Accounts)]
pub struct InitializePoolAuthority<'info> {
    /// CHECK: PDA derived from POOL_AUTHORITY_SEED; never read or written.
    /// Existence as an account is incidental — Anchor still requires us to
    /// declare it, but it doesn't need any data.
    #[account(
        seeds = [POOL_AUTHORITY_SEED],
        bump,
    )]
    pub pool_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SharedAccountsRoute<'info> {
    pub token_program: Program<'info, Token>,

    /// User signing for the swap. In Jupiter this is `userTransferAuthority`;
    /// for the vault path this will be the vault PDA signing for itself.
    pub user_transfer_authority: Signer<'info>,

    /// User's source token account (vault's volatile ATA for our use).
    #[account(mut)]
    pub source_token_account: Box<Account<'info, TokenAccount>>,

    /// Mock pool's input token account (receives `in_amount`).
    #[account(mut)]
    pub program_source_token_account: Box<Account<'info, TokenAccount>>,

    /// Mock pool's output token account (pays out the stable).
    #[account(mut)]
    pub program_destination_token_account: Box<Account<'info, TokenAccount>>,

    /// User's destination token account (vault's stable ATA for our use).
    #[account(mut)]
    pub destination_token_account: Box<Account<'info, TokenAccount>>,

    /// CHECK: read-only price feed; payload layout is validated when read.
    pub price_feed: UncheckedAccount<'info>,

    /// Decimal-only view of input mint. We just need `decimals`.
    pub input_mint_decimals: Box<Account<'info, anchor_spl::token::Mint>>,
    /// Decimal-only view of output mint.
    pub output_mint_decimals: Box<Account<'info, anchor_spl::token::Mint>>,

    /// CHECK: PDA that owns the pool ATAs.
    #[account(
        seeds = [POOL_AUTHORITY_SEED],
        bump,
    )]
    pub pool_authority: UncheckedAccount<'info>,
}

#[error_code]
pub enum MockJupiterError {
    #[msg("Mock Switchboard feed account data is shorter than expected.")]
    FeedDataTooShort,
    #[msg("Mock Switchboard feed reported a non-positive price.")]
    NonPositivePrice,
    #[msg("Math overflow while computing swap output.")]
    MathOverflow,
}
