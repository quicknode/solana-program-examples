use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::constants::{
    AUTHORITY_SEED, BASIS_POINTS_DENOMINATOR, MARKET_SEED, QUOTE_VAULT_SEED, UNDERLYING_VAULT_SEED,
};
use crate::errors::OptionsError;
use crate::state::Market;

pub fn handle_initialize_market(
    context: Context<InitializeMarketAccountConstraints>,
    fee_bps: u16,
) -> Result<()> {
    // Options on a token settled in the same token are meaningless.
    require_keys_neq!(
        context.accounts.underlying_mint.key(),
        context.accounts.quote_mint.key(),
        OptionsError::InvalidParameter
    );
    // A fee of 100% or more would leave the writer nothing from a sale. Zero
    // is allowed: a venue run at cost is a valid choice.
    require!(
        fee_bps < BASIS_POINTS_DENOMINATOR as u16,
        OptionsError::InvalidParameter
    );

    let market = &mut context.accounts.market;
    market.admin = context.accounts.admin.key();
    market.underlying_mint = context.accounts.underlying_mint.key();
    market.quote_mint = context.accounts.quote_mint.key();
    market.underlying_vault = context.accounts.underlying_vault.key();
    market.quote_vault = context.accounts.quote_vault.key();
    market.underlying_locked = 0;
    market.quote_locked = 0;
    market.fees_owed = 0;
    market.fee_bps = fee_bps;
    market.bump = context.bumps.market;
    market.authority_bump = context.bumps.market_authority;

    Ok(())
}

#[derive(Accounts)]
pub struct InitializeMarketAccountConstraints<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    // One venue per pair, so every option on NVDAx settled in USDC shares the
    // two vaults and the one fee schedule.
    #[account(
        init,
        payer = admin,
        space = Market::DISCRIMINATOR.len() + Market::INIT_SPACE,
        seeds = [MARKET_SEED, underlying_mint.key().as_ref(), quote_mint.key().as_ref()],
        bump,
    )]
    pub market: Box<Account<'info, Market>>,

    pub underlying_mint: Box<InterfaceAccount<'info, Mint>>,

    pub quote_mint: Box<InterfaceAccount<'info, Mint>>,

    /// CHECK: PDA that owns both vaults. Holds no data; used only to sign
    /// vault CPIs.
    #[account(
        seeds = [AUTHORITY_SEED, market.key().as_ref()],
        bump,
    )]
    pub market_authority: UncheckedAccount<'info>,

    #[account(
        init,
        payer = admin,
        seeds = [UNDERLYING_VAULT_SEED, market.key().as_ref()],
        bump,
        token::mint = underlying_mint,
        token::authority = market_authority,
        token::token_program = token_program,
    )]
    pub underlying_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init,
        payer = admin,
        seeds = [QUOTE_VAULT_SEED, market.key().as_ref()],
        bump,
        token::mint = quote_mint,
        token::authority = market_authority,
        token::token_program = token_program,
    )]
    pub quote_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}
