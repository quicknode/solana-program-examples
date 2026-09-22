use anchor_lang::prelude::*;
use anchor_spl::mint;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    constants::{CONFIG_SEED, LIQUIDITY_SEED},
    errors::AmmError,
    state::{Config, PoolConfig},
};

pub fn handle_initialize_pool(
    context: &mut Context<InitializePoolAccountConstraints>,
) -> Result<()> {
    let bump = context.bumps.pool_config;
    let pool_config = &mut context.accounts.pool_config;
    pool_config.config = *context.accounts.config.address();
    pool_config.mint_a = *context.accounts.mint_a.address();
    pool_config.mint_b = *context.accounts.mint_b.address();
    pool_config.bump = bump;

    Ok(())
}

#[derive(Accounts)]
pub struct InitializePoolAccountConstraints {
    #[account(
        seeds = [CONFIG_SEED],
        bump,
    )]
    pub config: Box<BorshAccount<Config>>,

    #[account(
        init,
        payer = payer,
        space = PoolConfig::DISCRIMINATOR.len() + PoolConfig::INIT_SPACE,
        seeds = [
            config.address().as_ref(),
            mint_a.address().as_ref(),
            mint_b.address().as_ref(),
        ],
        bump,
        constraint = mint_a.address() < mint_b.address() @ AmmError::InvalidMintOrder,
    )]
    pub pool_config: Box<BorshAccount<PoolConfig>>,

    /// The LP mint. `pool_config` is its mint authority and signs every
    /// mint_to with its own seeds.
    #[account(
        init,
        payer = payer,
        seeds = [
            config.address().as_ref(),
            mint_a.address().as_ref(),
            mint_b.address().as_ref(),
            LIQUIDITY_SEED,
        ],
        bump,
        mint::decimals = 6,
        mint::authority = pool_config,
        // Required when the token program is an `Interface`: without it the
        // init CPI is rejected with InvalidArgument.
        mint::token_program = token_program,
    )]
    pub liquidity_provider_mint: Box<InterfaceAccount<Mint>>,

    pub mint_a: Box<InterfaceAccount<Mint>>,

    pub mint_b: Box<InterfaceAccount<Mint>>,

    /// The pool's token-A reserve: the associated token account of
    /// `pool_config`, which signs every transfer out of it.
    #[account(
        init,
        payer = payer,
        associated_token::mint = mint_a,
        associated_token::authority = pool_config,
        associated_token::token_program = token_program,
    )]
    pub pool_a: Box<InterfaceAccount<TokenAccount>>,

    /// The pool's token-B reserve, likewise owned by `pool_config`.
    #[account(
        init,
        payer = payer,
        associated_token::mint = mint_b,
        associated_token::authority = pool_config,
        associated_token::token_program = token_program,
    )]
    pub pool_b: Box<InterfaceAccount<TokenAccount>>,

    /// The account paying for all rents
    #[account(mut)]
    pub payer: Signer,

    /// Solana ecosystem accounts
    pub token_program: Interface<'static, TokenInterface>,
    pub associated_token_program: Program<AssociatedToken>,
    pub system_program: Program<System>,
}
